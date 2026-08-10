//! Client for the legacy Go `ThroneCore` process.
//!
//! Protocol (matches `core/server/dispatch.go` + Qt `RPC.cpp`):
//! - GUI listens on a unix domain socket
//! - Core connects (env `THRONE_CORE_SOCKET` = full path)
//! - Request:  `[u32 le reqId][u16 le methodLen][method][u32 le payloadLen][payload]`
//! - Response: `[u32 le reqId][u8 status][u32 le dataLen][data]`
//! - Payload = protobuf (`LoadConfigReq` / `EmptyReq` / `ErrorResp`)

mod config_build;
mod privilege;
mod proto_wire;
mod rule_set_list;
mod sys_proxy;

pub use config_build::{
    AutoSelectorBuild, BuiltConfig, apply_ruleset_mirror, build_load_config, build_load_config_ex,
    build_url_test_config,
};
pub use rule_set_list::{RULE_SET_LIST, lookup_rule_set_url};
pub use privilege::{
    ElevatedPermissions, PrivilegeOutcome, core_has_setuid, core_is_root_setuid,
    core_path_beside_gui, find_core_real_path, is_setuid_set, path_on_nosuid_volume,
    reexec_off_nosuid_volume, request_core_privileges,
};
pub use proto_wire::{
    AutoSelectorGroupStatus, AutoSelectorMemberStatus, ConnectionRow, IpTestResult,
    SpeedTestResult, UrlTestResult,
};
pub use sys_proxy::{
    force_clear_system_proxy, proxy_client_host, set_system_proxy, set_tun_system_dns,
    tun_dns_address,
};

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
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
    /// Tun Mode needs root on the core binary; elevation UI was opened or must be.
    #[error("tun privilege required: {0}")]
    TunPrivilegeRequired(String),
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

/// Match upstream: only stop before Start when a profile is still tracked.
/// A completed Stop leaves the core IPC process alive and ready for reuse.
fn should_stop_before_start(running_profile: Option<ProfileId>) -> bool {
    running_profile.is_some()
}

/// Cap for core stdout/stderr lines buffered until the UI drains them.
const CORE_LOG_BUF_CAP: usize = 2_000;

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
    /// Lines from ThroneCore stdout/stderr (inbound/outbound connection logs, etc.).
    /// Reader threads push; UI drains via [`Self::take_core_logs`].
    core_log_buf: Arc<Mutex<VecDeque<String>>>,
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
            core_log_buf: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn running_profile_id(&self) -> Option<ProfileId> {
        self.running_profile
    }

    /// Drain buffered core log lines (sing-box inbound/outbound traffic, errors, …).
    ///
    /// Safe to call frequently from the UI poller. Empty when the core is idle
    /// or no new output arrived since the last drain.
    pub fn take_core_logs(&self) -> Vec<String> {
        match self.core_log_buf.lock() {
            Ok(mut g) => g.drain(..).collect(),
            Err(poisoned) => poisoned.into_inner().drain(..).collect(),
        }
    }

    /// Last `n` core log lines without draining (for error hints).
    fn core_log_tail(&self, n: usize) -> String {
        core_log_tail_from(&self.core_log_buf, n)
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
        let mut child = cmd.spawn().map_err(|e| {
            CoreError::Spawn(format!(
                "{e} — tried {} (ensure GUI binary is named Throne and core sits beside it)",
                bin.display()
            ))
        })?;
        // Stream stdout/stderr immediately so (1) UI gets inbound/outbound traffic
        // logs like upstream Qt Throne and (2) pipe buffers cannot block the core.
        start_core_log_readers(&mut child, &self.core_log_buf);
        self.child = Some(child);

        // Wait for core to connect (up to ~8s, matching core's 10×500ms retries)
        let deadline = Instant::now() + Duration::from_secs(8);
        let stream = loop {
            if Instant::now() > deadline {
                // Brief wait so reader threads can flush final lines.
                std::thread::sleep(Duration::from_millis(50));
                let hint = self.core_log_tail(30);
                self.shutdown_inner();
                return Err(CoreError::Rpc(format!(
                    "timeout waiting for core IPC. {hint}"
                )));
            }
            // Reap early exit
            if let Some(child) = self.child.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    std::thread::sleep(Duration::from_millis(50));
                    let stderr = self.core_log_tail(40);
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
        self.start_profile_ex(profile, settings, route_profile, None, apply_system_proxy)
    }

    /// Start a profile, optionally expanding an Auto Selector into members.
    pub fn start_profile_ex(
        &mut self,
        profile: &Profile,
        settings: &AppSettings,
        route_profile: Option<&throne_domain::RouteProfile>,
        auto_selector: Option<&AutoSelectorBuild>,
        apply_system_proxy: bool,
    ) -> Result<(), CoreError> {
        let built = build_load_config_ex(profile, settings, route_profile, auto_selector)?;

        // Tun needs a privileged core (setuid root). Check/request before Start.
        if settings.tun_mode_enabled {
            self.ensure_tun_privileges()?;
            info!(
                tun_cidr = %built.tun_ipv4_cidr,
                "Start with Tun inbound enabled"
            );
        }

        self.ensure_connected()?;

        // Switching an active profile requires an in-band Stop. An idle IPC
        // session is reused directly, matching the upstream Qt client.
        if should_stop_before_start(self.running_profile) {
            let _ = self.call(
                "Stop",
                &proto_wire::encode_empty_req(),
                Duration::from_secs(2),
            );
            self.running_profile = None;
        }

        let extras = proto_wire::LoadConfigExtras {
            xray_outbound_dns_address: built.xray_outbound_dns_address.clone(),
            xray_outbound_dns_strategy: built.xray_outbound_dns_strategy.clone(),
            xray_lazy_start: built.xray_lazy_start,
            xray_idle_seconds: built.xray_idle_seconds,
            xray_full_configs: built.xray_full_configs.clone(),
            xray_full_idle_seconds: built.xray_full_idle_seconds,
        };
        // need_xray = shared sidecar only; full configs ride on field 16 (1.2.4).
        let payload = proto_wire::encode_load_config_req_ex(
            &built.core_config_json,
            false,
            built.need_xray,
            &built.xray_config,
            &built.tun_ipv4_cidr,
            &extras,
        );
        // 12s is enough for normal Start; longer hangs freeze node switching UI.
        let resp = match self.call("Start", &payload, Duration::from_secs(12)) {
            Ok(r) => r,
            Err(e) => {
                // Don't leave system proxy / Tun DNS pointing at a dead stack.
                force_clear_system_proxy();
                let _ = set_tun_system_dns(false, "");
                return Err(e);
            }
        };
        let err = proto_wire::decode_error_resp(&resp)?;
        if !err.is_empty() {
            // "already started" → one quick Stop, then one Start retry (no full
            // process recycle unless Stop itself fails).
            if err.to_ascii_lowercase().contains("already started") {
                warn!("Start: instance already started — Stop then retry once");
                let stop_ok = self
                    .call(
                        "Stop",
                        &proto_wire::encode_empty_req(),
                        Duration::from_secs(2),
                    )
                    .is_ok();
                if !stop_ok {
                    self.force_kill_core();
                    self.ensure_connected()?;
                }
                self.running_profile = None;
                let resp2 = self.call("Start", &payload, Duration::from_secs(12))?;
                let err2 = proto_wire::decode_error_resp(&resp2)?;
                if !err2.is_empty() {
                    force_clear_system_proxy();
                    let _ = set_tun_system_dns(false, "");
                    return Err(CoreError::Rpc(format_core_error(&err2)));
                }
            } else {
                force_clear_system_proxy();
                let _ = set_tun_system_dns(false, "");
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

        // Client-side Tun DNS (macOS): point Wi-Fi/Ethernet at tunIP+1 so apps
        // hit hijack-dns even when an older ThroneCore skipped SetSystemDNS.
        if settings.tun_mode_enabled && !built.tun_ipv4_cidr.is_empty() {
            if let Err(e) = set_tun_system_dns(true, &built.tun_ipv4_cidr) {
                warn!(%e, "Tun system DNS enable failed — DNS may leak/pollute");
            } else {
                info!(
                    dns = %tun_dns_address(&built.tun_ipv4_cidr).unwrap_or_default(),
                    "Tun system DNS applied on primary NICs"
                );
            }
        }

        info!(
            profile = profile.id,
            port = settings.inbound_socks_port,
            tun = settings.tun_mode_enabled,
            "core Start OK"
        );
        Ok(())
    }

    /// Path of the ThroneCore binary used for IPC (beside GUI after prepare).
    pub fn resolved_core_path(&self) -> PathBuf {
        if let Ok(gui) = std::env::current_exe() {
            if let Some(dir) = gui.parent() {
                let dest = dir.join("ThroneCore");
                if dest.exists() {
                    return dest;
                }
            }
        }
        if self.config.binary_path.exists() {
            return self.config.binary_path.clone();
        }
        resolve_core_binary()
    }

    /// Upstream `Configs::IsAdmin` — live core `IsPrivileged` (euid == 0).
    ///
    /// When the core is not connected, returns `false` (same as a failed RPC).
    pub fn is_admin(&mut self) -> bool {
        if !self.connected {
            return false;
        }
        match self.call(
            "IsPrivileged",
            &proto_wire::encode_empty_req(),
            Duration::from_millis(800),
        ) {
            Ok(resp) => proto_wire::decode_is_privileged_resp(&resp).unwrap_or(false),
            Err(_) => false,
        }
    }

    /// Upstream name used by older call sites.
    pub fn is_core_privileged(&mut self) -> bool {
        self.is_admin()
    }

    /// Upstream `MainWindow::get_elevated_permissions` + Tun enable gate.
    ///
    /// Returns:
    /// - `Ok(true)` — Tun may be turned on (`IsAdmin` or root-setuid core).
    /// - `Err(TunPrivilegeRequired)` — Terminal opened / denied / re-exec; **do not** enable Tun yet.
    pub fn get_elevated_permissions(&mut self) -> Result<bool, CoreError> {
        // Upstream: if (IsAdmin()) return true;
        if self.is_admin() {
            return Ok(true);
        }

        // nosuid volume (e.g. /Volumes/data): move off before any Terminal spam.
        if let Ok(exe) = std::env::current_exe() {
            if privilege::path_on_nosuid_volume(&exe) {
                // Stop core first so we don't leave orphans across exec.
                if self.connected || self.child.is_some() {
                    self.force_kill_core();
                }
                let core_src = prepare_core_beside_gui(&self.config).ok();
                match privilege::reexec_off_nosuid_volume(core_src.as_deref()) {
                    Ok(false) => {}
                    Ok(true) => {
                        // exec replaced us — unreachable
                        return Err(CoreError::TunPrivilegeRequired(
                            "Restarting on a volume that supports Tun…".into(),
                        ));
                    }
                    Err(e) => {
                        return Err(CoreError::TunPrivilegeRequired(e));
                    }
                }
            }
        }

        // Ensure FindCoreRealPath exists (copy beside GUI if needed).
        let path = prepare_core_beside_gui(&self.config)?;

        match privilege::get_elevated_permissions_for_core(&path) {
            ElevatedPermissions::Ready => {
                // Upstream: StopVPNProcess so next Start picks up setuid euid.
                if self.connected || self.child.is_some() {
                    info!("StopVPNProcess equivalent — restart core after setuid grant");
                    self.force_kill_core();
                }
                Ok(true)
            }
            ElevatedPermissions::Reexecing => Err(CoreError::TunPrivilegeRequired(
                "Restarting on a volume that supports Tun…".into(),
            )),
            ElevatedPermissions::RetryAfterPassword { hint } => {
                Err(CoreError::TunPrivilegeRequired(hint))
            }
            ElevatedPermissions::Denied { reason } => {
                Err(CoreError::TunPrivilegeRequired(reason))
            }
        }
    }

    /// Upstream `set_spmode_vpn(true)` permission check before enabling Tun.
    pub fn request_tun_privileges_for_toggle(&mut self) -> Result<bool, CoreError> {
        if self.is_admin() {
            return Ok(true);
        }
        self.get_elevated_permissions()
    }

    /// Before Start with Tun: same gate as enabling Tun.
    pub fn ensure_tun_privileges(&mut self) -> Result<(), CoreError> {
        if self.is_admin() {
            return Ok(());
        }
        let path = prepare_core_beside_gui(&self.config)?;
        if privilege::core_is_root_setuid(&path) && !privilege::path_on_nosuid_volume(&path) {
            if (self.connected || self.child.is_some()) && !self.is_admin() {
                self.force_kill_core();
            }
            return Ok(());
        }
        self.get_elevated_permissions().map(|_| ())
    }

    /// Stop the running profile.
    ///
    /// Order is intentional for UI responsiveness:
    /// 1. Clear system proxy first (browsers unblock even if core is wedged)
    /// 2. Stop RPC with a short timeout
    /// 3. **Keep ThroneCore alive** for the next Start (cold spawn is the main lag).
    ///    Only force-kill when Stop RPC fails (wedged core).
    pub fn stop_profile(&mut self, settings: &AppSettings) -> Result<(), CoreError> {
        // 1) Always drop system proxy + Tun DNS first — browsers unblock even if
        //    core Stop wedges. Keep networksetup on primary NICs only.
        let host = if settings.inbound_address.trim().is_empty() {
            "127.0.0.1"
        } else {
            settings.inbound_address.trim()
        };
        if let Err(e) = set_system_proxy(false, host, settings.inbound_socks_port) {
            warn!(%e, "system proxy clear on stop failed — force clear primary services");
            force_clear_system_proxy();
        }
        // Always try to clear Tun DNS (idempotent Empty). Leaving 172.19.0.2
        // after Stop = total DNS blackhole.
        if let Err(e) = set_tun_system_dns(false, "") {
            warn!(%e, "Tun system DNS clear on stop failed");
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

    /// Idempotent snapshot of every running auto-selector group (core ≥ 1.2.3).
    pub fn query_auto_selectors(&mut self) -> Result<Vec<AutoSelectorGroupStatus>, CoreError> {
        if !self.connected || self.running_profile.is_none() {
            return Err(CoreError::NotRunning);
        }
        let resp = self.call(
            "QueryAutoSelectors",
            &proto_wire::encode_empty_req(),
            Duration::from_secs(5),
        )?;
        proto_wire::decode_query_auto_selectors_resp(&resp)
    }

    /// `recheck` = force a sweep; `select` + member tag pins (empty member = unpin).
    pub fn auto_selector_action(
        &mut self,
        tag: &str,
        action: &str,
        member: &str,
    ) -> Result<(), CoreError> {
        if !self.connected || self.running_profile.is_none() {
            return Err(CoreError::NotRunning);
        }
        let payload = proto_wire::encode_auto_selector_action(tag, action, member);
        let resp = self.call("AutoSelectorAction", &payload, Duration::from_secs(8))?;
        let err = proto_wire::decode_error_resp(&resp)?;
        if err.is_empty() {
            Ok(())
        } else {
            Err(CoreError::Rpc(format_core_error(&err)))
        }
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
        // Clone Arc so the error closure does not re-borrow `self` while `stream` is live.
        let log_buf = Arc::clone(&self.core_log_buf);
        stream.read_exact(&mut header).map_err(|e| {
            self.connected = false;
            // Connection drop mid-call usually means the Go core panicked
            // (historically: nil optional bool on LoadConfigReq).
            let t = core_log_tail_from(&log_buf, 20);
            let tail = if t.is_empty() {
                String::new()
            } else {
                format!(" · core: {t}")
            };
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

    // Never overwrite a setuid core (upstream leaves applicationDirPath/ThroneCore
    // alone after `chmod u+s`). Copying would strip setuid under `cargo run`.
    let dest_has_setuid = privilege::is_setuid_set(&dest);

    let need_copy = if dest_has_setuid {
        false
    } else {
        match (dest.metadata(), source.metadata()) {
            (Ok(d), Ok(s)) => d.len() != s.len(),
            (Err(_), _) => true,
            _ => true,
        }
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
        // Do not chmod a setuid binary (would drop u+s or fail on root-owned).
        if !dest_has_setuid {
            if let Ok(meta) = std::fs::metadata(&dest) {
                let mut perms = meta.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&dest, perms);
            }
        }
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

/// Spawn background threads that funnel core stdout/stderr into `buf` line-by-line.
///
/// Must run immediately after `Command::spawn` while pipes are still attached.
/// Dropping the child (or process exit) closes the pipes and ends the threads.
fn start_core_log_readers(child: &mut Child, buf: &Arc<Mutex<VecDeque<String>>>) {
    if let Some(stdout) = child.stdout.take() {
        spawn_pipe_reader("throne-core-stdout", stdout, Arc::clone(buf));
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_pipe_reader("throne-core-stderr", stderr, Arc::clone(buf));
    }
}

fn spawn_pipe_reader(
    name: &str,
    pipe: impl Read + Send + 'static,
    buf: Arc<Mutex<VecDeque<String>>>,
) {
    let _ = std::thread::Builder::new().name(name.into()).spawn(move || {
        let reader = BufReader::new(pipe);
        for line in reader.lines() {
            match line {
                Ok(raw) => {
                    if let Some(msg) = normalize_core_log_line(&raw) {
                        push_core_log_line(&buf, msg);
                    }
                }
                Err(_) => break,
            }
        }
    });
}

/// Trim noise from a core log line. Returns `None` for empty / pure whitespace.
fn normalize_core_log_line(raw: &str) -> Option<String> {
    let line = raw.trim_end_matches(['\r', '\n']).trim();
    if line.is_empty() {
        return None;
    }
    // Go standard logger prefixes `yyyy/mm/dd HH:MM:SS ` — keep the message body
    // when present so the UI timestamp is the single clock.
    let body = strip_go_std_log_prefix(line);
    let body = body.trim();
    if body.is_empty() {
        None
    } else {
        Some(body.to_string())
    }
}

fn strip_go_std_log_prefix(line: &str) -> &str {
    // "2006/01/02 15:04:05 message"
    let bytes = line.as_bytes();
    if bytes.len() > 20
        && bytes[4] == b'/'
        && bytes[7] == b'/'
        && bytes[10] == b' '
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b' '
    {
        return &line[20..];
    }
    line
}

fn push_core_log_line(buf: &Arc<Mutex<VecDeque<String>>>, msg: String) {
    let Ok(mut g) = buf.lock() else {
        return;
    };
    g.push_back(msg);
    while g.len() > CORE_LOG_BUF_CAP {
        g.pop_front();
    }
}

fn core_log_tail_from(buf: &Arc<Mutex<VecDeque<String>>>, n: usize) -> String {
    let g = match buf.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if g.is_empty() || n == 0 {
        return String::new();
    }
    let skip = g.len().saturating_sub(n);
    g.iter().skip(skip).cloned().collect::<Vec<_>>().join("\n")
}

/// Read leftover stdout/stderr only after the child has exited (tests / fallback).
#[cfg(test)]
fn read_child_stderr(child: &mut Child) -> String {
    // Reading a piped stream to EOF blocks while the core is still alive. RPC
    // timeouts must return promptly, matching the upstream client behavior.
    if !matches!(child.try_wait(), Ok(Some(_))) {
        return String::new();
    }

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

    #[cfg(unix)]
    #[test]
    fn diagnostic_output_does_not_wait_for_a_live_core_process() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 2"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn live child");
        let started = Instant::now();

        let output = read_child_stderr(&mut child);

        assert!(
            started.elapsed() < Duration::from_millis(500),
            "reading diagnostics waited for the live child to exit"
        );
        assert!(output.is_empty());
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn build_and_encode_start_payload() {
        let mut p = Profile::new(1, 1, "t", ProfileType::Socks);
        p.outbound = ParsedOutbound {
            server: Some("127.0.0.1".into()),
            server_port: Some(1080),
            ..Default::default()
        };
        let built = build_load_config(&p, &AppSettings::default(), None).unwrap();
        let bytes = proto_wire::encode_load_config_req(
            &built.core_config_json,
            false,
            false,
            "",
            &built.tun_ipv4_cidr,
        );
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

    #[test]
    fn normalize_core_log_keeps_inbound_outbound_lines() {
        let line = normalize_core_log_line(
            "INFO[0001] [1234567890] inbound/mixed[mixed-in]: inbound connection from 127.0.0.1:54321",
        )
        .expect("line");
        assert!(line.contains("inbound/mixed"));
        assert!(line.contains("inbound connection"));

        let out = normalize_core_log_line(
            "INFO[0001] [1234567890] outbound/direct[direct]: outbound connection to apple.com:443",
        )
        .expect("line");
        assert!(out.contains("outbound/direct"));
        assert!(out.contains("apple.com"));
    }

    #[test]
    fn normalize_core_log_strips_go_std_prefix() {
        let line = normalize_core_log_line("2026/08/04 15:30:01 Start: {\"log\":{}}").expect("line");
        assert_eq!(line, "Start: {\"log\":{}}");
        assert!(normalize_core_log_line("   \n").is_none());
    }

    #[test]
    fn take_core_logs_drains_buffer() {
        let session = CoreSession::new(CoreConfig::default());
        push_core_log_line(
            &session.core_log_buf,
            "inbound/mixed[mixed-in]: inbound connection from 127.0.0.1:1".into(),
        );
        push_core_log_line(
            &session.core_log_buf,
            "outbound/proxy[proxy]: outbound connection to example.com:443".into(),
        );
        let first = session.take_core_logs();
        assert_eq!(first.len(), 2);
        assert!(first[0].contains("inbound"));
        assert!(first[1].contains("outbound"));
        assert!(session.take_core_logs().is_empty());
    }

    #[test]
    fn stop_before_start_only_when_profile_is_tracked() {
        // Cold or idle IPC session: no Stop needed.
        assert!(!should_stop_before_start(None));
        // An actively tracked profile must be stopped before switching.
        assert!(should_stop_before_start(Some(42)));
    }
}
