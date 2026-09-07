//! Dock icon light/dark switch (macOS).
//!
//! Loads light/dark `.icns` and assigns them with an explicit **point size**
//! taken from the stock `applicationIconImage` (before we touch it).
//!
//! Do **not** use `lockFocus` / redraw tricks — on modern macOS that often
//! yields an empty `NSImage`, and the Dock then shows a generic **folder** tile.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI8, AtomicU64, Ordering};

/// -1 unset, 0 light, 1 dark
static APPLIED: AtomicI8 = AtomicI8::new(-1);
/// Packed f32 width/height of stock dock icon points (0 = unset).
static ORIG_SIZE_BITS: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use cocoa::appkit::{NSApp, NSApplication, NSImage};
    use cocoa::base::{id, nil};
    use cocoa::foundation::{NSSize, NSString};
    use objc::{msg_send, sel, sel_impl};

    const LIGHT_ICNS: &[u8] = include_bytes!("../resources/app-icon-light.icns");
    const DARK_ICNS: &[u8] = include_bytes!("../resources/app-icon-dark.icns");
    /// Safe default when stock size cannot be read. Matches typical app icon pts.
    const FALLBACK_PT: f64 = 128.0;

    fn pack_size(w: f64, h: f64) -> u64 {
        let wb = (w as f32).to_bits() as u64;
        let hb = (h as f32).to_bits() as u64;
        (wb << 32) | hb
    }

    fn unpack_size(bits: u64) -> (f64, f64) {
        let w = f32::from_bits((bits >> 32) as u32) as f64;
        let h = f32::from_bits(bits as u32) as f64;
        (w, h)
    }

    unsafe fn capture_original_size_if_needed(app: id) {
        if ORIG_SIZE_BITS.load(Ordering::Relaxed) != 0 {
            return;
        }
        let current: id = msg_send![app, applicationIconImage];
        let (mut w, mut h) = (FALLBACK_PT, FALLBACK_PT);
        if current != nil {
            let valid: bool = msg_send![current, isValid];
            if valid {
                let size: NSSize = msg_send![current, size];
                if size.width.is_finite() && size.height.is_finite() && size.width >= 16.0 {
                    w = size.width;
                    h = size.height;
                }
            }
        }
        // Ignore already-bloated sizes from a previous bad setApplicationIconImage.
        if !(32.0..=256.0).contains(&w) || !(32.0..=256.0).contains(&h) {
            w = FALLBACK_PT;
            h = FALLBACK_PT;
        }
        ORIG_SIZE_BITS.store(pack_size(w, h), Ordering::Relaxed);
        tracing::debug!(w, h, "dock icon stock point size");
    }

    fn target_size() -> NSSize {
        let bits = ORIG_SIZE_BITS.load(Ordering::Relaxed);
        let (w, h) = if bits == 0 {
            (FALLBACK_PT, FALLBACK_PT)
        } else {
            unpack_size(bits)
        };
        NSSize::new(w, h)
    }

    fn bundle_resources_dir() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let exe = exe.canonicalize().ok().unwrap_or(exe);
        let macos_dir = exe.parent()?;
        if macos_dir.file_name()?.to_str()? != "MacOS" {
            return None;
        }
        let contents = macos_dir.parent()?;
        let resources = contents.join("Resources");
        resources.is_dir().then_some(resources)
    }

    fn icon_path_for(dark: bool) -> Option<PathBuf> {
        let names = if dark {
            ["AppIcon-dark.icns", "app-icon-dark.icns"]
        } else {
            ["AppIcon-light.icns", "app-icon-light.icns"]
        };
        if let Some(res) = bundle_resources_dir() {
            for name in names {
                let p = res.join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        embedded_icns_path(dark).ok()
    }

    fn embedded_icns_path(dark: bool) -> std::io::Result<PathBuf> {
        let bytes = if dark { DARK_ICNS } else { LIGHT_ICNS };
        let name = if dark {
            "throne-app-icon-dark.icns"
        } else {
            "throne-app-icon-light.icns"
        };
        let dir = std::env::temp_dir().join("throne-dock-icons");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(name);
        let need_write = match std::fs::metadata(&path) {
            Ok(m) => m.len() as usize != bytes.len(),
            Err(_) => true,
        };
        if need_write {
            std::fs::write(&path, bytes)?;
        }
        Ok(path)
    }

    unsafe fn nsimage_from_file(path: &Path) -> id {
        let Some(path_str) = path.to_str() else {
            return nil;
        };
        let ns_path: id = NSString::alloc(nil).init_str(path_str);
        if ns_path == nil {
            return nil;
        }
        NSImage::initWithContentsOfFile_(NSImage::alloc(nil), ns_path)
    }

    pub fn apply(dark: bool) {
        let Some(path) = icon_path_for(dark) else {
            tracing::warn!("dock icon file not found for dark={dark}");
            return;
        };
        unsafe {
            let app = NSApp();
            if app == nil {
                return;
            }
            capture_original_size_if_needed(app);

            let image = nsimage_from_file(&path);
            if image == nil {
                tracing::warn!(path = %path.display(), "failed to load dock icns");
                return;
            }
            let valid: bool = msg_send![image, isValid];
            if !valid {
                tracing::warn!(path = %path.display(), "dock icns NSImage is not valid — skip");
                return;
            }

            // Pin point size to stock Dock icon; do not lockFocus/redraw.
            let size = target_size();
            let _: () = msg_send![image, setSize: size];

            app.setApplicationIconImage_(image);
            tracing::debug!(
                path = %path.display(),
                dark,
                w = size.width,
                h = size.height,
                "dock icon applied"
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod macos {
    pub fn apply(_dark: bool) {}
}

/// Switch Dock icon for light/dark at the stock Dock tile size.
///
/// When `follow_status` is false (upstream `follow_status_in_taskbar`), leave
/// the stock icon alone.
pub fn apply_for_scheme(is_dark: bool, follow_status: bool) {
    if !follow_status {
        APPLIED.store(-1, Ordering::Relaxed);
        return;
    }
    let tag: i8 = if is_dark { 1 } else { 0 };
    if APPLIED.load(Ordering::Relaxed) == tag {
        return;
    }
    macos::apply(is_dark);
    APPLIED.store(tag, Ordering::Relaxed);
}
