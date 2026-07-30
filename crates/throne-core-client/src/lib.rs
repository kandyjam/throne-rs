//! Client for the legacy Go `ThroneCore` process.
//!
//! Protocol (matches `core/server/dispatch.go` + Qt `RPC.cpp`):
//! - GUI listens on a unix domain socket
//! - Core connects (env `THRONE_CORE_SOCKET` = full path)
//! - Request:  `[u32 le reqId][u16 le methodLen][method][u32 le payloadLen][payload]`
//! - Response: `[u32 le reqId][u8 status][u32 le dataLen][data]`
//! - Payload = protobuf (`LoadConfigReq` / `EmptyReq` / `ErrorResp`)

mod config_build;
mod proto_wire;
mod rule_set_list;
mod sys_proxy;

pub use config_build::{
    BuiltConfig, apply_ruleset_mirror, build_load_config, build_url_test_config,
};
pub use proto_wire::{ConnectionRow, IpTestResult, SpeedTestResult, UrlTestResult};
pub use sys_proxy::{force_clear_system_proxy, set_system_proxy};

use std::io::{Read, Write};
use std::net::Shutdown;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use thiserror::Error;
use tracing::{info, warn};

use throne_domain::{AppSettings, Profile, ProfileId, TrafficSnapshot};

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("core binary not found (looked for ThroneCore / Core). Set THRONE_CORE or place binary next to the app.\nSearched: {0}")]
    BinaryMissing(String),
    #[error("failed to spawn core: {0}")]
    Spawn(String),
    #[error("IPC listen failed: {0}")]
    IpcListen(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("core RPC error: {0}")]
    Rpc(String),
    #[error("core is not running")]
    NotRunning,
    #[error("core config error: {0}")]
    Config(String),
    #[error("timeout waiting for core IPC connection")]
    ConnectTimeout,
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

/// Shorten core error text for UI: `decode config at {huge json}: real reason`
/// often dumps the entire config before the useful cause.
pub fn format_core_error(err: &str) -> String {
    const MARKER: &str = "decode config at ";
    if let Some(idx) = err.find(MARKER) {
        let after = &err[idx + MARKER.len()..];
        // Cause is after the JSON blob, usually `\n: reason` or `: reason`.
        if let Some(cause) = after
            .rsplit_once("\n:")
            .map(|(_, c)| c.trim())
            .or_else(|| {
                // Fallback: last ": " that looks like a prose reason (not JSON).
                after
                    .rmatch_indices(": ")
                    .find(|(i, _)| {
                        let rest = after.get(i + 2..).unwrap_or("");
                        !rest.starts_with('{') && !rest.starts_with('[') && rest.len() < 400
                    })
                    .map(|(i, _)| after[i + 2..].trim())
            })
        {
            if !cause.is_empty() {
                return format!("decode config: {cause}");
            }
        }
    }
    // Soft cap for other huge messages.
    if err.len() > 280 {
        format!("{}…", &err[..277])
    } else {
        err.to_string()
    }
}

/// Configuration for locating / launching the Go core.
#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub binary_path: PathBuf,
    pub work_dir: PathBuf,
    pub extra_args: Vec<String>,
    /// Directory for Xray geo assets (`XRAY_LOCATION_ASSET`).
    pub asset_dir: PathBuf,
}

impl Default for CoreConfig {
    fn default() -> Self {
        let work = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            binary_path: resolve_core_binary(),
            work_dir: work.clone(),
            extra_args: Vec::new(),
            asset_dir: work,
        }
    }
}

/// Resolve ThroneCore / Core binary path.
pub fn resolve_core_binary() -> PathBuf {
    if let Ok(p) = std::env::var("THRONE_CORE") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return pb;
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("ThroneCore"));
            candidates.push(dir.join("Core"));
            candidates.push(dir.join("nekobox_core"));
            // cargo run: target/debug/throne → also check workspace root / core build outs
            if let Some(target) = dir.parent() {
                candidates.push(target.join("ThroneCore"));
                candidates.push(target.join("Core"));
                if let Some(ws) = target.parent() {
                    candidates.push(ws.join("ThroneCore"));
                    candidates.push(ws.join("Core"));
                    candidates.push(ws.join("build").join("ThroneCore"));
                }
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("ThroneCore"));
        candidates.push(cwd.join("Core"));
        candidates.push(cwd.join("build").join("ThroneCore"));
    }
    // Installed Qt Throne.app (macOS) — reuse its bundled core.
    candidates.push(PathBuf::from(
        "/Applications/Throne.app/Contents/MacOS/ThroneCore",
    ));
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(
            PathBuf::from(home).join("Applications/Throne.app/Contents/MacOS/ThroneCore"),
        );
    }
    for c in &candidates {
        if c.exists() {
            return c.clone();
        }
    }
    // Fall back to PATH name
    PathBuf::from("ThroneCore")
}

pub fn core_binary_available(config: &CoreConfig) -> bool {
    config.binary_path.exists()
        || which_in_path("ThroneCore").is_some()
        || which_in_path("Core").is_some()
}

fn which_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Request payload for loading a profile into the core (maps to `LoadConfigReq`).
#[derive(Debug, Clone)]
pub struct LoadConfigRequest {
    pub profile_id: ProfileId,
    pub core_config_json: String,
    pub need_xray: bool,
    pub xray_config: String,
    pub disable_stats: bool,
}

/// Long-lived core session: IPC server + child process + RPC stream.
pub struct CoreSession {
    config: CoreConfig,
    child: Option<Child>,
    #[cfg(unix)]
    listener: Option<UnixListener>,
    #[cfg(unix)]
    stream: Option<UnixStream>,
    socket_path: Option<PathBuf>,
    next_id: AtomicU32,
    connected: bool,
    running_profile: Option<ProfileId>,
}

impl CoreSession {
    pub fn new(config: CoreConfig) -> Self {
        Self {
            config,
            child: None,
            #[cfg(unix)]
            listener: None,
            #[cfg(unix)]
            stream: None,
            socket_path: None,
            next_id: AtomicU32::new(1),
            connected: false,
            running_profile: None,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn running_profile_id(&self) -> Option<ProfileId> {
        self.running_profile
    }

    /// Ensure IPC listener + core process are up and RPC is connected.
    pub fn ensure_connected(&mut self) -> Result<(), CoreError> {
        if self.connected {
            // Probe: if stream is dead, reconnect.
            if self.ping_ok() {
                return Ok(());
            }
            self.connected = false;
            #[cfg(unix)]
            {
                self.stream = None;
            }
        }
        self.start_ipc_and_core()?;
        Ok(())
    }

    fn ping_ok(&mut self) -> bool {
        // Prefer a cheap no-op RPC. QueryStats is heavier (clash manager walk).
        // EmptyReq methods: IsPrivileged is fast and always registered.
        self.call(
            "IsPrivileged",
            &proto_wire::encode_empty_req(),
            Duration::from_millis(400),
        )
        .is_ok()
            || self
                .call(
                    "QueryStats",
                    &proto_wire::encode_empty_req(),
                    Duration::from_millis(800),
                )
                .is_ok()
    }

    #[cfg(unix)]
    fn start_ipc_and_core(&mut self) -> Result<(), CoreError> {
        // Clean previous
        self.shutdown_inner();

        // Upstream parentcheck (darwin/linux): parent basename must be "Throne"
        // and ThroneCore must live in the *same directory* as the GUI binary.
        let bin = prepare_core_beside_gui(&self.config)?;
        let work_dir = bin
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| self.config.work_dir.clone());

        let sock_name = format!("throneIPC-{}", uuid_simple());
        // Keep path short (unix sun_path limit); prefer /tmp on macOS.
        let socket_path = PathBuf::from("/tmp").join(&sock_name);
        let _ = std::fs::remove_file(&socket_path);

        let listener = UnixListener::bind(&socket_path).map_err(|e| {
            CoreError::IpcListen(format!("{} ({e})", socket_path.display()))
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|e| CoreError::IpcListen(e.to_string()))?;
        self.listener = Some(listener);
        self.socket_path = Some(socket_path.clone());

        info!(
            path = %bin.display(),
            cwd = %work_dir.display(),
            socket = %socket_path.display(),
            "spawning ThroneCore"
        );

        let mut cmd = Command::new(&bin);
        cmd.current_dir(&work_dir)
            .args(&self.config.extra_args)
            .env("THRONE_CORE_SOCKET", &socket_path)
            .env("XRAY_LOCATION_ASSET", &self.config.asset_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Parentcheck expects real parent PID = this process (named Throne).
        let child = cmd.spawn().map_err(|e| {
            CoreError::Spawn(format!(
                "{e} — tried {} (ensure GUI binary is named Throne and core sits beside it)",
                bin.display()
            ))
        })?;
        self.child = Some(child);

        // Wait for core to connect (up to ~8s, matching core's 10×500ms retries)
        let deadline = Instant::now() + Duration::from_secs(8);
        let stream = loop {
            if Instant::now() > deadline {
                let hint = self
                    .child
                    .as_mut()
                    .map(read_child_stderr)
                    .unwrap_or_default();
                self.shutdown_inner();
                return Err(CoreError::Rpc(format!(
                    "timeout waiting for core IPC. {hint}"
                )));
            }
            // Reap early exit
            if let Some(child) = self.child.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    let stderr = read_child_stderr(child);
                    self.shutdown_inner();
                    let hint = if stderr.contains("parent check") {
                        "\nHint: GUI binary must be named `Throne` and ThroneCore must be in the same folder (upstream parentcheck)."
                    } else {
                        ""
                    };
                    return Err(CoreError::Rpc(format!(
                        "ThroneCore exited early ({status}): {stderr}{hint}"
                    )));
                }
            }
            match self.listener.as_ref().unwrap().accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    self.shutdown_inner();
                    return Err(CoreError::IpcListen(e.to_string()));
                }
            }
        };

        stream
            .set_nonblocking(false)
            .map_err(|e| CoreError::IpcListen(e.to_string()))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| CoreError::IpcListen(e.to_string()))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| CoreError::IpcListen(e.to_string()))?;
        self.stream = Some(stream);
        self.connected = true;
        info!("core IPC connected");
        Ok(())
    }

    #[cfg(not(unix))]
    fn start_ipc_and_core(&mut self) -> Result<(), CoreError> {
        Err(CoreError::NotImplemented("Windows named-pipe IPC"))
    }

    /// Start a profile: build config, ensure core, Start RPC, optional system proxy.
    ///
    /// `route_profile` (e.g. Bypass China) is compiled into `route.rules` /
    /// `route.rule_set`. Pass `None` for minimal sniff + private→direct defaults.
    pub fn start_profile(
        &mut self,
        profile: &Profile,
        settings: &AppSettings,
        route_profile: Option<&throne_domain::RouteProfile>,
        apply_system_proxy: bool,
    ) -> Result<(), CoreError> {
        let built = build_load_config(profile, settings, route_profile)?;
        self.ensure_connected()?;

        // Keep-alive path: a previous Stop may have left boxInstance up, or a
        // half-failed Start. Always Stop first so Start is clean.
        if self.running_profile.is_some() || self.connected {
            let _ = self.call(
                "Stop",
                &proto_wire::encode_empty_req(),
                Duration::from_secs(2),
            );
            self.running_profile = None;
        }

        let payload = proto_wire::encode_load_config_req(
            &built.core_config_json,
            false,
            built.need_xray,
            &built.xray_config,
        );
        let resp = match self.call("Start", &payload, Duration::from_secs(30)) {
            Ok(r) => r,
            Err(e) => {
                // Don't leave system proxy pointing at a dead port.
                force_clear_system_proxy();
                return Err(e);
            }
        };
        let err = proto_wire::decode_error_resp(&resp)?;
        if !err.is_empty() {
            // "instance already started" → recycle core once and retry.
            if err.contains("already started") {
                warn!("Start: instance already started — recycling core");
                self.force_kill_core();
                self.ensure_connected()?;
                let resp2 = self.call("Start", &payload, Duration::from_secs(30))?;
                let err2 = proto_wire::decode_error_resp(&resp2)?;
                if !err2.is_empty() {
                    force_clear_system_proxy();
                    return Err(CoreError::Rpc(format_core_error(&err2)));
                }
            } else {
                force_clear_system_proxy();
                return Err(CoreError::Rpc(format_core_error(&err)));
            }
        }

        self.running_profile = Some(profile.id);

        if apply_system_proxy || settings.system_proxy_enabled {
            let host = if settings.inbound_address.trim().is_empty() {
                "127.0.0.1"
            } else {
                settings.inbound_address.trim()
            };
            if let Err(e) = set_system_proxy(true, host, settings.inbound_socks_port) {
                warn!(%e, "system proxy enable failed (core is still running)");
            }
        }

        info!(
            profile = profile.id,
            port = settings.inbound_socks_port,
            "core Start OK"
        );
        Ok(())
    }

    /// Stop the running profile.
    ///
    /// Order is intentional for UI responsiveness:
    /// 1. Clear system proxy first (browsers unblock even if core is wedged)
    /// 2. Stop RPC with a short timeout
    /// 3. **Keep ThroneCore alive** for the next Start (cold spawn is the main lag).
    ///    Only force-kill when Stop RPC fails (wedged core).
    pub fn stop_profile(&mut self, settings: &AppSettings) -> Result<(), CoreError> {
        // 1) Always drop system proxy first — networksetup must not run after a
        //    long blocked Stop, or the UI feels frozen for 15s+.
        let host = if settings.inbound_address.trim().is_empty() {
            "127.0.0.1"
        } else {
            settings.inbound_address.trim()
        };
        // Clear proxy thoroughly — leftover system proxy to :2080 = no web at all.
        if let Err(e) = set_system_proxy(false, host, settings.inbound_socks_port) {
            warn!(%e, "system proxy clear on stop failed — force clear all services");
            force_clear_system_proxy();
        }

        // 2) Stop RPC — sing-box CloseWithTimeout is ~2s server-side.
        //    Prefer keep-alive: Qt Throne also reuses the core process.
        let mut kill = false;
        if self.connected {
            let payload = proto_wire::encode_empty_req();
            match self.call("Stop", &payload, Duration::from_secs(2)) {
                Ok(resp) => {
                    if let Ok(err) = proto_wire::decode_error_resp(&resp) {
                        if !err.is_empty() {
                            warn!(%err, "core Stop returned error");
                            // Instance may be half-dead; recycle process.
                            if err.contains("not") || err.contains("panic") {
                                kill = true;
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(%e, "core Stop RPC failed (will force-kill)");
                    kill = true;
                }
            }
        } else {
            kill = self.child.is_some();
        }
        self.running_profile = None;

        if kill {
            self.force_kill_core();
        }
        Ok(())
    }

    /// Kill core process and drop IPC without waiting forever.
    pub fn force_kill_core(&mut self) {
        self.shutdown_inner();
    }

    /// URL-test the currently running instance's `proxy` outbound.
    pub fn url_test_current(&mut self, url: &str, timeout_ms: i32) -> Result<UrlTestResult, CoreError> {
        self.ensure_connected()?;
        if self.running_profile.is_none() {
            return Err(CoreError::NotRunning);
        }
        let payload = proto_wire::encode_test_req(
            "",
            &[],
            url,
            true,  // test_current
            false, // use_default_outbound
            1,
            timeout_ms.max(1000),
        );
        let resp = self.call("Test", &payload, Duration::from_secs(60))?;
        let results = proto_wire::decode_test_resp(&resp)?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| CoreError::Rpc("empty Test response".into()))
    }

    /// Batch URL-test profiles by spinning a temporary box (does not disturb running instance).
    pub fn url_test_profiles(
        &mut self,
        profiles: &[&Profile],
        settings: &AppSettings,
    ) -> Result<Vec<(ProfileId, UrlTestResult)>, CoreError> {
        if profiles.is_empty() {
            return Ok(Vec::new());
        }
        let (config, tags) = build_url_test_config(profiles, settings)?;
        self.ensure_connected()?;
        let url = if settings.test_latency_url.trim().is_empty() {
            "https://www.gstatic.com/generate_204"
        } else {
            settings.test_latency_url.trim()
        };
        let payload = proto_wire::encode_test_req(
            &config,
            &tags,
            url,
            false,
            false,
            8,
            8000,
        );
        // Batch can take a while with many nodes.
        let resp = self.call(
            "Test",
            &payload,
            Duration::from_secs(30 + 10 * profiles.len() as u64),
        )?;
        let results = proto_wire::decode_test_resp(&resp)?;
        let mut out = Vec::with_capacity(results.len());
        for r in results {
            let id = r
                .outbound_tag
                .strip_prefix('p')
                .and_then(|s| s.parse::<ProfileId>().ok())
                .unwrap_or(0);
            out.push((id, r));
        }
        Ok(out)
    }

    pub fn query_stats(&mut self) -> Result<TrafficSnapshot, CoreError> {
        if !self.connected || self.running_profile.is_none() {
            return Err(CoreError::NotRunning);
        }
        let resp =
            self.call("QueryStats", &proto_wire::encode_empty_req(), Duration::from_secs(5))?;
        let (ups, downs) = proto_wire::decode_query_stats_resp(&resp)?;
        let proxy_up = ups.get("proxy").copied().unwrap_or(0);
        let proxy_down = downs.get("proxy").copied().unwrap_or(0);
        let direct_up = ups.get("direct").copied().unwrap_or(0);
        let direct_down = downs.get("direct").copied().unwrap_or(0);
        Ok(TrafficSnapshot {
            proxy_up,
            proxy_down,
            direct_up,
            direct_down,
        })
    }

    pub fn query_connections(&mut self) -> Result<Vec<ConnectionRow>, CoreError> {
        if !self.connected || self.running_profile.is_none() {
            return Err(CoreError::NotRunning);
        }
        let resp = self.call(
            "QueryConnections",
            &proto_wire::encode_empty_req(),
            Duration::from_secs(5),
        )?;
        let (active, _closed) = proto_wire::decode_query_connections_resp(&resp)?;
        Ok(active)
    }

    /// IP / country lookup for profiles (temporary box) or empty tags = default outbound of config.
    pub fn ip_test_profiles(
        &mut self,
        profiles: &[&Profile],
        settings: &AppSettings,
    ) -> Result<Vec<(ProfileId, IpTestResult)>, CoreError> {
        if profiles.is_empty() {
            return Ok(Vec::new());
        }
        let (config, tags) = build_url_test_config(profiles, settings)?;
        self.ensure_connected()?;
        let payload = proto_wire::encode_ip_test_req(&config, &tags, false, 8, 10000);
        let resp = self.call(
            "IPTest",
            &payload,
            Duration::from_secs(30 + 8 * profiles.len() as u64),
        )?;
        let results = proto_wire::decode_ip_test_resp(&resp)?;
        let mut out = Vec::with_capacity(results.len());
        for r in results {
            let id = r
                .outbound_tag
                .strip_prefix('p')
                .and_then(|s| s.parse::<ProfileId>().ok())
                .unwrap_or(0);
            out.push((id, r));
        }
        Ok(out)
    }

    /// Simple download speedtest on the running profile (or temp box for one profile).
    pub fn speed_test_simple(
        &mut self,
        profile: Option<&Profile>,
        settings: &AppSettings,
        test_current: bool,
    ) -> Result<SpeedTestResult, CoreError> {
        self.ensure_connected()?;
        let (config, tags) = if test_current {
            (String::new(), Vec::new())
        } else {
            let p = profile.ok_or_else(|| CoreError::Config("no profile for speedtest".into()))?;
            let (c, t) = build_url_test_config(&[p], settings)?;
            (c, t)
        };
        let payload = proto_wire::encode_speed_test_req(
            &config,
            &tags,
            test_current,
            false,
            true, // simple_download
            15000,
        );
        let resp = self.call("SpeedTest", &payload, Duration::from_secs(90))?;
        let results = proto_wire::decode_speed_test_resp(&resp)?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| CoreError::Rpc("empty SpeedTest response".into()))
    }

    #[cfg(unix)]
    fn call(&mut self, method: &str, payload: &[u8], timeout: Duration) -> Result<Vec<u8>, CoreError> {
        let stream = self
            .stream
            .as_mut()
            .ok_or(CoreError::NotRunning)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;

        let req_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let method_bytes = method.as_bytes();
        if method_bytes.len() > u16::MAX as usize {
            return Err(CoreError::Rpc("method name too long".into()));
        }

        let mut frame = Vec::with_capacity(4 + 2 + method_bytes.len() + 4 + payload.len());
        frame.extend_from_slice(&req_id.to_le_bytes());
        frame.extend_from_slice(&(method_bytes.len() as u16).to_le_bytes());
        frame.extend_from_slice(method_bytes);
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(payload);
        stream.write_all(&frame).map_err(|e| {
            self.connected = false;
            CoreError::Rpc(format!("write: {e}"))
        })?;
        stream.flush().ok();

        // Read response header 9 bytes
        let mut header = [0u8; 9];
        stream.read_exact(&mut header).map_err(|e| {
            self.connected = false;
            // Connection drop mid-call usually means the Go core panicked
            // (historically: nil optional bool on LoadConfigReq).
            let tail = self
                .child
                .as_mut()
                .map(read_child_stderr)
                .filter(|s| !s.is_empty())
                .map(|s| format!(" · core: {s}"))
                .unwrap_or_default();
            CoreError::Rpc(format!("read header: {e}{tail}"))
        })?;
        let resp_id = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let status = header[4];
        let data_len = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;
        if resp_id != req_id {
            // In theory concurrent replies could interleave; we only send one at a time.
            warn!(resp_id, req_id, "unexpected response id");
        }
        let mut data = vec![0u8; data_len];
        if data_len > 0 {
            stream.read_exact(&mut data).map_err(|e| {
                self.connected = false;
                CoreError::Rpc(format!("read body: {e}"))
            })?;
        }
        if status != 0 {
            let msg = String::from_utf8_lossy(&data).into_owned();
            return Err(CoreError::Rpc(if msg.is_empty() {
                format!("RPC status={status}")
            } else {
                msg
            }));
        }
        Ok(data)
    }

    #[cfg(not(unix))]
    fn call(&mut self, _method: &str, _payload: &[u8], _timeout: Duration) -> Result<Vec<u8>, CoreError> {
        Err(CoreError::NotImplemented("Windows RPC"))
    }

    pub fn shutdown(&mut self) -> Result<(), CoreError> {
        self.shutdown_inner();
        Ok(())
    }

    fn shutdown_inner(&mut self) {
        #[cfg(unix)]
        {
            if let Some(s) = self.stream.take() {
                // Non-blocking best-effort; don't hang on half-closed peer.
                let _ = s.set_nonblocking(true);
                let _ = s.shutdown(Shutdown::Both);
            }
            self.listener = None;
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            // Bounded wait — never block the caller for more than ~500ms.
            let deadline = Instant::now() + Duration::from_millis(500);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    _ => {
                        // Last resort: kill again and abandon wait.
                        let _ = child.kill();
                        break;
                    }
                }
            }
        }
        if let Some(path) = self.socket_path.take() {
            let _ = std::fs::remove_file(path);
        }
        self.connected = false;
        self.running_profile = None;
    }
}

impl Drop for CoreSession {
    fn drop(&mut self) {
        self.shutdown_inner();
    }
}

/// Ensure a runnable `ThroneCore` sits next to the GUI binary.
///
/// Upstream `parentcheck` requires:
/// - parent process basename == `Throne`
/// - `ThroneCore` directory == parent directory
fn prepare_core_beside_gui(config: &CoreConfig) -> Result<PathBuf, CoreError> {
    let gui = std::env::current_exe().map_err(|e| {
        CoreError::Spawn(format!("cannot resolve current_exe: {e}"))
    })?;
    let gui_dir = gui.parent().ok_or_else(|| {
        CoreError::Spawn("current_exe has no parent directory".into())
    })?;

    let dest = gui_dir.join("ThroneCore");
    let source = find_core_source(config)?;

    // Copy when missing or source is newer / different size.
    let need_copy = match (dest.metadata(), source.metadata()) {
        (Ok(d), Ok(s)) => d.len() != s.len(),
        (Err(_), _) => true,
        _ => true,
    };
    if need_copy {
        if source != dest {
            info!(
                from = %source.display(),
                to = %dest.display(),
                "copying ThroneCore beside GUI for parentcheck"
            );
            std::fs::copy(&source, &dest).map_err(|e| {
                CoreError::Spawn(format!(
                    "copy {} → {}: {e}",
                    source.display(),
                    dest.display()
                ))
            })?;
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dest)
            .map_err(|e| CoreError::Spawn(e.to_string()))?
            .permissions();
        perms.set_mode(0o755);
        let _ = std::fs::set_permissions(&dest, perms);
    }

    // Drop macOS quarantine so Gatekeeper does not block exec (EACCES / killed).
    clear_quarantine(&dest);

    if !dest.exists() {
        return Err(CoreError::BinaryMissing(dest.display().to_string()));
    }
    Ok(dest)
}

fn find_core_source(config: &CoreConfig) -> Result<PathBuf, CoreError> {
    if config.binary_path.exists() {
        return Ok(config.binary_path.clone());
    }
    let resolved = resolve_core_binary();
    if resolved.exists() {
        return Ok(resolved);
    }
    if let Some(p) = which_in_path("ThroneCore") {
        return Ok(p);
    }
    if let Some(p) = which_in_path("Core") {
        return Ok(p);
    }
    Err(CoreError::BinaryMissing(config.binary_path.display().to_string()))
}

fn clear_quarantine(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(path)
            .status();
        // Also clear on the file itself ( -d for single file )
        let _ = Command::new("xattr")
            .args(["-d", "com.apple.quarantine"])
            .arg(path)
            .status();
    }
    let _ = path;
}

fn read_child_stderr(child: &mut Child) -> String {
    let mut s = String::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut s);
    }
    if s.is_empty() {
        if let Some(mut out) = child.stdout.take() {
            let _ = out.read_to_string(&mut s);
        }
    }
    s.chars().take(800).collect()
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{t:x}-{}", std::process::id())
}

/// Whether a path looks like an executable core binary.
pub fn looks_like_core(path: &Path) -> bool {
    path.exists() && path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use throne_domain::{ParsedOutbound, Profile, ProfileType};

    #[test]
    fn build_and_encode_start_payload() {
        let mut p = Profile::new(1, 1, "t", ProfileType::Socks);
        p.outbound = ParsedOutbound {
            server: Some("127.0.0.1".into()),
            server_port: Some(1080),
            ..Default::default()
        };
        let built = build_load_config(&p, &AppSettings::default(), None).unwrap();
        let bytes = proto_wire::encode_load_config_req(&built.core_config_json, false, false, "");
        assert!(bytes.len() > 10);
    }

    #[test]
    fn stub_start_without_binary_errors_clearly() {
        let mut session = CoreSession::new(CoreConfig {
            binary_path: PathBuf::from("/nonexistent/ThroneCore-xyz"),
            ..Default::default()
        });
        // Force path only
        let err = session.ensure_connected().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not found")
                || msg.contains("copy")
                || msg.contains("timeout")
                || msg.contains("exited")
                || msg.contains("spawn"),
            "unexpected: {msg}"
        );
    }

    #[test]
    fn format_core_error_extracts_decode_cause() {
        let raw = r#"decode config at {"log":{"level":"info"},"inbounds":[{"sniff":true}]}
: inbounds[0]: legacy inbound fields are deprecated in sing-box 1.11.0"#;
        let short = format_core_error(raw);
        assert!(short.contains("legacy inbound"), "got: {short}");
        assert!(!short.contains("\"sniff\""), "should drop JSON blob: {short}");
    }
}
