use std::borrow::Cow;

use anyhow::Result;
use gpui::{AssetSource, SharedString};

/// App assets: Throne icons first, then [gpui-kit-assets] for IconName SVGs.
pub struct Assets;

fn load_throne(path: &str) -> Option<Cow<'static, [u8]>> {
    let bytes: &'static [u8] = match path {
        "icons/box.svg" => include_bytes!("../assets/icons/box.svg"),
        "icons/settings.svg" => include_bytes!("../assets/icons/settings.svg"),
        "icons/layers.svg" => include_bytes!("../assets/icons/layers.svg"),
        "icons/route.svg" => include_bytes!("../assets/icons/route.svg"),
        "icons/wrench.svg" => include_bytes!("../assets/icons/wrench.svg"),
        "icons/play.svg" => include_bytes!("../assets/icons/play.svg"),
        "icons/square.svg" => include_bytes!("../assets/icons/square.svg"),
        "icons/loader.svg" => include_bytes!("../assets/icons/loader.svg"),
        "icons/copy.svg" => include_bytes!("../assets/icons/copy.svg"),
        "icons/trash.svg" => include_bytes!("../assets/icons/trash.svg"),
        _ => return None,
    };
    Some(Cow::Borrowed(bytes))
}

const THRONE_ICON_NAMES: &[&str] = &[
    "box.svg",
    "settings.svg",
    "layers.svg",
    "route.svg",
    "wrench.svg",
    "play.svg",
    "square.svg",
    "loader.svg",
    "copy.svg",
    "trash.svg",
];

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = load_throne(path) {
            return Ok(Some(bytes));
        }
        // Lucide set used by gpui-component IconName (Spinner → icons/loader.svg, etc.).
        match gpui_kit_assets::Assets.load(path) {
            Ok(data) => Ok(data),
            Err(_) => Ok(None),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut names: Vec<SharedString> = if path == "icons" || path.is_empty() {
            THRONE_ICON_NAMES
                .iter()
                .map(|n| SharedString::from(*n))
                .collect()
        } else {
            Vec::new()
        };

        if let Ok(component) = gpui_kit_assets::Assets.list(path) {
            for name in component {
                if !names
                    .iter()
                    .any(|existing| existing.as_ref() == name.as_ref())
                {
                    names.push(name);
                }
            }
        }
        Ok(names)
    }
}
