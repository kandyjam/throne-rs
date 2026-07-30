mod assets;
mod theme;
mod tray;
mod ui;

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    App, Application, Bounds, Entity, TitlebarOptions, WindowBounds, WindowHandle, WindowOptions,
    point, prelude::*, px, size,
};
use tracing_subscriber::EnvFilter;

use throne_domain::{NKR_VERSION, display_name};
use ui::MainWindow;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!(version = NKR_VERSION, "Throne starting");

    Application::new().with_assets(assets::Assets).run(|cx: &mut App| {
        cx.activate(true);

        // Keep the root view alive across window close so tray "Show" / "Toggle"
        // can reopen the same session instead of losing CoreSession state.
        let root = cx.new(MainWindow::new);
        let window_slot: Rc<RefCell<Option<WindowHandle<MainWindow>>>> =
            Rc::new(RefCell::new(Some(open_main_window(cx, root.clone()))));

        match tray::install() {
            Ok(()) => {
                let window_slot = window_slot.clone();
                let root = root.clone();
                cx.spawn(move |cx: &mut gpui::AsyncApp| {
                    let async_cx = cx.clone();
                    async move {
                        loop {
                            smol::Timer::after(std::time::Duration::from_millis(100)).await;
                            while let Some(command) = tray::next_command() {
                                if async_cx
                                    .update(|cx| match command {
                                        tray::TrayCommand::ShowWindow => {
                                            show_main_window(cx, &window_slot, &root);
                                        }
                                        tray::TrayCommand::ToggleProxy => {
                                            root.update(cx, |view, cx| {
                                                view.toggle_proxy(cx);
                                            });
                                        }
                                        tray::TrayCommand::Quit => cx.quit(),
                                    })
                                    .is_err()
                                {
                                    return;
                                }
                            }
                        }
                    }
                })
                .detach();
            }
            Err(error) => tracing::warn!(%error, "desktop tray unavailable"),
        }
    });
}

fn open_main_window(cx: &mut App, root: Entity<MainWindow>) -> WindowHandle<MainWindow> {
    // Upstream mainwindow.ui minimum 800×600
    let bounds = Bounds::centered(None, size(px(960.), px(640.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                // Upstream tray/title includes NKR_VERSION
                title: Some(display_name().into()),
                appears_transparent: false,
                traffic_light_position: Some(point(px(9.), px(9.))),
            }),
            focus: true,
            show: true,
            ..Default::default()
        },
        |_window, _cx| root,
    )
    .expect("open main window")
}

/// `WindowHandle::is_active` is `None` when the window has been closed.
fn window_needs_reopen(is_active: Option<bool>) -> bool {
    is_active.is_none()
}

/// Bring the main window forward, recreating it if the user closed it to the tray.
fn show_main_window(
    cx: &mut App,
    window_slot: &Rc<RefCell<Option<WindowHandle<MainWindow>>>>,
    root: &Entity<MainWindow>,
) {
    cx.activate(true);

    let is_active = window_slot
        .borrow()
        .as_ref()
        .and_then(|handle| handle.is_active(cx));

    if window_needs_reopen(is_active) {
        *window_slot.borrow_mut() = Some(open_main_window(cx, root.clone()));
        return;
    }

    if let Some(handle) = window_slot.borrow().as_ref() {
        let _ = handle.update(cx, |_, window, _| {
            window.activate_window();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::tray::{TrayCommand, command_from_menu_id};

    #[test]
    fn original_group_actions_have_original_shortcuts_and_confirmation() {
        let source = include_str!("ui/main_window.rs");

        assert!(source.contains("cmd-shift-g"));
        assert!(source.contains("ctrl-shift-g"));
        assert!(source.contains("cmd-shift-r"));
        assert!(source.contains("ctrl-shift-r"));
        assert_eq!(source.matches("cmd-shift-r").count(), 1);
        assert_eq!(source.matches("ctrl-shift-r").count(), 1);
        assert!(source.contains("ConfirmDeleteUnavailable"));
    }

    #[test]
    fn main_window_omits_non_original_dns_and_runtime_summary_regions() {
        let source = include_str!("ui/main_window.rs");

        assert!(source.contains("mode_checkbox(\"tun\", \"Tun Mode\""));
        assert!(source.contains("mode_checkbox(\"proxy\", \"System Proxy\""));
        assert!(!source.contains("mode_checkbox(\"dns\", \"System DNS\""));
        assert!(!source.contains("fn render_data_view(&self)"));
        assert!(!source.contains(".child(self.render_data_view())"));
    }

    #[test]
    fn tray_menu_ids_map_to_their_application_actions() {
        assert_eq!(command_from_menu_id("throne.show"), Some(TrayCommand::ShowWindow));
        assert_eq!(command_from_menu_id("throne.toggle"), Some(TrayCommand::ToggleProxy));
        assert_eq!(command_from_menu_id("throne.quit"), Some(TrayCommand::Quit));
        assert_eq!(command_from_menu_id("unrelated"), None);
    }

    #[test]
    fn closed_window_handle_is_treated_as_needing_reopen() {
        use super::window_needs_reopen;

        // `WindowHandle::is_active` returns None when the platform window is gone.
        assert!(window_needs_reopen(None));
        // Still open (focused or not) → just activate, do not recreate.
        assert!(!window_needs_reopen(Some(true)));
        assert!(!window_needs_reopen(Some(false)));
    }
}
