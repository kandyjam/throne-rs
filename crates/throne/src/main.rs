mod assets;
mod theme;
mod tray;
mod ui;

use gpui::{
    App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, prelude::*, px,
    size,
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

        // Upstream mainwindow.ui minimum 800×600
        let bounds = Bounds::centered(None, size(px(960.), px(640.)), cx);
        let window_handle = cx
            .open_window(
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
            |_window, cx| cx.new(MainWindow::new),
            )
            .expect("open main window");

        match tray::install() {
            Ok(()) => {
                cx.spawn(move |cx: &mut gpui::AsyncApp| {
                    let async_cx = cx.clone();
                    async move {
                        loop {
                            smol::Timer::after(std::time::Duration::from_millis(100)).await;
                            while let Some(command) = tray::next_command() {
                                if async_cx
                                    .update(|cx| match command {
                                        tray::TrayCommand::ShowWindow => {
                                            cx.activate(true);
                                            let _ = window_handle.update(cx, |_, window, _| {
                                                window.activate_window();
                                            });
                                        }
                                        tray::TrayCommand::ToggleProxy => {
                                            let _ = window_handle.update(cx, |view, _, cx| {
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
}
