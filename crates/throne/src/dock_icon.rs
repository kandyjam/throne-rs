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

/// Ignore already-bloated sizes from a previous bad `setApplicationIconImage`.
fn clamp_stock_point_size(w: f64, h: f64) -> (f64, f64) {
    if (32.0..=256.0).contains(&w) && (32.0..=256.0).contains(&h) {
        (w, h)
    } else {
        (FALLBACK_PT, FALLBACK_PT)
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use objc2::rc::Retained;
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::{NSSize, NSString};

    const LIGHT_ICNS: &[u8] = include_bytes!("../resources/app-icon-light.icns");
    const DARK_ICNS: &[u8] = include_bytes!("../resources/app-icon-dark.icns");

    fn capture_original_size_if_needed(app: &NSApplication) {
        if ORIG_SIZE_BITS.load(Ordering::Relaxed) != 0 {
            return;
        }
        let (mut w, mut h) = (FALLBACK_PT, FALLBACK_PT);
        if let Some(current) = app.applicationIconImage() {
            if current.isValid() {
                let size = current.size();
                if size.width.is_finite() && size.height.is_finite() && size.width >= 16.0 {
                    w = size.width;
                    h = size.height;
                }
            }
        }
        let (w, h) = clamp_stock_point_size(w, h);
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

    fn nsimage_from_file(path: &Path) -> Option<Retained<NSImage>> {
        let path_str = path.to_str()?;
        let ns_path = NSString::from_str(path_str);
        NSImage::initWithContentsOfFile(NSImage::alloc(), &ns_path)
    }

    pub fn apply(dark: bool) -> bool {
        let Some(path) = icon_path_for(dark) else {
            tracing::warn!("dock icon file not found for dark={dark}");
            return false;
        };
        let Some(mtm) = MainThreadMarker::new() else {
            tracing::warn!("dock icon apply skipped: not on the main thread");
            return false;
        };
        let app = NSApplication::sharedApplication(mtm);
        capture_original_size_if_needed(&app);

        let Some(image) = nsimage_from_file(&path) else {
            tracing::warn!(path = %path.display(), "failed to load dock icns");
            return false;
        };
        if !image.isValid() {
            tracing::warn!(path = %path.display(), "dock icns NSImage is not valid — skip");
            return false;
        }

        // Pin point size to stock Dock icon; do not lockFocus/redraw.
        let size = target_size();
        image.setSize(size);
        // SAFETY: objc2 marks this unsafe because the image argument's
        // nullability is uncertain. We pass Some(&image) from a Retained
        // NSImage that outlives the call (AppKit retains it).
        unsafe {
            app.setApplicationIconImage(Some(&image));
        }
        tracing::debug!(
            path = %path.display(),
            dark,
            w = size.width,
            h = size.height,
            "dock icon applied"
        );
        true
    }
}

#[cfg(not(target_os = "macos"))]
mod macos {
    pub fn apply(_dark: bool) -> bool {
        true
    }
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
    if macos::apply(is_dark) {
        APPLIED.store(tag, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_size_roundtrips_finite_points() {
        let bits = pack_size(128.0, 128.0);
        let (w, h) = unpack_size(bits);
        assert!((w - 128.0).abs() < 0.01);
        assert!((h - 128.0).abs() < 0.01);

        let bits = pack_size(32.0, 256.0);
        let (w, h) = unpack_size(bits);
        assert!((w - 32.0).abs() < 0.01);
        assert!((h - 256.0).abs() < 0.01);

        assert_eq!(pack_size(0.0, 0.0), 0);
    }

    #[test]
    fn clamp_stock_point_size_keeps_typical_dock_tile() {
        assert_eq!(clamp_stock_point_size(128.0, 128.0), (128.0, 128.0));
        assert_eq!(clamp_stock_point_size(32.0, 32.0), (32.0, 32.0));
        assert_eq!(clamp_stock_point_size(256.0, 256.0), (256.0, 256.0));
    }

    #[test]
    fn clamp_stock_point_size_rejects_bloated_or_tiny() {
        assert_eq!(
            clamp_stock_point_size(1024.0, 1024.0),
            (FALLBACK_PT, FALLBACK_PT)
        );
        assert_eq!(
            clamp_stock_point_size(16.0, 16.0),
            (FALLBACK_PT, FALLBACK_PT)
        );
        assert_eq!(
            clamp_stock_point_size(128.0, 512.0),
            (FALLBACK_PT, FALLBACK_PT)
        );
    }

    #[test]
    fn apply_for_scheme_skips_when_follow_status_disabled() {
        apply_for_scheme(true, false);
        assert_eq!(APPLIED.load(Ordering::Relaxed), -1);
        apply_for_scheme(false, false);
        assert_eq!(APPLIED.load(Ordering::Relaxed), -1);
    }
}
