//! Window content shell: hosts [`MainWindow`] and flushes dialog state outside its render.
//!
//! Dialog builders must **not** run while `MainWindow` is mid-render (they call
//! `entity.read`). gpui-component 0.7 paints the dialog layer from the Root plugin.

use gpui::{div, prelude::*, Context, Entity, IntoElement, Render, Window};

use crate::ui::main_window::MainWindow;

pub struct AppShell {
    main: Entity<MainWindow>,
    _main_obs: gpui::Subscription,
}

impl AppShell {
    pub fn new(main: Entity<MainWindow>, cx: &mut Context<Self>) -> Self {
        // Re-paint when MainWindow state changes so open_dialog builders refresh.
        let _main_obs = cx.observe(&main, |_, _, cx| cx.notify());
        Self { main, _main_obs }
    }
}

impl Render for AppShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Flush deferred dialog work while MainWindow is not mid-render.
        self.main.update(cx, |main, cx| {
            main.prepare_dialog_layer(window, cx);
        });

        div().size_full().relative().child(self.main.clone())
    }
}
