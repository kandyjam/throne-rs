mod theme;
mod ui;

use gpui::{
    App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, prelude::*, px,
    size,
};
use tracing_subscriber::EnvFilter;

use ui::MainWindow;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    Application::new().run(|cx: &mut App| {
        cx.activate(true);

        // Upstream mainwindow.ui minimum 800×600
        let bounds = Bounds::centered(None, size(px(960.), px(640.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Throne".into()),
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
    });
}
