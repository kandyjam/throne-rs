//! Light / dark color tokens.
//!
//! Active scheme is resolved from `AppSettings.theme` plus the OS appearance
//! (GPUI [`WindowAppearance`]), matching upstream Throne's "System" theme.

use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{rgb, Hsla, WindowAppearance};

/// Whether the UI currently paints with the dark palette.
static ACTIVE_DARK: AtomicBool = AtomicBool::new(false);

/// Resolved UI brightness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorScheme {
    Light,
    Dark,
}

impl ColorScheme {
    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }
}

/// True when the platform reports a dark (or vibrant-dark) window appearance.
pub fn system_is_dark(appearance: WindowAppearance) -> bool {
    matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

/// Map a persisted theme preference + current OS appearance to a scheme.
///
/// Aligns with upstream `themeUsesDarkLog` / ThemeManager:
/// - forced dark: `QDarkStyle`, `BlackSoft`, `dark`
/// - forced light: `FlatGray`, `LightBlue`, `SoftPink`, `light`
/// - otherwise (`System`, empty, Fusion, …): follow the OS
pub fn resolve_scheme(theme_setting: &str, system_dark: bool) -> ColorScheme {
    let lower = theme_setting.trim().to_ascii_lowercase();
    if lower.is_empty() || lower == "0" || lower == "system" {
        return if system_dark {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        };
    }
    if lower.contains("qdarkstyle") || lower.contains("blacksoft") || lower == "dark" {
        return ColorScheme::Dark;
    }
    if lower.contains("flatgray")
        || lower.contains("lightblue")
        || lower.contains("softpink")
        || lower.contains("vista")
        || lower == "light"
    {
        return ColorScheme::Light;
    }
    // Bi-mode styles (Fusion, OS styles, unknown): follow system.
    if system_dark {
        ColorScheme::Dark
    } else {
        ColorScheme::Light
    }
}

/// Publish the active scheme for [`Theme`] token lookups during paint.
pub fn set_active_scheme(scheme: ColorScheme) {
    ACTIVE_DARK.store(scheme.is_dark(), Ordering::Relaxed);
}

/// Apply preference + OS appearance in one step.
pub fn apply_preference(theme_setting: &str, system_dark: bool) -> ColorScheme {
    let scheme = resolve_scheme(theme_setting, system_dark);
    set_active_scheme(scheme);
    scheme
}

pub fn active_scheme() -> ColorScheme {
    if ACTIVE_DARK.load(Ordering::Relaxed) {
        ColorScheme::Dark
    } else {
        ColorScheme::Light
    }
}

/// Color tokens. Methods read the process-wide active scheme (set via
/// [`apply_preference`] before painting).
pub struct Theme;

impl Theme {
    pub fn bg_app() -> Hsla {
        match active_scheme() {
            ColorScheme::Light => rgb(0xf0f0f0).into(),
            // Upstream QDarkStyle window
            ColorScheme::Dark => rgb(0x19232d).into(),
        }
    }

    pub fn bg_panel() -> Hsla {
        match active_scheme() {
            ColorScheme::Light => rgb(0xe8e8e8).into(),
            ColorScheme::Dark => rgb(0x1c2834).into(),
        }
    }

    pub fn bg_elevated() -> Hsla {
        match active_scheme() {
            ColorScheme::Light => rgb(0xffffff).into(),
            ColorScheme::Dark => rgb(0x243447).into(),
        }
    }

    pub fn bg_hover() -> Hsla {
        match active_scheme() {
            ColorScheme::Light => rgb(0xdce8f8).into(),
            ColorScheme::Dark => rgb(0x1a72bb).into(),
        }
    }

    pub fn bg_selected() -> Hsla {
        match active_scheme() {
            ColorScheme::Light => rgb(0x3d7eff).into(),
            ColorScheme::Dark => rgb(0x346792).into(),
        }
    }

    pub fn bg_toolbar_btn() -> Hsla {
        match active_scheme() {
            ColorScheme::Light => rgb(0xf7f7f7).into(),
            ColorScheme::Dark => rgb(0x455364).into(),
        }
    }

    pub fn border_light() -> Hsla {
        match active_scheme() {
            ColorScheme::Light => rgb(0xb0b0b0).into(),
            ColorScheme::Dark => rgb(0x54687a).into(),
        }
    }

    pub fn text() -> Hsla {
        match active_scheme() {
            ColorScheme::Light => rgb(0x202020).into(),
            ColorScheme::Dark => rgb(0xdfe1e2).into(),
        }
    }

    /// Toolbar / inline glyph color. Tracks [`Self::text`] so SVG icons flip with light/dark.
    pub fn icon() -> Hsla {
        Self::text()
    }

    pub fn text_muted() -> Hsla {
        match active_scheme() {
            ColorScheme::Light => rgb(0x606060).into(),
            ColorScheme::Dark => rgb(0x9da9b5).into(),
        }
    }

    pub fn text_on_selected() -> Hsla {
        rgb(0xffffff).into()
    }

    pub fn accent() -> Hsla {
        match active_scheme() {
            ColorScheme::Light => rgb(0x2a6af0).into(),
            ColorScheme::Dark => rgb(0x5ab0ff).into(),
        }
    }

    pub fn accent_soft() -> Hsla {
        match active_scheme() {
            ColorScheme::Light => rgb(0xc8daf8).into(),
            ColorScheme::Dark => rgb(0x26486b).into(),
        }
    }

    pub fn success() -> Hsla {
        match active_scheme() {
            ColorScheme::Light => rgb(0x2e8b57).into(),
            ColorScheme::Dark => rgb(0x3dd68c).into(),
        }
    }

    pub fn danger() -> Hsla {
        match active_scheme() {
            ColorScheme::Light => rgb(0xc0392b).into(),
            ColorScheme::Dark => rgb(0xff6b6b).into(),
        }
    }

    pub fn warning() -> Hsla {
        match active_scheme() {
            ColorScheme::Light => rgb(0xd48806).into(),
            ColorScheme::Dark => rgb(0xf0b429).into(),
        }
    }
}

pub fn latency_color(ms: i32) -> Hsla {
    match ms {
        0 => Theme::text_muted(),
        n if n < 0 => Theme::danger(),
        n if n < 80 => Theme::success(),
        n if n < 150 => Theme::warning(),
        _ => Theme::danger(),
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_preference, resolve_scheme, set_active_scheme, ColorScheme};

    #[test]
    fn system_preference_follows_os() {
        assert_eq!(resolve_scheme("System", true), ColorScheme::Dark);
        assert_eq!(resolve_scheme("System", false), ColorScheme::Light);
        assert_eq!(resolve_scheme("0", true), ColorScheme::Dark);
        assert_eq!(resolve_scheme("", false), ColorScheme::Light);
    }

    #[test]
    fn named_themes_force_brightness() {
        assert_eq!(resolve_scheme("QDarkStyle", false), ColorScheme::Dark);
        assert_eq!(resolve_scheme("blacksoft", false), ColorScheme::Dark);
        assert_eq!(resolve_scheme("dark", false), ColorScheme::Dark);
        assert_eq!(resolve_scheme("FlatGray", true), ColorScheme::Light);
        assert_eq!(resolve_scheme("LightBlue", true), ColorScheme::Light);
        assert_eq!(resolve_scheme("softpink", true), ColorScheme::Light);
        assert_eq!(resolve_scheme("light", true), ColorScheme::Light);
    }

    #[test]
    fn unknown_styles_follow_system() {
        assert_eq!(resolve_scheme("Fusion", true), ColorScheme::Dark);
        assert_eq!(resolve_scheme("windows11", false), ColorScheme::Light);
    }

    #[test]
    fn apply_preference_updates_active_scheme() {
        set_active_scheme(ColorScheme::Light);
        assert_eq!(apply_preference("System", true), ColorScheme::Dark);
        assert!(super::active_scheme().is_dark());
        assert_eq!(apply_preference("FlatGray", true), ColorScheme::Light);
        assert!(!super::active_scheme().is_dark());
    }
}
