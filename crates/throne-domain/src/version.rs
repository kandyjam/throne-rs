//! Application version aligned with upstream Throne `NKR_VERSION`.
//!
//! Upstream injects the release tag via CMake `INPUT_VERSION` / `NKR_VERSION`
//! (see `cmake/nkr.cmake`). We use the Cargo package version for the same role.

/// Full version string, e.g. `4.3.7` or `4.3.7-rs.1` if a pre-release suffix is set.
pub const NKR_VERSION: &str = env!("CARGO_PKG_VERSION");

/// User-Agent / compact product id: `Throne/<version>` (upstream SettingsRepo).
pub fn user_agent() -> String {
    let base = NKR_VERSION.split('-').next().unwrap_or(NKR_VERSION);
    format!("Throne/{base}")
}

/// Window / tray title fragment used by upstream `refresh_status`.
pub fn display_name() -> String {
    format!("Throne {NKR_VERSION}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_semver_like() {
        let parts: Vec<_> = NKR_VERSION.split('.').collect();
        assert!(parts.len() >= 2, "expected major.minor… got {NKR_VERSION}");
        assert!(parts[0].chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn user_agent_prefix() {
        assert!(user_agent().starts_with("Throne/"));
    }
}
