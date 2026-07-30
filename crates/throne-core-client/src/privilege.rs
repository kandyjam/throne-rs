//! TUN privilege helpers — align with upstream `get_elevated_permissions`.
//!
//! On macOS/Linux, creating a TUN device requires the core process to run as
//! root (typically via setuid on `ThroneCore`). Upstream:
//! - macOS: elevate core with admin privileges (`chown root` + `chmod u+s`)
//! - Linux: `pkexec chown/chmod u+s`
//! - Windows: relaunch GUI as admin (not implemented here yet)
//!
//! Note: upstream `isSetuidSet` only checks the setuid **bit**, not file owner.
//! Requiring `uid == 0` caused an infinite Terminal loop under `cargo run` when
//! `chmod u+s` stuck but `chown root` did not.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tracing::{info, warn};

/// Outcome of a privilege grant request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivilegeOutcome {
    /// Core already has privileges (setuid bit, or live euid 0).
    AlreadyPrivileged,
    /// Elevation finished successfully (setuid root now).
    Granted,
    /// Elevation UI was shown; user must finish / retry (legacy path).
    ElevationLaunched { hint: String },
    /// Could not start elevation (missing tool, path, etc.).
    Failed(String),
    /// Platform does not implement automatic elevation yet.
    Unsupported(&'static str),
}

/// Paths for which we already opened an elevation prompt recently (anti-spam).
static RECENT_ELEVATION: Mutex<Option<(PathBuf, Instant)>> = Mutex::new(None);

const ELEVATION_COOLDOWN: Duration = Duration::from_secs(90);

/// True when the core binary has the setuid bit (matches upstream `isSetuidSet`).
///
/// Does **not** require `uid == 0` — upstream only checks `S_ISUID`. A user-owned
/// setuid bit is not enough for TUN, but `core_is_root_setuid` covers the real case.
pub fn core_has_setuid(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match std::fs::metadata(path) {
            Ok(meta) => (meta.mode() & 0o4000) != 0,
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

/// True when the binary is root-owned **and** setuid — actually useful for TUN.
pub fn core_is_root_setuid(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match std::fs::metadata(path) {
            Ok(meta) => (meta.mode() & 0o4000) != 0 && meta.uid() == 0,
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

fn elevation_on_cooldown(path: &Path) -> bool {
    let Ok(guard) = RECENT_ELEVATION.lock() else {
        return false;
    };
    match guard.as_ref() {
        Some((p, at)) if p == path && at.elapsed() < ELEVATION_COOLDOWN => true,
        _ => false,
    }
}

fn mark_elevation_attempted(path: &Path) {
    if let Ok(mut guard) = RECENT_ELEVATION.lock() {
        *guard = Some((path.to_path_buf(), Instant::now()));
    }
}

/// Clear cooldown after a confirmed grant (so a later core replace can re-elevate).
pub fn clear_elevation_cooldown() {
    if let Ok(mut guard) = RECENT_ELEVATION.lock() {
        *guard = None;
    }
}

/// Request elevated privileges for `core_path` (setuid root).
///
/// Prefer a single native admin dialog over spamming Terminal windows.
pub fn request_core_privileges(core_path: &Path) -> PrivilegeOutcome {
    if !core_path.exists() {
        return PrivilegeOutcome::Failed(format!(
            "ThroneCore not found at {}",
            core_path.display()
        ));
    }

    // Already fully elevated for TUN.
    if core_is_root_setuid(core_path) {
        clear_elevation_cooldown();
        return PrivilegeOutcome::AlreadyPrivileged;
    }

    // Cooldown: avoid opening admin UI on every Tun toggle while the user is
    // still typing their password (or just finished).
    if elevation_on_cooldown(core_path) {
        // Re-check in case they finished between clicks.
        if core_is_root_setuid(core_path) {
            clear_elevation_cooldown();
            return PrivilegeOutcome::Granted;
        }
        return PrivilegeOutcome::ElevationLaunched {
            hint: "Admin prompt already shown — finish it, then enable Tun Mode again."
                .into(),
        };
    }

    #[cfg(target_os = "macos")]
    {
        return request_macos(core_path);
    }
    #[cfg(target_os = "linux")]
    {
        return request_linux(core_path);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = core_path;
        PrivilegeOutcome::Unsupported(
            "Tun Mode requires administrator privileges; automatic elevation is not implemented on this OS yet",
        )
    }
}

#[cfg(target_os = "macos")]
fn request_macos(core_path: &Path) -> PrivilegeOutcome {
    // Use `do shell script … with administrator privileges` — one native password
    // dialog, no Terminal window. Upstream used Terminal + sudo; that spam-opens
    // a new tab on every Tun toggle under cargo run.
    let path = core_path.display().to_string();
    // Escape for embedding inside AppleScript double-quoted string.
    let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
    let shell = format!("chown root:wheel \"{escaped}\" && chmod u+s \"{escaped}\"");
    // Escape for embedding inside the outer AppleScript string argument.
    let shell_as = shell.replace('\\', "\\\\").replace('"', "\\\"");
    let script =
        format!("do shell script \"{shell_as}\" with administrator privileges");

    info!(path = %core_path.display(), "requesting macOS setuid on ThroneCore via osascript");
    mark_elevation_attempted(core_path);

    let output = Command::new("osascript").args(["-e", &script]).output();

    match output {
        Ok(out) if out.status.success() => {
            if core_is_root_setuid(core_path) {
                clear_elevation_cooldown();
                info!(path = %core_path.display(), "ThroneCore is now root setuid");
                PrivilegeOutcome::Granted
            } else if core_has_setuid(core_path) {
                // Bit set but not root-owned — still not enough for TUN.
                warn!(
                    path = %core_path.display(),
                    "setuid bit set but owner is not root"
                );
                PrivilegeOutcome::Failed(
                    "ThroneCore setuid is set but not owned by root — try again or: sudo chown root:wheel target/debug/ThroneCore && sudo chmod u+s target/debug/ThroneCore"
                        .into(),
                )
            } else {
                PrivilegeOutcome::Failed(
                    "admin dialog succeeded but ThroneCore is still not setuid root".into(),
                )
            }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let msg = stderr.trim();
            let msg = if msg.is_empty() {
                stdout.trim()
            } else {
                msg
            };
            // User cancelled the dialog — don't treat as hard crash.
            if msg.contains("User canceled") || msg.contains("-128") {
                PrivilegeOutcome::Failed("administrator authentication cancelled".into())
            } else {
                PrivilegeOutcome::Failed(format!(
                    "osascript failed ({})",
                    if msg.is_empty() {
                        out.status.to_string()
                    } else {
                        msg.to_string()
                    }
                ))
            }
        }
        Err(e) => PrivilegeOutcome::Failed(format!("failed to run osascript: {e}")),
    }
}

#[cfg(target_os = "linux")]
fn request_linux(core_path: &Path) -> PrivilegeOutcome {
    if !have_pkexec() {
        return PrivilegeOutcome::Failed(
            "pkexec not found — install polkit (pkexec) to grant Tun privileges".into(),
        );
    }
    let path = core_path.display().to_string();
    info!(path = %path, "requesting Linux setuid on ThroneCore via pkexec");
    mark_elevation_attempted(core_path);

    let chown = Command::new("pkexec")
        .args(["chown", "root:root", &path])
        .status();
    match chown {
        Ok(s) if s.success() => {}
        Ok(s) => {
            return PrivilegeOutcome::Failed(format!("pkexec chown failed ({s})"));
        }
        Err(e) => return PrivilegeOutcome::Failed(format!("pkexec chown: {e}")),
    }

    let chmod = Command::new("pkexec")
        .args(["chmod", "u+s", &path])
        .status();
    match chmod {
        Ok(s) if s.success() => {
            if core_is_root_setuid(core_path) {
                clear_elevation_cooldown();
                PrivilegeOutcome::Granted
            } else {
                PrivilegeOutcome::Failed(
                    "pkexec finished but ThroneCore is still not root setuid".into(),
                )
            }
        }
        Ok(s) => PrivilegeOutcome::Failed(format!("pkexec chmod u+s failed ({s})")),
        Err(e) => PrivilegeOutcome::Failed(format!("pkexec chmod: {e}")),
    }
}

#[cfg(target_os = "linux")]
fn have_pkexec() -> bool {
    Command::new("pkexec")
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Resolve the on-disk core path that should receive setuid (beside GUI).
pub fn core_path_beside_gui() -> Option<PathBuf> {
    let gui = std::env::current_exe().ok()?;
    let dir = gui.parent()?;
    let dest = dir.join("ThroneCore");
    if dest.exists() {
        Some(dest)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_path_fails() {
        let r = request_core_privileges(Path::new("/nonexistent/ThroneCore-xyz"));
        assert!(matches!(r, PrivilegeOutcome::Failed(_)), "{r:?}");
    }

    #[test]
    fn setuid_helpers_on_missing() {
        assert!(!core_has_setuid(Path::new("/no/such/ThroneCore")));
        assert!(!core_is_root_setuid(Path::new("/no/such/ThroneCore")));
    }
}
