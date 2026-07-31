mod assets;
mod dock_icon;
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

        let tray_state = root.read(cx).tray_menu_state();
        match tray::install(tray_state) {
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
                                    .update(|cx| {
                                        dispatch_tray_command(command, cx, &window_slot, &root)
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
        |window, cx| {
            // Follow OS light/dark and re-paint when the system appearance changes.
            root.update(cx, |view, cx| {
                view.attach_window_appearance(window, cx);
            });
            root
        },
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

/// Upstream tray action routing (labels/order match `MainWindow` Setup Tray).
fn dispatch_tray_command(
    command: tray::TrayCommand,
    cx: &mut App,
    window_slot: &Rc<RefCell<Option<WindowHandle<MainWindow>>>>,
    root: &Entity<MainWindow>,
) {
    match command {
        tray::TrayCommand::ShowWindow | tray::TrayCommand::SelectServer => {
            show_main_window(cx, window_slot, root);
            if matches!(command, tray::TrayCommand::SelectServer) {
                root.update(cx, |view, cx| {
                    view.tray_select_server(cx);
                });
            }
        }
        tray::TrayCommand::SelectRouting => {
            show_main_window(cx, window_slot, root);
            root.update(cx, |view, cx| {
                view.tray_select_routing(cx);
            });
        }
        tray::TrayCommand::ToggleStartWithSystem => {
            root.update(cx, |view, cx| view.tray_toggle_start_with_system(cx));
        }
        tray::TrayCommand::ToggleRememberLast => {
            root.update(cx, |view, cx| view.tray_toggle_remember_last(cx));
        }
        tray::TrayCommand::ToggleAllowLan => {
            root.update(cx, |view, cx| view.tray_toggle_allow_lan(cx));
        }
        tray::TrayCommand::EnableSystemProxy => {
            root.update(cx, |view, cx| view.tray_set_system_proxy(true, cx));
        }
        tray::TrayCommand::EnableTun => {
            root.update(cx, |view, cx| view.tray_set_tun(true, cx));
        }
        tray::TrayCommand::DisableSpMode => {
            root.update(cx, |view, cx| view.tray_disable_spmode(cx));
        }
        tray::TrayCommand::RestartCore => {
            root.update(cx, |view, cx| view.tray_restart_core(cx));
        }
        tray::TrayCommand::RestartProgram => {
            restart_program(cx);
        }
        tray::TrayCommand::Exit => cx.quit(),
    }
}

/// Upstream `actionRestart_Program` — re-exec current binary then quit.
fn restart_program(cx: &mut App) {
    match std::env::current_exe() {
        Ok(exe) => {
            let mut cmd = std::process::Command::new(&exe);
            cmd.args(std::env::args_os().skip(1));
            if let Err(error) = cmd.spawn() {
                tracing::error!(%error, "failed to restart program");
            } else {
                cx.quit();
            }
        }
        Err(error) => tracing::error!(%error, "current_exe failed; cannot restart"),
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
        assert!(source.contains("ConfirmUpdateAllSubscriptions"));
        assert!(source.contains("SubscriptionDiff"));
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
        assert_eq!(
            command_from_menu_id("throne.show"),
            Some(TrayCommand::ShowWindow)
        );
        assert_eq!(
            command_from_menu_id("throne.exit"),
            Some(TrayCommand::Exit)
        );
        assert_eq!(
            command_from_menu_id("throne.select_server"),
            Some(TrayCommand::SelectServer)
        );
        assert_eq!(
            command_from_menu_id("throne.sp.system_proxy"),
            Some(TrayCommand::EnableSystemProxy)
        );
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
