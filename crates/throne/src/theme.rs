//! Dark theme tokens inspired by modern proxy clients / Zed.

use gpui::{Hsla, rgb};

pub struct Theme;

impl Theme {
    pub fn bg_app() -> Hsla {
        rgb(0x1a1b1e).into()
    }

    pub fn bg_panel() -> Hsla {
        rgb(0x222327).into()
    }

    pub fn bg_elevated() -> Hsla {
        rgb(0x2a2b30).into()
    }

    pub fn bg_hover() -> Hsla {
        rgb(0x32333a).into()
    }

    pub fn bg_selected() -> Hsla {
        rgb(0x2d3a4f).into()
    }

    pub fn border() -> Hsla {
        rgb(0x3a3b42).into()
    }

    pub fn text() -> Hsla {
        rgb(0xe8e8ed).into()
    }

    pub fn text_muted() -> Hsla {
        rgb(0x9a9aa3).into()
    }

    pub fn accent() -> Hsla {
        rgb(0x5b8def).into()
    }

    pub fn accent_soft() -> Hsla {
        rgb(0x3d5a8a).into()
    }

    pub fn success() -> Hsla {
        rgb(0x3dd68c).into()
    }

    #[allow(dead_code)]
    pub fn danger() -> Hsla {
        rgb(0xf07178).into()
    }

    #[allow(dead_code)]
    pub fn warning() -> Hsla {
        rgb(0xe6b450).into()
    }

    pub fn latency_good() -> Hsla {
        rgb(0x3dd68c).into()
    }

    pub fn latency_ok() -> Hsla {
        rgb(0xe6b450).into()
    }

    pub fn latency_bad() -> Hsla {
        rgb(0xf07178).into()
    }

    pub fn latency_none() -> Hsla {
        Self::text_muted()
    }
}

pub fn latency_color(ms: i32) -> Hsla {
    match ms {
        0 => Theme::latency_none(),
        n if n < 0 => Theme::latency_bad(),
        n if n < 80 => Theme::latency_good(),
        n if n < 150 => Theme::latency_ok(),
        _ => Theme::latency_bad(),
    }
}
