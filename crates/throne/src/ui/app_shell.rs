//! Window content shell: hosts [`MainWindow`] and paints gpui-component Dialog layers.
//!
//! Dialog builders must **not** run while `MainWindow` is mid-render (they call
//! `entity.read`). Keeping the dialog layer as a sibling of `MainWindow` under this
//! shell avoids the "cannot read while already being updated" panic.

use gpui::{Context, Entity, IntoElement, Render, Window, div, prelude::*};
use gpui_component::Root;

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
        // Flush deferred dialog work while MainWindow is not mid-render, then
        // paint the dialog layer as a sibling so builders can `main.read(cx)`.
        self.main.update(cx, |main, cx| {
            main.prepare_dialog_layer(window, cx);
        });

        div()
            .size_full()
            .relative()
            .child(self.main.clone())
            .children(Root::render_dialog_layer(window, cx))
    }
}
