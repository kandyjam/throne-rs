//! TUN privilege helpers — mirror upstream Qt Throne, with cargo-run fixes.
//!
//! Upstream intent (`get_elevated_permissions`):
//! - `IsAdmin` = live core `IsPrivileged` (euid 0)
//! - grant via `chown root` + `chmod u+s` on `ThroneCore`
//!
//! **cargo-run fixes vs raw upstream `Mac_Run_Command`:**
//! 1. **nosuid volumes** (`/Volumes/data`): setuid is ignored — stage GUI+core to
//!    `~/Library/Application Support/Throne/runtime/` and re-exec.
//! 2. **paths with spaces** (`Application Support`): upstream embeds
//!    `sudo chown '…'` inside shell-single-quoted `osascript -e '…'`, which
//!    **breaks the quoting** and yields `osascript` exit 1. We use
//!    `do shell script … with administrator privileges` + AppleScript
//!    `quoted form of` instead (one system password dialog, no Terminal).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tracing::info;

/// Outcome of upstream `get_elevated_permissions` / Tun enable gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElevatedPermissions {
    /// Tun may be enabled now (`IsAdmin` or root-setuid core).
    Ready,
    /// Process is about to `exec` onto a suid-capable volume — caller should stop.
    Reexecing,
    /// Terminal opened; enter password, then enable Tun again (do **not** enable yet).
    RetryAfterPassword { hint: String },
    /// Hard failure — do not enable Tun.
    Denied { reason: String },
}

/// Paths we already showed an admin dialog for recently (anti-spam).
static RECENT_ELEVATION: Mutex<Option<(PathBuf, Instant)>> = Mutex::new(None);
const ELEVATION_COOLDOWN: Duration = Duration::from_secs(90);

/// Upstream `Configs::isSetuidSet` — only checks `S_ISUID`.
pub fn is_setuid_set(path: &Path) -> bool {
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

pub fn core_has_setuid(path: &Path) -> bool {
    is_setuid_set(path)
}

/// Root-owned + setuid — actually useful for TUN (and not ignored by nosuid).
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

/// Upstream `Configs::FindCoreRealPath`.
pub fn find_core_real_path() -> Option<PathBuf> {
    let gui = std::env::current_exe().ok()?;
    let dir = gui.parent()?;
    Some(dir.join("ThroneCore"))
}

pub fn core_path_beside_gui() -> Option<PathBuf> {
    let p = find_core_real_path()?;
    p.exists().then_some(p)
}

/// True if `path`'s mount has the `nosuid` flag (setuid bit ignored by kernel).
pub fn path_on_nosuid_volume(path: &Path) -> bool {
    let abs = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    let abs_s = abs.to_string_lossy();

    let Ok(out) = Command::new("mount").output() else {
        return false;
    };
    let text = String::from_utf8_lossy(&out.stdout);

    // `/dev/disk5s1 on /Volumes/data (apfs, local, nodev, nosuid, journaled, noowners)`
    let mut best: Option<(usize, bool)> = None;
    for line in text.lines() {
        let Some((_, rest)) = line.split_once(" on ") else {
            continue;
        };
        let Some((mnt, opts)) = rest.split_once(" (") else {
            continue;
        };
        let mnt = mnt.trim();
        if mnt.is_empty() {
            continue;
        }
        let matches = abs_s.as_ref() == mnt
            || abs_s.starts_with(&(mnt.to_string() + "/"))
            || (mnt == "/" && abs_s.starts_with('/'));
        if !matches {
            continue;
        }
        let len = mnt.len();
        let nosuid = opts.split(|c| c == ',' || c == ' ' || c == ')').any(|t| t == "nosuid");
        if best.map(|(l, _)| len >= l).unwrap_or(true) {
            best = Some((len, nosuid));
        }
    }
    best.map(|(_, n)| n).unwrap_or(false)
}

/// Runtime dir on the system Data volume (suid works).
pub fn suid_runtime_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/Throne/runtime"))
}

fn copy_file(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(src, dst).map_err(|e| format!("copy {} → {}: {e}", src.display(), dst.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if !is_setuid_set(dst) {
            let mut p = std::fs::metadata(dst)
                .map_err(|e| e.to_string())?
                .permissions();
            p.set_mode(0o755);
            let _ = std::fs::set_permissions(dst, p);
        }
    }
    Ok(())
}

fn needs_refresh(src: &Path, dst: &Path) -> bool {
    if !dst.exists() {
        return true;
    }
    // Never replace an elevated core.
    if core_is_root_setuid(dst) {
        return false;
    }
    match (src.metadata(), dst.metadata()) {
        (Ok(s), Ok(d)) => s.len() != d.len(),
        _ => true,
    }
}

/// If the running GUI is on a **nosuid** volume, stage `Throne`+`ThroneCore` onto
/// the system volume and `exec` that GUI (upstream parentcheck still holds).
///
/// Returns `Ok(true)` when this process has been replaced (does not return on
/// success — `exec` replaces the image). `Ok(false)` if already on a suid volume.
pub fn reexec_off_nosuid_volume(core_source: Option<&Path>) -> Result<bool, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if !path_on_nosuid_volume(&exe) {
        return Ok(false);
    }
    if std::env::var_os("THRONE_OFF_NOSUID").is_some() {
        return Err(format!(
            "GUI is still on a nosuid volume ({}) after re-exec. Move the project off /Volumes/data or install to /Applications.",
            exe.display()
        ));
    }

    let runtime = suid_runtime_dir().ok_or_else(|| "HOME not set".to_string())?;
    std::fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;

    let dest_gui = runtime.join("Throne");
    let dest_core = runtime.join("ThroneCore");

    if needs_refresh(&exe, &dest_gui) {
        info!(from = %exe.display(), to = %dest_gui.display(), "staging Throne off nosuid volume");
        copy_file(&exe, &dest_gui)?;
    }

    let src_core = core_source
        .map(|p| p.to_path_buf())
        .or_else(|| exe.parent().map(|d| d.join("ThroneCore")))
        .filter(|p| p.exists());
    if let Some(src) = src_core {
        if needs_refresh(&src, &dest_core) {
            info!(from = %src.display(), to = %dest_core.display(), "staging ThroneCore off nosuid volume");
            copy_file(&src, &dest_core)?;
        }
    } else if !dest_core.exists() {
        return Err(
            "ThroneCore not found beside GUI — cannot stage Tun runtime off nosuid volume".into(),
        );
    }

    // Drop quarantine on staged binaries.
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(&runtime)
            .status();
    }

    info!(
        to = %dest_gui.display(),
        "re-exec Throne from suid-capable volume for Tun Mode"
    );

    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| runtime.clone());
    let mut cmd = Command::new(&dest_gui);
    cmd.args(&args)
        .current_dir(&cwd)
        .env("THRONE_OFF_NOSUID", "1")
        .env("THRONE_ORIG_EXE", &exe)
        .env("THRONE_ORIG_CWD", &cwd);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        return Err(format!("exec {} failed: {err}", dest_gui.display()));
    }
    #[cfg(not(unix))]
    {
        let _ = cmd.spawn().map_err(|e| e.to_string())?;
        std::process::exit(0);
    }
}

fn elevation_on_cooldown(path: &Path) -> bool {
    let Ok(g) = RECENT_ELEVATION.lock() else {
        return false;
    };
    match g.as_ref() {
        Some((p, t)) if p == path && t.elapsed() < ELEVATION_COOLDOWN => true,
        _ => false,
    }
}

fn mark_elevation(path: &Path) {
    if let Ok(mut g) = RECENT_ELEVATION.lock() {
        *g = Some((path.to_path_buf(), Instant::now()));
    }
}

fn clear_elevation_cooldown() {
    if let Ok(mut g) = RECENT_ELEVATION.lock() {
        *g = None;
    }
}

/// Escape a path for use inside an AppleScript `"…"` string.
fn applescript_string_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Grant setuid-root on `core_path` via a single macOS admin password dialog.
///
/// Uses `do shell script … with administrator privileges` + `quoted form of`
/// so paths with spaces (`Application Support`) work. Upstream's Terminal
/// `do script` + nested shell quotes breaks on those paths (osascript exit 1).
#[cfg(target_os = "macos")]
pub fn elevate_core_setuid_macos(core_path: &Path) -> Result<(), String> {
    let path = core_path.to_string_lossy();
    let path_as = applescript_string_escape(&path);
    // quoted form of produces shell-safe quoting for the path.
    let script = format!(
        r#"do shell script "chown root:wheel " & quoted form of "{path_as}" & " && chmod u+s " & quoted form of "{path_as}" with administrator privileges"#
    );
    info!(path = %core_path.display(), "requesting admin for ThroneCore setuid");
    mark_elevation(core_path);

    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("failed to run osascript: {e}"))?;

    if output.status.success() {
        if core_is_root_setuid(core_path) {
            clear_elevation_cooldown();
            info!(path = %core_path.display(), "ThroneCore is root setuid");
            return Ok(());
        }
        return Err(format!(
            "admin OK but ThroneCore is still not root setuid ({})",
            core_path.display()
        ));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let msg = stderr.trim();
    let msg = if msg.is_empty() {
        stdout.trim()
    } else {
        msg
    };
    if msg.contains("User canceled") || msg.contains("-128") {
        Err("administrator authentication cancelled".into())
    } else if msg.is_empty() {
        Err(format!(
            "osascript failed ({})",
            output.status.code().unwrap_or(-1)
        ))
    } else {
        Err(msg.to_string())
    }
}

/// Upstream macOS `get_elevated_permissions` body + nosuid / path-space fixes.
pub fn get_elevated_permissions_for_core(core_path: &Path) -> ElevatedPermissions {
    if !core_path.exists() {
        return ElevatedPermissions::Denied {
            reason: format!("ThroneCore not found at {}", core_path.display()),
        };
    }

    // Already fully elevated on a suid-capable volume.
    if core_is_root_setuid(core_path) && !path_on_nosuid_volume(core_path) {
        info!(path = %core_path.display(), "root setuid core ready");
        clear_elevation_cooldown();
        return ElevatedPermissions::Ready;
    }

    // Core (or GUI) still on nosuid → re-exec to system volume first.
    if path_on_nosuid_volume(core_path) {
        match reexec_off_nosuid_volume(Some(core_path)) {
            Ok(false) => {}
            Ok(true) => return ElevatedPermissions::Reexecing,
            Err(e) => {
                return ElevatedPermissions::Denied {
                    reason: format!(
                        "Tun Mode cannot use setuid on this volume (nosuid). {e}"
                    ),
                };
            }
        }
    }

    if is_setuid_set(core_path) && core_is_root_setuid(core_path) {
        clear_elevation_cooldown();
        return ElevatedPermissions::Ready;
    }

    #[cfg(target_os = "macos")]
    {
        if elevation_on_cooldown(core_path) {
            if core_is_root_setuid(core_path) {
                clear_elevation_cooldown();
                return ElevatedPermissions::Ready;
            }
            return ElevatedPermissions::RetryAfterPassword {
                hint: "Complete the macOS password dialog if it is still open, then enable Tun Mode again.".into(),
            };
        }

        match elevate_core_setuid_macos(core_path) {
            Ok(()) => ElevatedPermissions::Ready,
            Err(e) => ElevatedPermissions::Denied { reason: e },
        }
    }

    #[cfg(target_os = "linux")]
    {
        get_elevated_permissions_linux(core_path)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        ElevatedPermissions::Denied {
            reason: "Please run Throne as administrator for Tun Mode.".into(),
        }
    }
}

#[cfg(target_os = "linux")]
fn get_elevated_permissions_linux(core_path: &Path) -> ElevatedPermissions {
    if core_is_root_setuid(core_path) {
        return ElevatedPermissions::Ready;
    }
    if !Command::new("pkexec")
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return ElevatedPermissions::Denied {
            reason: "Please install \"pkexec\" first.".into(),
        };
    }
    let path = core_path.display().to_string();
    let chown = Command::new("pkexec")
        .args(["chown", "root:root", &path])
        .status();
    match chown {
        Ok(s) if s.success() => {}
        Ok(s) => {
            return ElevatedPermissions::Denied {
                reason: format!("pkexec chown failed ({s})"),
            };
        }
        Err(e) => {
            return ElevatedPermissions::Denied {
                reason: format!("pkexec chown: {e}"),
            };
        }
    }
    match Command::new("pkexec")
        .args(["chmod", "u+s", &path])
        .status()
    {
        Ok(s) if s.success() && core_is_root_setuid(core_path) => ElevatedPermissions::Ready,
        Ok(s) if s.success() => ElevatedPermissions::RetryAfterPassword {
            hint: "Core privileges updated — enable Tun Mode again.".into(),
        },
        Ok(s) => ElevatedPermissions::Denied {
            reason: format!("pkexec chmod u+s failed ({s})"),
        },
        Err(e) => ElevatedPermissions::Denied {
            reason: format!("pkexec chmod: {e}"),
        },
    }
}

// --- legacy enum for older call sites ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivilegeOutcome {
    AlreadyPrivileged,
    Granted,
    ElevationLaunched { hint: String },
    Failed(String),
    Unsupported(&'static str),
}

pub fn request_core_privileges(core_path: &Path) -> PrivilegeOutcome {
    match get_elevated_permissions_for_core(core_path) {
        ElevatedPermissions::Ready => PrivilegeOutcome::AlreadyPrivileged,
        ElevatedPermissions::Reexecing => PrivilegeOutcome::ElevationLaunched {
            hint: "Restarting on a volume that supports Tun…".into(),
        },
        ElevatedPermissions::RetryAfterPassword { hint } => {
            PrivilegeOutcome::ElevationLaunched { hint }
        }
        ElevatedPermissions::Denied { reason } => PrivilegeOutcome::Failed(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_path_denied() {
        let r = get_elevated_permissions_for_core(Path::new("/nonexistent/ThroneCore-xyz"));
        assert!(matches!(r, ElevatedPermissions::Denied { .. }), "{r:?}");
    }

    #[test]
    fn setuid_helpers_on_missing() {
        assert!(!is_setuid_set(Path::new("/no/such/ThroneCore")));
        assert!(!core_is_root_setuid(Path::new("/no/such/ThroneCore")));
    }

    #[test]
    fn detects_nosuid_on_volumes_data_if_present() {
        let p = Path::new("/Volumes/data");
        if p.exists() {
            // This machine mounts /Volumes/data with nosuid.
            assert!(
                path_on_nosuid_volume(p),
                "/Volumes/data should be nosuid on this host"
            );
        }
    }
}
