use gpui::{App, ClickEvent, SharedString, Window, div, prelude::*, px};

use crate::theme::Theme;

/// Toolbar button width (keep in sync with main_window menu overlay offsets).
pub const TOOLBAR_BTN_W: f32 = 68.;
/// Gap between toolbar menu buttons.
pub const TOOLBAR_BTN_GAP: f32 = 4.;
/// Top padding of the top bar + button height — menu overlay top edge.
pub const TOOLBAR_MENU_TOP: f32 = 66.;
/// Left padding of the top bar.
pub const TOOLBAR_PAD_X: f32 = 8.;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartStopState {
    Start,
    Stop,
    Starting,
    Stopping,
}

pub fn start_stop_presentation(state: StartStopState) -> (&'static str, bool) {
    match state {
        StartStopState::Start => ("Start", false),
        StartStopState::Stop => ("Stop", false),
        StartStopState::Starting => ("Starting…", true),
        StartStopState::Stopping => ("Stopping…", true),
    }
}

#[cfg(test)]
mod tests {
    use super::{StartStopState, start_stop_presentation};

    #[test]
    fn transitional_start_stop_states_show_loading_feedback() {
        assert_eq!(
            start_stop_presentation(StartStopState::Starting),
            ("Starting…", true)
        );
        assert_eq!(
            start_stop_presentation(StartStopState::Stopping),
            ("Stopping…", true)
        );
    }

    #[test]
    fn stable_start_stop_states_do_not_show_loading_feedback() {
        assert_eq!(
            start_stop_presentation(StartStopState::Start),
            ("Start", false)
        );
        assert_eq!(
            start_stop_presentation(StartStopState::Stop),
            ("Stop", false)
        );
    }
}

/// Toolbar button. Dropdown content is rendered as a **root-level overlay**
/// (see `MainWindow::render_toolbar_menu_overlay`) so it is not clipped or
/// painted under the profile table / group tabs.
pub fn toolbar_btn(
    id: impl Into<SharedString>,
    glyph: &'static str,
    label: &'static str,
    open: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id: SharedString = id.into();
    div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_0p5()
        .w(px(TOOLBAR_BTN_W))
        .h(px(56.))
        .rounded_sm()
        .border_1()
        .border_color(if open {
            Theme::accent()
        } else {
            Theme::border_light()
        })
        .bg(if open {
            Theme::accent_soft()
        } else {
            Theme::bg_toolbar_btn()
        })
        .hover(|e| e.bg(Theme::bg_hover()))
        .cursor_pointer()
        .child(
            div()
                .text_lg()
                .line_height(px(22.))
                .text_color(Theme::text())
                .child(glyph),
        )
        .child(div().text_xs().text_color(Theme::text()).child(label))
        .on_click(on_toggle)
}

/// Floating dropdown panel for toolbar menus (absolute, caller sets top/left).
pub fn toolbar_menu_panel(
    id: impl Into<SharedString>,
    top: f32,
    left: f32,
    menu: impl IntoElement,
) -> impl IntoElement {
    div()
        .id(id.into())
        .absolute()
        .top(px(top))
        .left(px(left))
        .min_w(px(230.))
        .max_h(px(360.))
        .overflow_y_scroll()
        .py_1()
        .bg(Theme::bg_elevated())
        .border_1()
        .border_color(Theme::border_light())
        .rounded_sm()
        .shadow_lg()
        // Capture clicks so they don't fall through to the table.
        .occlude()
        .child(menu)
}

pub fn start_stop_btn(
    state: StartStopState,
    loading_glyph: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let (label, loading) = start_stop_presentation(state);
    let color = match state {
        StartStopState::Stop | StartStopState::Stopping => Theme::stop_red(),
        StartStopState::Start | StartStopState::Starting => Theme::start_green(),
    };
    let glyph = if loading {
        loading_glyph
    } else if matches!(state, StartStopState::Stop) {
        "■"
    } else {
        "▶"
    };
    div()
        .id("startstop")
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .w(px(64.))
        .h(px(56.))
        .rounded_md()
        .border_1()
        .border_color(color)
        .bg(Theme::bg_elevated())
        .hover(|e| e.bg(Theme::bg_hover()))
        .cursor_pointer()
        .child(div().text_xl().text_color(color).child(glyph))
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(color)
                .child(label),
        )
        .on_click(on_click)
}

pub fn mode_checkbox(
    id: impl Into<SharedString>,
    label: &'static str,
    checked: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .flex()
        .items_center()
        .gap_1p5()
        .h(px(18.))
        .cursor_pointer()
        .on_click(on_click)
        .child(
            div()
                .size(px(14.))
                .rounded_sm()
                .border_1()
                .border_color(Theme::border())
                .bg(if checked {
                    Theme::accent()
                } else {
                    Theme::bg_elevated()
                })
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(Theme::text_on_selected())
                .child(if checked { "✓" } else { "" }),
        )
        .child(div().text_xs().text_color(Theme::text()).child(label))
}

pub fn menu_item(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .w_full()
        .px_3()
        .py_1p5()
        .text_sm()
        .text_color(Theme::text())
        .cursor_pointer()
        .hover(|e| e.bg(Theme::accent()).text_color(Theme::text_on_selected()))
        .child(label.into())
        .on_click(on_click)
}

pub fn menu_separator() -> impl IntoElement {
    div()
        .h(px(1.))
        .w_full()
        .my_1()
        .bg(Theme::border_light())
}

pub fn menu_label(text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .px_3()
        .py_1()
        .text_xs()
        .text_color(Theme::text_muted())
        .child(text.into())
}

/// Modal dialog chrome (secondary features).
///
/// GPUI hit-testing is multi-hit by default: without [`.occlude()`], a click on
/// the panel still marks the backdrop as hovered and fires its close handler.
/// That is why "click field to type" used to dismiss the dialog immediately.
pub fn modal_shell(
    title: impl Into<SharedString>,
    body: impl IntoElement,
    on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let on_close = std::rc::Rc::new(on_close);
    let on_close_bg = on_close.clone();
    let on_close_x = on_close;
    div()
        .id("modal-root")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        // Block interaction with the main window under the modal.
        .occlude()
        .child(
            // dim backdrop — click outside the panel closes
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(gpui::rgba(0x00000080))
                .id("modal-backdrop")
                .on_click(move |ev, w, cx| on_close_bg(ev, w, cx)),
        )
        .child(
            div()
                .id("modal-panel")
                .relative()
                .w(px(520.))
                .max_h(px(480.))
                .flex()
                .flex_col()
                .bg(Theme::bg_elevated())
                .border_1()
                .border_color(Theme::border())
                .rounded_md()
                .shadow_lg()
                // Critical: absorb hits so the backdrop under the panel does not close us.
                .occlude()
                // Swallow clicks on empty panel chrome (title bar padding, gaps).
                .on_click(|_, _, cx| cx.stop_propagation())
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_4()
                        .py_2()
                        .border_b_1()
                        .border_color(Theme::border_light())
                        .bg(Theme::bg_panel())
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(Theme::text())
                                .child(title.into()),
                        )
                        .child(
                            div()
                                .id("modal-x")
                                .px_2()
                                .cursor_pointer()
                                .text_color(Theme::text_muted())
                                .child("✕")
                                .on_click(move |ev, w, cx| {
                                    on_close_x(ev, w, cx);
                                }),
                        ),
                )
                .child(
                    div()
                        .id("modal-body")
                        .flex_1()
                        .overflow_y_scroll()
                        .p_4()
                        .child(body),
                ),
        )
}

pub fn form_row(label: &'static str, value: impl Into<SharedString>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_3()
        .mb_2()
        .child(
            div()
                .w(px(160.))
                .text_xs()
                .text_color(Theme::text_muted())
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .px_2()
                .py_1()
                .rounded_sm()
                .border_1()
                .border_color(Theme::border_light())
                .bg(Theme::bg_app())
                .text_sm()
                .text_color(Theme::text())
                .child(value.into()),
        )
}

pub fn primary_btn(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .px_3()
        .py_1p5()
        .rounded_sm()
        .bg(Theme::accent())
        .text_sm()
        .text_color(Theme::text_on_selected())
        .cursor_pointer()
        .hover(|e| e.opacity(0.9))
        .child(label.into())
        .on_click(on_click)
}

pub fn secondary_btn(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .px_3()
        .py_1p5()
        .rounded_sm()
        .border_1()
        .border_color(Theme::border_light())
        .bg(Theme::bg_toolbar_btn())
        .text_sm()
        .text_color(Theme::text())
        .cursor_pointer()
        .hover(|e| e.bg(Theme::bg_hover()))
        .child(label.into())
        .on_click(on_click)
}
