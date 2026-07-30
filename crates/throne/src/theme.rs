//! Colors closer to default Qt Fusion / Throne desktop chrome.

use gpui::{Hsla, rgb};

pub struct Theme;

impl Theme {
    pub fn bg_app() -> Hsla {
        rgb(0xf0f0f0).into()
    }

    pub fn bg_panel() -> Hsla {
        rgb(0xe8e8e8).into()
    }

    pub fn bg_elevated() -> Hsla {
        rgb(0xffffff).into()
    }

    pub fn bg_hover() -> Hsla {
        rgb(0xdce8f8).into()
    }

    pub fn bg_selected() -> Hsla {
        rgb(0x3d7eff).into()
    }

    pub fn bg_selected_text() -> Hsla {
        rgb(0xffffff).into()
    }

    pub fn bg_toolbar_btn() -> Hsla {
        rgb(0xf7f7f7).into()
    }

    pub fn border() -> Hsla {
        rgb(0x777777).into()
    }

    pub fn border_light() -> Hsla {
        rgb(0xb0b0b0).into()
    }

    pub fn text() -> Hsla {
        rgb(0x202020).into()
    }

    pub fn text_muted() -> Hsla {
        rgb(0x606060).into()
    }

    pub fn text_on_selected() -> Hsla {
        rgb(0xffffff).into()
    }

    pub fn accent() -> Hsla {
        rgb(0x2a6af0).into()
    }

    pub fn accent_soft() -> Hsla {
        rgb(0xc8daf8).into()
    }

    pub fn success() -> Hsla {
        rgb(0x2e8b57).into()
    }

    pub fn danger() -> Hsla {
        rgb(0xc0392b).into()
    }

    pub fn warning() -> Hsla {
        rgb(0xd48806).into()
    }

    pub fn start_green() -> Hsla {
        rgb(0x27ae60).into()
    }

    pub fn stop_red() -> Hsla {
        rgb(0xe74c3c).into()
    }

    pub fn tab_selected_border() -> Hsla {
        rgb(0x3d7eff).into()
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
