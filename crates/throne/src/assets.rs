use std::borrow::Cow;

use anyhow::Result;
use gpui::{AssetSource, SharedString};

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes = match path {
            "icons/box.svg" => include_bytes!("../assets/icons/box.svg").as_slice(),
            "icons/settings.svg" => include_bytes!("../assets/icons/settings.svg").as_slice(),
            "icons/layers.svg" => include_bytes!("../assets/icons/layers.svg").as_slice(),
            "icons/route.svg" => include_bytes!("../assets/icons/route.svg").as_slice(),
            "icons/wrench.svg" => include_bytes!("../assets/icons/wrench.svg").as_slice(),
            "icons/play.svg" => include_bytes!("../assets/icons/play.svg").as_slice(),
            "icons/square.svg" => include_bytes!("../assets/icons/square.svg").as_slice(),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        if path == "icons" {
            Ok([
                "box.svg",
                "settings.svg",
                "layers.svg",
                "route.svg",
                "wrench.svg",
                "play.svg",
                "square.svg",
            ]
            .into_iter()
            .map(SharedString::from)
            .collect())
        } else {
            Ok(Vec::new())
        }
    }
}
