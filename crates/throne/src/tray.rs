//! System tray — menu layout mirrors upstream Throne `MainWindow` tray setup:
//!
//! ```text
//! Show Window
//! ───────────
//! Start with system              [check]
//! Remember last profile          [check]
//! Allow other devices to connect [check]
//! ───────────
//! Select Server
//! Select Routing
//! System Proxy ▶
//!   Enable System Proxy          [check]
//!   Enable Tun                   [check]
//!   Disable                      [check]
//! ───────────
//! Restart Core
//! Restart Program
//! Exit
//! ```
//!
//! Source: `throneproj/Throne` `src/ui/mainwindow.cpp` (Setup Tray).

use std::cell::RefCell;
use std::sync::atomic::{AtomicU8, Ordering};

use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

use crate::theme::ColorScheme;

// ── Menu item ids (stable strings for MenuEvent routing) ─────────────────

pub const SHOW_WINDOW_ID: &str = "throne.show";
pub const START_WITH_SYSTEM_ID: &str = "throne.start_with_system";
pub const REMEMBER_LAST_ID: &str = "throne.remember_last";
pub const ALLOW_LAN_ID: &str = "throne.allow_lan";
pub const SELECT_SERVER_ID: &str = "throne.select_server";
pub const SELECT_ROUTING_ID: &str = "throne.select_routing";
pub const SP_SYSTEM_PROXY_ID: &str = "throne.sp.system_proxy";
pub const SP_TUN_ID: &str = "throne.sp.tun";
pub const SP_DISABLED_ID: &str = "throne.sp.disabled";
pub const RESTART_CORE_ID: &str = "throne.restart_core";
pub const RESTART_PROGRAM_ID: &str = "throne.restart_program";
pub const EXIT_ID: &str = "throne.exit";

/// White crown line-art (alpha mask). Used as macOS template and dark-bar glyph.
/// 64×64 for sharper menu-bar rendering on retina displays.
const TRAY_ICON_ON_DARK: &[u8] = include_bytes!("../assets/tray-icon-on-dark.rgba");
/// Near-black crown line-art for light menu bars / taskbars (Win/Linux).
const TRAY_ICON_ON_LIGHT: &[u8] = include_bytes!("../assets/tray-icon-on-light.rgba");
const TRAY_ICON_SIZE: u32 = 64;

struct TrayHandles {
    tray: TrayIcon,
    start_with_system: CheckMenuItem,
    remember_last: CheckMenuItem,
    allow_lan: CheckMenuItem,
    sp_system_proxy: CheckMenuItem,
    sp_tun: CheckMenuItem,
    sp_disabled: CheckMenuItem,
}

// tray-icon::TrayIcon is !Send/!Sync — keep it on the UI thread only.
thread_local! {
    static TRAY: RefCell<Option<TrayHandles>> = const { RefCell::new(None) };
}

/// 0 = unset, 1 = light, 2 = dark — avoid redundant set_icon calls.
static APPLIED_SCHEME: AtomicU8 = AtomicU8::new(0);

/// Snapshot used to paint checkmarks (upstream reads SettingsRepo on show).
#[derive(Debug, Clone, Copy, Default)]
pub struct TrayMenuState {
    pub start_with_system: bool,
    pub remember_last: bool,
    pub allow_lan: bool,
    pub system_proxy: bool,
    pub tun: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    ShowWindow,
    ToggleStartWithSystem,
    ToggleRememberLast,
    ToggleAllowLan,
    SelectServer,
    SelectRouting,
    EnableSystemProxy,
    EnableTun,
    DisableSpMode,
    RestartCore,
    RestartProgram,
    Exit,
}

pub fn command_from_menu_id(id: &str) -> Option<TrayCommand> {
    match id {
        SHOW_WINDOW_ID => Some(TrayCommand::ShowWindow),
        START_WITH_SYSTEM_ID => Some(TrayCommand::ToggleStartWithSystem),
        REMEMBER_LAST_ID => Some(TrayCommand::ToggleRememberLast),
        ALLOW_LAN_ID => Some(TrayCommand::ToggleAllowLan),
        SELECT_SERVER_ID => Some(TrayCommand::SelectServer),
        SELECT_ROUTING_ID => Some(TrayCommand::SelectRouting),
        SP_SYSTEM_PROXY_ID => Some(TrayCommand::EnableSystemProxy),
        SP_TUN_ID => Some(TrayCommand::EnableTun),
        SP_DISABLED_ID => Some(TrayCommand::DisableSpMode),
        RESTART_CORE_ID => Some(TrayCommand::RestartCore),
        RESTART_PROGRAM_ID => Some(TrayCommand::RestartProgram),
        EXIT_ID => Some(TrayCommand::Exit),
        _ => None,
    }
}

/// Build the upstream-aligned tray menu and install the status item.
pub fn install(initial: TrayMenuState) -> Result<(), String> {
    let show_window = MenuItem::with_id(SHOW_WINDOW_ID, "Show Window", true, None);

    let start_with_system = CheckMenuItem::with_id(
        START_WITH_SYSTEM_ID,
        "Start with system",
        true,
        initial.start_with_system,
        None,
    );
    let remember_last = CheckMenuItem::with_id(
        REMEMBER_LAST_ID,
        "Remember last profile",
        true,
        initial.remember_last,
        None,
    );
    let allow_lan = CheckMenuItem::with_id(
        ALLOW_LAN_ID,
        "Allow other devices to connect",
        true,
        initial.allow_lan,
        None,
    );

    let select_server = MenuItem::with_id(SELECT_SERVER_ID, "Select Server", true, None);
    let select_routing = MenuItem::with_id(SELECT_ROUTING_ID, "Select Routing", true, None);

    let sp_system_proxy = CheckMenuItem::with_id(
        SP_SYSTEM_PROXY_ID,
        "Enable System Proxy",
        true,
        initial.system_proxy,
        None,
    );
    let sp_tun = CheckMenuItem::with_id(SP_TUN_ID, "Enable Tun", true, initial.tun, None);
    let sp_disabled = CheckMenuItem::with_id(
        SP_DISABLED_ID,
        "Disable",
        true,
        !initial.system_proxy && !initial.tun,
        None,
    );

    let sp_menu = Submenu::with_id("throne.spmode", "System Proxy", true);
    sp_menu
        .append_items(&[&sp_system_proxy, &sp_tun, &sp_disabled])
        .map_err(|error| format!("build System Proxy submenu: {error}"))?;

    let restart_core = MenuItem::with_id(RESTART_CORE_ID, "Restart Core", true, None);
    let restart_program = MenuItem::with_id(RESTART_PROGRAM_ID, "Restart Program", true, None);
    let exit = MenuItem::with_id(EXIT_ID, "Exit", true, None);

    let sep1 = PredefinedMenuItem::separator();
    let sep2 = PredefinedMenuItem::separator();
    let sep3 = PredefinedMenuItem::separator();

    let menu = Menu::new();
    menu.append_items(&[
        &show_window,
        &sep1,
        &start_with_system,
        &remember_last,
        &allow_lan,
        &sep2,
        &select_server,
        &select_routing,
        &sp_menu,
        &sep3,
        &restart_core,
        &restart_program,
        &exit,
    ])
    .map_err(|error| format!("build tray menu: {error}"))?;

    let scheme = crate::theme::active_scheme();
    let (rgba, template) = icon_bytes_for_scheme(scheme);
    let icon = Icon::from_rgba(rgba, TRAY_ICON_SIZE, TRAY_ICON_SIZE)
        .map_err(|error| format!("build tray icon: {error}"))?;

    let tray = TrayIconBuilder::new()
        .with_id("throne")
        .with_menu(Box::new(menu))
        .with_icon(icon)
        .with_icon_as_template(template)
        .with_tooltip("ThroneRs")
        .build()
        .map_err(|error| format!("install system tray: {error}"))?;

    TRAY.with(|cell| {
        *cell.borrow_mut() = Some(TrayHandles {
            tray,
            start_with_system,
            remember_last,
            allow_lan,
            sp_system_proxy,
            sp_tun,
            sp_disabled,
        });
    });
    store_applied(scheme);
    Ok(())
}

/// Refresh checkmarks from app settings (call after settings / spmode changes).
pub fn sync_menu_state(state: TrayMenuState) {
    TRAY.with(|cell| {
        let guard = cell.borrow();
        let Some(h) = guard.as_ref() else {
            return;
        };
        h.start_with_system.set_checked(state.start_with_system);
        h.remember_last.set_checked(state.remember_last);
        h.allow_lan.set_checked(state.allow_lan);
        h.sp_system_proxy.set_checked(state.system_proxy);
        h.sp_tun.set_checked(state.tun);
        h.sp_disabled
            .set_checked(!state.system_proxy && !state.tun);
    });
}

/// Re-tint / swap the tray glyph when UI scheme or OS appearance changes.
///
/// Must be called on the same thread that ran [`install`] (GPUI UI thread).
pub fn apply_scheme(scheme: ColorScheme) {
    if APPLIED_SCHEME.load(Ordering::Relaxed) == scheme_tag(scheme) {
        return;
    }
    let (rgba, template) = icon_bytes_for_scheme(scheme);
    let icon = match Icon::from_rgba(rgba, TRAY_ICON_SIZE, TRAY_ICON_SIZE) {
        Ok(icon) => icon,
        Err(error) => {
            tracing::warn!(%error, "failed to build themed tray icon");
            return;
        }
    };

    TRAY.with(|cell| {
        let guard = cell.borrow();
        let Some(h) = guard.as_ref() else {
            return;
        };
        let result = if cfg!(target_os = "macos") {
            h.tray.set_icon_with_as_template(Some(icon), template)
        } else {
            h.tray.set_icon(Some(icon))
        };
        if let Err(error) = result {
            tracing::warn!(%error, "failed to update tray icon for theme");
            return;
        }
        store_applied(scheme);
    });
}

pub fn next_command() -> Option<TrayCommand> {
    // Drain menu events first (explicit user actions).
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        if let Some(command) = command_from_menu_id(event.id.as_ref()) {
            return Some(command);
        }
    }

    // Left-click / double-click the tray glyph → Show Window (reopen after close).
    // Menu still opens on left click when menu_on_left_click is enabled.
    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
        if tray_event_shows_window(&event) {
            return Some(TrayCommand::ShowWindow);
        }
    }

    None
}

/// Whether a tray icon pointer event should bring the main window forward.
pub fn tray_event_shows_window(event: &TrayIconEvent) -> bool {
    match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } => true,
        TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => true,
        _ => false,
    }
}

/// Upstream Allow LAN: inbound `::` / `0.0.0.0` vs loopback `127.0.0.1`.
pub fn allow_lan_from_address(addr: &str) -> bool {
    matches!(addr.trim(), "::" | "0.0.0.0")
}

pub fn inbound_address_for_allow_lan(allow: bool) -> &'static str {
    if allow {
        "::"
    } else {
        "127.0.0.1"
    }
}

fn store_applied(scheme: ColorScheme) {
    APPLIED_SCHEME.store(scheme_tag(scheme), Ordering::Relaxed);
}

fn scheme_tag(scheme: ColorScheme) -> u8 {
    match scheme {
        ColorScheme::Light => 1,
        ColorScheme::Dark => 2,
    }
}

fn icon_bytes_for_scheme(scheme: ColorScheme) -> (Vec<u8>, bool) {
    if cfg!(target_os = "macos") {
        let _ = scheme;
        (TRAY_ICON_ON_DARK.to_vec(), true)
    } else {
        match scheme {
            ColorScheme::Light => (TRAY_ICON_ON_LIGHT.to_vec(), false),
            ColorScheme::Dark => (TRAY_ICON_ON_DARK.to_vec(), false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_map_to_upstream_tray_actions() {
        assert_eq!(
            command_from_menu_id(SHOW_WINDOW_ID),
            Some(TrayCommand::ShowWindow)
        );
        assert_eq!(
            command_from_menu_id(START_WITH_SYSTEM_ID),
            Some(TrayCommand::ToggleStartWithSystem)
        );
        assert_eq!(
            command_from_menu_id(REMEMBER_LAST_ID),
            Some(TrayCommand::ToggleRememberLast)
        );
        assert_eq!(
            command_from_menu_id(ALLOW_LAN_ID),
            Some(TrayCommand::ToggleAllowLan)
        );
        assert_eq!(
            command_from_menu_id(SELECT_SERVER_ID),
            Some(TrayCommand::SelectServer)
        );
        assert_eq!(
            command_from_menu_id(SELECT_ROUTING_ID),
            Some(TrayCommand::SelectRouting)
        );
        assert_eq!(
            command_from_menu_id(SP_SYSTEM_PROXY_ID),
            Some(TrayCommand::EnableSystemProxy)
        );
        assert_eq!(
            command_from_menu_id(SP_TUN_ID),
            Some(TrayCommand::EnableTun)
        );
        assert_eq!(
            command_from_menu_id(SP_DISABLED_ID),
            Some(TrayCommand::DisableSpMode)
        );
        assert_eq!(
            command_from_menu_id(RESTART_CORE_ID),
            Some(TrayCommand::RestartCore)
        );
        assert_eq!(
            command_from_menu_id(RESTART_PROGRAM_ID),
            Some(TrayCommand::RestartProgram)
        );
        assert_eq!(command_from_menu_id(EXIT_ID), Some(TrayCommand::Exit));
        assert_eq!(command_from_menu_id("unknown"), None);
    }

    #[test]
    fn allow_lan_address_helpers_match_upstream() {
        assert!(allow_lan_from_address("::"));
        assert!(allow_lan_from_address("0.0.0.0"));
        assert!(!allow_lan_from_address("127.0.0.1"));
        assert_eq!(inbound_address_for_allow_lan(true), "::");
        assert_eq!(inbound_address_for_allow_lan(false), "127.0.0.1");
    }

    #[test]
    fn left_click_and_double_click_show_window() {
        use tray_icon::{MouseButton, MouseButtonState, Rect, TrayIconEvent, TrayIconId};
        use tray_icon::dpi::PhysicalPosition;

        let id = TrayIconId::new("throne");
        let rect = Rect {
            position: PhysicalPosition::new(0.0, 0.0),
            size: tray_icon::dpi::PhysicalSize::new(1, 1),
        };
        let pos = PhysicalPosition::new(0.0, 0.0);

        assert!(tray_event_shows_window(&TrayIconEvent::Click {
            id: id.clone(),
            position: pos,
            rect,
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
        }));
        assert!(!tray_event_shows_window(&TrayIconEvent::Click {
            id: id.clone(),
            position: pos,
            rect,
            button: MouseButton::Left,
            button_state: MouseButtonState::Down,
        }));
        assert!(!tray_event_shows_window(&TrayIconEvent::Click {
            id: id.clone(),
            position: pos,
            rect,
            button: MouseButton::Right,
            button_state: MouseButtonState::Up,
        }));
        assert!(tray_event_shows_window(&TrayIconEvent::DoubleClick {
            id: id.clone(),
            position: pos,
            rect,
            button: MouseButton::Left,
        }));
        assert!(!tray_event_shows_window(&TrayIconEvent::Enter {
            id,
            position: pos,
            rect,
        }));
    }

    #[test]
    fn tray_assets_are_64x64_geometric_icon_with_transparent_corners() {
        for bytes in [TRAY_ICON_ON_DARK, TRAY_ICON_ON_LIGHT] {
            assert_eq!(bytes.len(), (TRAY_ICON_SIZE * TRAY_ICON_SIZE * 4) as usize);
            let alpha = |x: u32, y: u32| bytes[((y * TRAY_ICON_SIZE + x) * 4 + 3) as usize];
            // Transparent canvas corners.
            assert_eq!(alpha(0, 0), 0);
            assert_eq!(alpha(63, 0), 0);
            assert_eq!(alpha(0, 63), 0);
            assert_eq!(alpha(63, 63), 0);
            let mut painted = 0u32;
            for y in 0..TRAY_ICON_SIZE {
                for x in 0..TRAY_ICON_SIZE {
                    if alpha(x, y) > 0 {
                        painted += 1;
                    }
                }
            }
            // Upstream-style geometric crown silhouette — filled peaks, not full canvas.
            assert!(
                painted > 800 && painted < 2800,
                "expected geometric crown silhouette pixels, got {painted}"
            );
            // Crown body occupies the middle band.
            let has_body = (20..48).any(|y| (16..48).any(|x| alpha(x, y) > 0));
            assert!(has_body, "expected crown body pixels in center region");
        }
    }
}
