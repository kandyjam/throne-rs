use gpui::{App, ClickEvent, SharedString, Window, div, prelude::*, px};

use crate::theme::Theme;

/// Top toolbar menu button: icon glyph + label under (text-under-icon style).
pub fn toolbar_menu_btn(
    id: impl Into<SharedString>,
    glyph: &'static str,
    label: &'static str,
    open: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_0p5()
        .px_2()
        .py_1()
        .min_w(px(64.))
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
        .child(
            div()
                .text_xs()
                .text_color(Theme::text())
                .child(label),
        )
        .on_click(on_click)
}

/// Large Start / Stop control (upstream StartStopButton).
pub fn start_stop_btn(
    running: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let (label, color, glyph) = if running {
        ("Stop", Theme::stop_red(), "■")
    } else {
        ("Start", Theme::start_green(), "▶")
    };
    div()
        .id("startstop")
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .w(px(72.))
        .h(px(56.))
        .rounded_md()
        .border_1()
        .border_color(color)
        .bg(Theme::bg_elevated())
        .hover(|e| e.bg(Theme::bg_hover()))
        .cursor_pointer()
        .child(
            div()
                .text_xl()
                .text_color(color)
                .child(glyph),
        )
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(color)
                .child(label),
        )
        .on_click(on_click)
}

/// Checkbox row matching Tun / System DNS / System Proxy.
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
        .child(
            div()
                .text_xs()
                .text_color(Theme::text())
                .child(label),
        )
}

pub fn menu_item(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
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
