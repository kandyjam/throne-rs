use gpui::{App, SharedString, div, prelude::*, px, relative};

use crate::theme::Theme;

pub fn toolbar_button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    primary: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut App) + 'static,
) -> impl IntoElement {
    let label = label.into();
    let id = id.into();
    div()
        .id(id)
        .px_3()
        .py_1p5()
        .rounded_md()
        .text_sm()
        .cursor_pointer()
        .when(primary, |el| {
            el.bg(Theme::accent())
                .text_color(gpui::white())
                .hover(|e| e.bg(Theme::accent_soft()))
        })
        .when(!primary, |el| {
            el.bg(Theme::bg_elevated())
                .text_color(Theme::text())
                .border_1()
                .border_color(Theme::border())
                .hover(|e| e.bg(Theme::bg_hover()))
        })
        .child(label)
        .on_click(on_click)
}

pub fn pill(label: impl Into<SharedString>, color: gpui::Hsla) -> impl IntoElement {
    div()
        .px_2()
        .py_0p5()
        .rounded_full()
        .bg(Theme::bg_elevated())
        .border_1()
        .border_color(Theme::border())
        .text_xs()
        .text_color(color)
        .child(label.into())
}

pub fn section_label(text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(Theme::text_muted())
        .child(text.into())
}

pub fn spacer() -> impl IntoElement {
    div().flex_1()
}

pub fn h_rule() -> impl IntoElement {
    div()
        .w_full()
        .h(px(1.))
        .bg(Theme::border())
        .my_1()
}

pub fn status_dot(active: bool) -> impl IntoElement {
    div()
        .size(px(8.))
        .rounded_full()
        .bg(if active {
            Theme::success()
        } else {
            Theme::text_muted()
        })
}

/// Simple text field styling for the search box (click-to-focus later).
pub fn search_field(
    id: impl Into<SharedString>,
    value: &str,
    placeholder: &str,
    on_input: impl Fn(&str, &mut gpui::Window, &mut App) + 'static,
) -> impl IntoElement {
    let shown = if value.is_empty() {
        placeholder.to_string()
    } else {
        value.to_string()
    };
    let muted = value.is_empty();
    // Character-cycle demo filter keys: type letters via click chips for MVP;
    // full IME input lands with a proper Input element later.
    let _ = on_input;
    div()
        .id(id.into())
        .flex()
        .items_center()
        .px_3()
        .h(px(32.))
        .min_w(px(180.))
        .max_w(relative(0.35))
        .flex_1()
        .rounded_md()
        .bg(Theme::bg_elevated())
        .border_1()
        .border_color(Theme::border())
        .text_sm()
        .text_color(if muted {
            Theme::text_muted()
        } else {
            Theme::text()
        })
        .child(shown)
}
