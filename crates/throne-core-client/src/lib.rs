//! Client for the legacy Go `ThroneCore` process.
//!
//! The original Qt client talks to the core over a local socket with protobuf
//! (`core/server/gen/libcore.proto`). This crate defines the async surface and
//! a **stub** implementation so the GPUI shell can develop without a built core.
//!
//! Next steps:
//! 1. Generate Rust types from `libcore.proto` (prost / tonic or a custom
//!    length-prefixed protobuf codec matching the Qt local-socket framing).
//! 2. Spawn `CoreProcess` with the platform binary produced by
//!    `script/build_go.sh` (to be ported to `cargo xtask`).
//! 3. Wire `start` / `stop` / `query_stats` into the GPUI app state.

use std::path::PathBuf;
use std::process::Stdio;

use thiserror::Error;
use tokio::process::{Child, Command};
use tracing::{info, warn};

use throne_domain::{ProfileId, TrafficSnapshot};

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("core binary not found at {0}")]
    BinaryMissing(PathBuf),
    #[error("failed to spawn core: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("core RPC error: {0}")]
    Rpc(String),
    #[error("core is not running")]
    NotRunning,
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
}

/// Configuration for locating / launching the Go core.
#[derive(Debug, Clone)]
pub struct CoreConfig {
    /// Path to the `Core` / `nekobox_core` binary.
    pub binary_path: PathBuf,
    /// Working directory for the core process.
    pub work_dir: PathBuf,
    /// Extra CLI args (platform-specific privilege helpers, etc.).
    pub extra_args: Vec<String>,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            binary_path: default_core_binary(),
            work_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            extra_args: Vec::new(),
        }
    }
}

fn default_core_binary() -> PathBuf {
    // Prefer a sibling binary next to the GUI, then PATH lookup name.
    let local = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("Core")));
    local.unwrap_or_else(|| PathBuf::from("Core"))
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

/// Handle to a running (or stub) core session.
#[derive(Debug)]
pub struct CoreSession {
    config: CoreConfig,
    child: Option<Child>,
    connected: bool,
}

impl CoreSession {
    pub fn new(config: CoreConfig) -> Self {
        Self {
            config,
            child: None,
            connected: false,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Spawn the Go core process if the binary exists. Does not establish RPC yet.
    pub async fn spawn_process(&mut self) -> Result<(), CoreError> {
        if !self.config.binary_path.exists() {
            return Err(CoreError::BinaryMissing(self.config.binary_path.clone()));
        }
        info!(path = %self.config.binary_path.display(), "spawning throne core");
        let mut cmd = Command::new(&self.config.binary_path);
        cmd.current_dir(&self.config.work_dir)
            .args(&self.config.extra_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = cmd.spawn()?;
        self.child = Some(child);
        // RPC handshake is not implemented yet.
        self.connected = false;
        Err(CoreError::NotImplemented(
            "local-socket protobuf RPC handshake",
        ))
    }

    /// Start a profile configuration on the core.
    pub async fn start(&mut self, req: LoadConfigRequest) -> Result<(), CoreError> {
        let _ = req;
        if !self.connected {
            // Development path: allow UI to simulate without a core binary.
            warn!("core RPC not connected; start() is a no-op stub");
            return Err(CoreError::NotImplemented("Start RPC"));
        }
        Err(CoreError::NotImplemented("Start RPC"))
    }

    pub async fn stop(&mut self) -> Result<(), CoreError> {
        if !self.connected {
            warn!("core RPC not connected; stop() is a no-op stub");
            return Err(CoreError::NotImplemented("Stop RPC"));
        }
        Err(CoreError::NotImplemented("Stop RPC"))
    }

    pub async fn query_stats(&self) -> Result<TrafficSnapshot, CoreError> {
        if !self.connected {
            return Err(CoreError::NotRunning);
        }
        Err(CoreError::NotImplemented("QueryStats RPC"))
    }

    /// Kill the child process if we own one.
    pub async fn shutdown(&mut self) -> Result<(), CoreError> {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
        }
        self.connected = false;
        Ok(())
    }
}

/// Whether a core binary appears available on disk.
pub fn core_binary_available(config: &CoreConfig) -> bool {
    config.binary_path.exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_start_without_binary() {
        let mut session = CoreSession::new(CoreConfig {
            binary_path: PathBuf::from("/nonexistent/Core"),
            ..Default::default()
        });
        let err = session
            .start(LoadConfigRequest {
                profile_id: 1,
                core_config_json: "{}".into(),
                need_xray: false,
                xray_config: String::new(),
                disable_stats: false,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::NotImplemented(_)));
    }
}
