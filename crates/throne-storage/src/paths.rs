//! Locate original Throne `throne.db` files the same way the Qt client does.
//!
//! Upstream `main.cpp`:
//! - cwd becomes `<base>/config`
//! - DB path = `./throne.db` under that cwd
//! - base = applicationDirPath (portable) **or** AppConfigLocation when `-appdata` / packaged

use std::env;
use std::path::{Path, PathBuf};

/// Env override used by throne-rs (and useful for pointing at a portable install).
pub const ENV_DB: &str = "THRONE_DB";

/// Resolve which database file to open.
///
/// Priority:
/// 1. `$THRONE_DB`
/// 2. First **valid** Throne DB from [`discover_throne_databases`]
///    (skips empty/0-byte stubs that some packagers leave under
///    `Application Support/Throne/throne.db`)
/// 3. Fresh path under OS data dir: `…/throne-rs/throne.db` (new installs)
pub fn resolve_db_path() -> PathBuf {
    if let Ok(p) = env::var(ENV_DB) {
        let path = PathBuf::from(p);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    if let Some(existing) = discover_throne_databases()
        .into_iter()
        .find(|p| p.is_file() && is_usable_throne_db(p))
    {
        return existing;
    }
    default_db_path()
}

/// True when `path` is a non-empty SQLite file that looks like a Throne DB.
///
/// An empty touch-created `throne.db` (0 bytes) must not win over a real
/// `throne-rs/throne.db` — that made Tun/Start load a blank profile set.
fn is_usable_throne_db(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if meta.len() < 100 {
        return false;
    }
    // Inline check (avoid circular crate path through Database): open and
    // require profiles + settings tables.
    let Ok(conn) = rusqlite::Connection::open(path) else {
        return false;
    };
    let Ok(n): Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('profiles','groups','settings')",
        [],
        |r| r.get(0),
    ) else {
        return false;
    };
    n >= 2
}

/// New/default location when no legacy DB is found.
pub fn default_db_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("throne-rs").join("throne.db")
}

/// Candidate locations for an existing upstream Throne database.
pub fn discover_throne_databases() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut push = |p: PathBuf| {
        if !out.iter().any(|x| x == &p) {
            out.push(p);
        }
    };

    // 1) Beside the running binary: <exe_dir>/config/throne.db (portable)
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            push(dir.join("config").join("throne.db"));
            push(dir.join("throne.db"));
        }
    }

    // 2) CWD variants (dev / launched from install dir)
    if let Ok(cwd) = env::current_dir() {
        push(cwd.join("config").join("throne.db"));
        push(cwd.join("throne.db"));
        // parent/config when cwd is already config/
        if cwd.ends_with("config") {
            push(cwd.join("throne.db"));
        }
    }

    // 3) Qt AppConfigLocation equivalents for applicationName "Throne"
    //    Linux:   ~/.config/Throne/config/throne.db  (and sometimes ~/.config/Throne/throne.db)
    //    macOS:   ~/Library/Preferences/Throne/… is NOT AppConfig; Qt uses
    //             ~/Library/Application Support/Throne on some builds, and
    //             ~/Library/Preferences is for QSettings. AppConfigLocation on
    //             macOS is typically ~/Library/Preferences/<org>/<app> OR
    //             Application Support depending on Qt version.
    //    We probe the common ones thronged by Qt 5/6 + packaging.
    for base in app_config_bases() {
        push(base.join("Throne").join("config").join("throne.db"));
        push(base.join("Throne").join("throne.db"));
        push(base.join("throneproj").join("Throne").join("config").join("throne.db"));
        push(base.join("config").join("Throne").join("config").join("throne.db"));
    }

    // 4) macOS Application Support (packaged NKR_CPP_USE_APPDATA style)
    if let Some(home) = dirs::home_dir() {
        push(
            home.join("Library/Application Support/Throne/config/throne.db"),
        );
        push(home.join("Library/Application Support/Throne/throne.db"));
        push(
            home.join("Library/Preferences/Throne/config/throne.db"),
        );
        // Linux XDG
        push(home.join(".config/Throne/config/throne.db"));
        push(home.join(".config/Throne/throne.db"));
        push(home.join(".local/share/Throne/config/throne.db"));
    }

    // 5) Windows-style Roaming (when running under wine / cross)
    if let Ok(appdata) = env::var("APPDATA") {
        let a = PathBuf::from(appdata);
        push(a.join("Throne").join("config").join("throne.db"));
        push(a.join("Throne").join("throne.db"));
    }
    if let Ok(local) = env::var("LOCALAPPDATA") {
        let a = PathBuf::from(local);
        push(a.join("Throne").join("config").join("throne.db"));
    }

    out
}

fn app_config_bases() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(c) = dirs::config_dir() {
        v.push(c);
    }
    if let Some(d) = dirs::data_dir() {
        v.push(d);
    }
    if let Some(h) = dirs::home_dir() {
        v.push(h.join(".config"));
    }
    v
}

/// Sibling stats DB path (upstream `throne_stats.db`).
#[allow(dead_code)]
pub fn stats_db_path(main_db: &Path) -> PathBuf {
    main_db
        .parent()
        .map(|p| p.join("throne_stats.db"))
        .unwrap_or_else(|| PathBuf::from("throne_stats.db"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_returns_unique_paths() {
        let list = discover_throne_databases();
        let mut sorted = list.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(list.len(), sorted.len());
        assert!(list.iter().any(|p| p.ends_with("throne.db")));
    }
}
