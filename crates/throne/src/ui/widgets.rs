//! Shared UI primitives backed by [gpui-component](https://longbridge.github.io/gpui-component).
//!
//! Keep thin wrappers so call sites stay stable while chrome comes from the component library.

use std::rc::Rc;

use gpui::{
    App, ClickEvent, Entity, KeyDownEvent, SharedString, Window, div, prelude::*, px,
};
use gpui_component::{
    Icon, Sizable as _, Size, h_flex,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
    switch::Switch,
    tab::{Tab, TabBar},
};

use crate::theme::Theme;

/// Toolbar button width (keep in sync with main_window menu overlay offsets).
pub const TOOLBAR_BTN_W: f32 = 68.;
/// Gap between toolbar menu buttons.
pub const TOOLBAR_BTN_GAP: f32 = 4.;
/// Top bar: `py_2`(8) + button(52) — dropdown sits flush under the buttons.
pub const TOOLBAR_MENU_TOP: f32 = 60.;
/// Left padding of the top bar (`px_3`).
pub const TOOLBAR_PAD_X: f32 = 12.;
/// Compact menu row height (tighter than gpui-component PopupMenu's 26px default).
pub const MENU_ITEM_H: f32 = 22.;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolbarIcon {
    Program,
    Settings,
    Groups,
    Routing,
    Tools,
}

pub fn toolbar_icon_path(icon: ToolbarIcon) -> &'static str {
    match icon {
        ToolbarIcon::Program => "icons/box.svg",
        ToolbarIcon::Settings => "icons/settings.svg",
        ToolbarIcon::Groups => "icons/layers.svg",
        ToolbarIcon::Routing => "icons/route.svg",
        ToolbarIcon::Tools => "icons/wrench.svg",
    }
}

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
    use super::{StartStopState, ToolbarIcon, start_stop_presentation, toolbar_icon_path};

    #[test]
    fn toolbar_actions_use_distinct_embedded_icon_assets() {
        assert_eq!(toolbar_icon_path(ToolbarIcon::Program), "icons/box.svg");
        assert_eq!(
            toolbar_icon_path(ToolbarIcon::Settings),
            "icons/settings.svg"
        );
        assert_eq!(toolbar_icon_path(ToolbarIcon::Groups), "icons/layers.svg");
        assert_eq!(toolbar_icon_path(ToolbarIcon::Routing), "icons/route.svg");
        assert_eq!(toolbar_icon_path(ToolbarIcon::Tools), "icons/wrench.svg");
    }

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

fn icon_from_path(path: impl Into<SharedString>) -> Icon {
    Icon::default().path(path)
}

/// Toolbar button. Dropdown content is a compact `menu_panel` rendered as a
/// root-level overlay (`MainWindow::render_toolbar_menu_overlay`) so it sits
/// above the profile table / group tabs.
///
/// Layout stays vertical (icon over label) to match upstream Throne chrome;
/// icon glyph comes from gpui-component [`Icon`].
pub fn toolbar_btn(
    id: impl Into<SharedString>,
    icon: ToolbarIcon,
    label: &'static str,
    open: bool,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id: SharedString = id.into();
    let icon_color = if open {
        Theme::accent()
    } else {
        Theme::icon()
    };
    div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_0p5()
        .w(px(TOOLBAR_BTN_W))
        .h(px(52.))
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
            icon_from_path(toolbar_icon_path(icon))
                .with_size(Size::Small)
                .text_color(icon_color),
        )
        .child(div().text_xs().text_color(Theme::text()).child(label))
        .on_click(on_toggle)
}

/// Square Start / Stop control — compact icon-only gpui-component [`Button`].
///
/// Smaller than the 52px toolbar buttons so it sits lighter in the top bar.
pub fn start_stop_btn(
    state: StartStopState,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let (label, loading) = start_stop_presentation(state);
    let is_stop = matches!(state, StartStopState::Stop | StartStopState::Stopping);
    let icon_path = if matches!(state, StartStopState::Stop | StartStopState::Stopping) {
        "icons/square.svg"
    } else {
        "icons/play.svg"
    };

    // No `.label(...)` → square icon button; tooltip carries Start / Stop.
    let btn = Button::new("startstop")
        .icon(icon_from_path(icon_path).with_size(Size::Small))
        .loading(loading)
        .tooltip(label)
        .with_size(Size::Size(px(40.)))
        .on_click(on_click);

    if is_stop {
        btn.danger().outline()
    } else {
        btn.success().outline()
    }
}

/// Mode toggle (Tun / System Proxy / settings flags) — gpui-component [`Switch`].
pub fn mode_switch(
    id: impl Into<SharedString>,
    label: &'static str,
    checked: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    Switch::new(id.into())
        .label(label)
        .checked(checked)
        .with_size(Size::XSmall)
        .on_click(move |_checked, window, cx| {
            on_click(&ClickEvent::default(), window, cx);
        })
}

/// Full-width settings toggle: switch + wrapping label that tracks dialog width.
///
/// Prefer this over [`mode_switch`] for long Basic Settings labels that would
/// otherwise clip inside the Switch's single-line label slot.
///
/// The whole row is the hit target (Switch is display-only) so long labels stay
/// clickable without double-firing.
pub fn settings_switch_row(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    checked: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id: SharedString = id.into();
    let label = label.into();
    h_flex()
        .id(id.clone())
        .w_full()
        .items_start()
        .gap_2()
        .mb_1()
        .cursor_pointer()
        .on_click(move |ev, w, cx| on_click(ev, w, cx))
        .child(Switch::new(id).checked(checked).with_size(Size::XSmall))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_sm()
                .text_color(Theme::text())
                .child(label),
        )
}

/// Left column width used by upstream Basic Settings form grids (~label column).
pub const FORM_LABEL_W: f32 = 210.;

/// Two-column form row: left label, right control (Qt `QGridLayout` / form style).
pub fn form_row(
    label: impl Into<SharedString>,
    control: impl IntoElement,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .items_center()
        .gap_3()
        .mb_2()
        .child(
            div()
                .w(px(FORM_LABEL_W))
                .flex_shrink_0()
                .text_sm()
                .text_color(Theme::text())
                .child(label.into()),
        )
        .child(div().flex_1().min_w(px(0.)).child(control))
}

/// Form row bound to a real [`Input`] (label | input).
pub fn form_input_row(
    label: impl Into<SharedString>,
    state: &Entity<InputState>,
) -> impl IntoElement {
    form_row(label, Input::new(state).cleanable(true))
}

/// Upstream auto-update row: `Label | [Enable] Interval … [minutes]`.
pub fn form_enable_interval_row(
    row_id: impl Into<SharedString>,
    title: impl Into<SharedString>,
    enabled: bool,
    minutes: &Entity<InputState>,
    on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let row_id: SharedString = row_id.into();
    let switch_id = SharedString::from(format!("{row_id}-en"));
    form_row(
        title,
        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(
                h_flex()
                    .id(switch_id.clone())
                    .items_center()
                    .gap_1()
                    .flex_shrink_0()
                    .cursor_pointer()
                    .on_click(move |ev, w, cx| on_toggle(ev, w, cx))
                    .child(Switch::new(switch_id).checked(enabled).with_size(Size::XSmall))
                    .child(
                        div()
                            .text_sm()
                            .text_color(Theme::text())
                            .child("Enable"),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .flex_shrink_0()
                    .child("Interval (minute, invalid if less than 30)"),
            )
            .child(
                div()
                    .w(px(88.))
                    .flex_shrink_0()
                    .child(Input::new(minutes).cleanable(true)),
            ),
    )
}

/// Muted helper text above dialog forms.
pub fn section_hint(text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .w_full()
        .text_xs()
        .text_color(Theme::text_muted())
        .mb_1()
        .child(text.into())
}

/// Titled group panel — mirrors upstream `QGroupBox` (Routes / Route Profile).
pub fn group_panel(
    title: impl Into<SharedString>,
    body: impl IntoElement,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .mb_2()
        .border_1()
        .border_color(Theme::border_light())
        .rounded_md()
        .bg(Theme::bg_elevated())
        .child(
            div()
                .px_3()
                .py_1p5()
                .border_b_1()
                .border_color(Theme::border_light())
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(Theme::text())
                .child(title.into()),
        )
        .child(div().p_3().child(body))
}

/// Fallback line-edit chrome (routing fields without NestedInputs).
///
/// Prefer real [`Input`] / [`input_field_row`] for new forms.
pub fn focus_field(
    id: impl Into<SharedString>,
    value: impl Into<SharedString>,
    focused: bool,
    placeholder: Option<&str>,
    on_focus: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id: SharedString = id.into();
    let value = value.into();
    let ph = placeholder.unwrap_or("").to_string();
    let empty = value.is_empty();
    let showing_placeholder = !focused && empty && !ph.is_empty();
    let display: SharedString = if focused {
        if empty {
            "▌".into()
        } else {
            format!("{value}▌").into()
        }
    } else if empty {
        ph.into()
    } else {
        value
    };
    let on_focus = std::rc::Rc::new(on_focus);
    let on_focus_md = on_focus.clone();
    let on_focus_click = on_focus;

    div()
        .id(id)
        .flex_1()
        .min_w(px(120.))
        .h(px(30.))
        .px_2()
        .flex()
        .items_center()
        .rounded_sm()
        .border_1()
        .border_color(if focused {
            Theme::accent()
        } else {
            Theme::border_light()
        })
        .bg(Theme::bg_elevated())
        .text_sm()
        .text_color(if showing_placeholder {
            Theme::text_muted()
        } else {
            Theme::text()
        })
        .cursor_text()
        .on_mouse_down(gpui::MouseButton::Left, move |_, w, cx| {
            cx.stop_propagation();
            on_focus_md(w, cx);
        })
        .on_click(move |_, w, cx| {
            cx.stop_propagation();
            on_focus_click(w, cx);
        })
        .child(display)
}

/// Label + real gpui-component [`Input`] row.
pub fn input_field_row(
    label: &'static str,
    state: &Entity<InputState>,
    label_width: f32,
) -> impl IntoElement {
    h_flex()
        .items_start()
        .gap_3()
        .mb_2()
        .w_full()
        .child(
            div()
                .w(px(label_width))
                .flex_shrink_0()
                .pt(px(6.))
                .text_xs()
                .text_color(Theme::text_muted())
                .child(label),
        )
        .child(div().flex_1().min_w(px(0.)).child(Input::new(state).cleanable(true)))
}

/// Format a GPUI keystroke as a Throne hotkey label (`Cmd/Ctrl+Shift+C`).
///
/// Returns `None` for pure modifier presses (user is still holding the chord).
pub fn format_hotkey_chord(keystroke: &gpui::Keystroke) -> Option<String> {
    let key = keystroke.key.as_str();
    // Modifier-only keydowns — wait for a real key.
    if matches!(
        key,
        "control" | "ctrl" | "shift" | "alt" | "option" | "meta" | "cmd" | "command" | "win"
            | "windows" | "super" | "fn" | "function"
    ) {
        return None;
    }
    // Escape is reserved for clear / dialog dismiss — not a bindable chord here.
    if key == "escape" {
        return None;
    }

    let mut parts: Vec<String> = Vec::new();
    // Match stored defaults: Cmd/Ctrl for either platform (⌘) or control.
    if keystroke.modifiers.platform || keystroke.modifiers.control {
        parts.push("Cmd/Ctrl".into());
    }
    if keystroke.modifiers.alt {
        parts.push("Alt".into());
    }
    if keystroke.modifiers.shift {
        parts.push("Shift".into());
    }

    let key_label = match key {
        "enter" | "return" => "Enter".to_string(),
        "space" => "Space".to_string(),
        "tab" => "Tab".to_string(),
        "backspace" => "Backspace".to_string(),
        "delete" => "Delete".to_string(),
        "up" => "Up".to_string(),
        "down" => "Down".to_string(),
        "left" => "Left".to_string(),
        "right" => "Right".to_string(),
        "pageup" => "PageUp".to_string(),
        "pagedown" => "PageDown".to_string(),
        "home" => "Home".to_string(),
        "end" => "End".to_string(),
        "insert" => "Insert".to_string(),
        other => {
            // Single letters / digits → uppercase; f-keys keep common casing.
            if other.len() == 1 {
                other.to_uppercase()
            } else if let Some(rest) = other.strip_prefix('f').filter(|r| r.chars().all(|c| c.is_ascii_digit())) {
                format!("F{rest}")
            } else {
                let mut chars = other.chars();
                match chars.next() {
                    Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
                    None => return None,
                }
            }
        }
    };
    parts.push(key_label);
    Some(parts.join("+"))
}

/// Label + key-capture field (upstream `QKeySequenceEdit` / `QtExtKeySequenceEdit`).
///
/// Click the field, then press a chord — `on_set` receives a label like
/// `Cmd/Ctrl+R`. Backspace/Delete/Escape clear (empty string).
pub fn hotkey_capture_row(
    id: impl Into<SharedString>,
    label: &'static str,
    value: &str,
    label_width: f32,
    on_set: impl Fn(String, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id: SharedString = id.into();
    let clear_id = SharedString::from(format!("{id}-clear"));
    let empty = value.trim().is_empty();
    let display: SharedString = if empty {
        "Press shortcut…".into()
    } else {
        value.to_string().into()
    };

    let on_set = Rc::new(on_set);
    let on_key = on_set.clone();
    let on_clear = on_set;

    h_flex()
        .items_center()
        .gap_3()
        .mb_2()
        .w_full()
        .child(
            div()
                .w(px(label_width))
                .text_xs()
                .text_color(Theme::text_muted())
                .child(label),
        )
        .child(
            div()
                .id(id)
                .flex_1()
                .min_w(px(120.))
                .flex()
                .items_center()
                .h(px(32.))
                .px_3()
                .rounded_md()
                .border_1()
                .border_color(Theme::border_light())
                .bg(Theme::bg_elevated())
                .cursor_text()
                .tab_index(0)
                .focus(|s| s.border_color(Theme::accent()))
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .text_color(if empty {
                            Theme::text_muted()
                        } else {
                            Theme::text()
                        })
                        .child(display),
                )
                .child(
                    div()
                        .id(clear_id)
                        .px_1()
                        .text_xs()
                        .text_color(Theme::text_muted())
                        .cursor_pointer()
                        .hover(|s| s.text_color(Theme::text()))
                        .child("✕")
                        .on_click(move |_, window, cx| {
                            on_clear(String::new(), window, cx);
                        }),
                )
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    let key = event.keystroke.key.as_str();
                    // Clear without binding Escape / lone Backspace / Delete.
                    if key == "escape"
                        || ((key == "backspace" || key == "delete")
                            && !event.keystroke.modifiers.modified())
                    {
                        on_key(String::new(), window, cx);
                        cx.stop_propagation();
                        return;
                    }
                    if let Some(chord) = format_hotkey_chord(&event.keystroke) {
                        on_key(chord, window, cx);
                        cx.stop_propagation();
                    }
                }),
        )
}

#[cfg(test)]
mod hotkey_format_tests {
    use super::format_hotkey_chord;
    use gpui::{Keystroke, Modifiers};

    fn ks(key: &str, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.into(),
            key_char: None,
        }
    }

    #[test]
    fn formats_cmd_ctrl_letter() {
        let mut m = Modifiers::default();
        m.platform = true;
        assert_eq!(
            format_hotkey_chord(&ks("r", m)).as_deref(),
            Some("Cmd/Ctrl+R")
        );
    }

    #[test]
    fn formats_shift_combo() {
        let mut m = Modifiers::default();
        m.control = true;
        m.shift = true;
        assert_eq!(
            format_hotkey_chord(&ks("c", m)).as_deref(),
            Some("Cmd/Ctrl+Shift+C")
        );
    }

    #[test]
    fn ignores_modifier_only() {
        let mut m = Modifiers::default();
        m.shift = true;
        assert_eq!(format_hotkey_chord(&ks("shift", m)), None);
    }
}

/// Full-width real multi-line [`Input`].
pub fn input_area(state: &Entity<InputState>) -> impl IntoElement {
    div().w_full().child(Input::new(state).cleanable(true))
}

/// Multi-line [`Input`] with a **definite** pixel height (route simple-rules grids).
///
/// gpui-component multi-line Inputs collapse to ~one line under `h_auto` unless
/// given an explicit `.h(...)`. Percentage/`h_full` only works when every ancestor
/// already has a definite height — so we pin the height in pixels here.
pub fn input_area_tall(state: &Entity<InputState>, height: f32) -> impl IntoElement {
    div()
        .w_full()
        .h(px(height))
        .child(Input::new(state).cleanable(true).h(px(height)))
}

/// Compact Input (flex-1) for inline rows; optional trailing control (e.g. preset ▼).
pub fn input_inline(state: &Entity<InputState>) -> impl IntoElement {
    div()
        .flex_1()
        .min_w(px(120.))
        .child(Input::new(state).cleanable(true).with_size(Size::Small))
}

/// Multiline focus box (Add from input / multi-line routing fields).
pub fn focus_text_area(
    id: impl Into<SharedString>,
    value: &str,
    focused: bool,
    min_height: f32,
    on_focus: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let display = if focused {
        if value.is_empty() {
            "▌".to_string()
        } else {
            format!("{value}▌")
        }
    } else if value.is_empty() {
        "…".into()
    } else {
        value.to_string()
    };

    div()
        .id(id.into())
        .w_full()
        .min_h(px(min_height))
        .overflow_y_scroll()
        .p_2()
        .rounded_sm()
        .border_1()
        .border_color(if focused {
            Theme::accent()
        } else {
            Theme::border_light()
        })
        .bg(Theme::bg_elevated())
        .text_sm()
        .text_color(Theme::text())
        .cursor_text()
        .on_click(|_, _, cx| cx.stop_propagation())
        .on_mouse_down(gpui::MouseButton::Left, {
            move |_, w, cx| {
                cx.stop_propagation();
                on_focus(w, cx);
            }
        })
        .child(display)
}

/// Dialog footer with Cancel + primary action.
pub fn dialog_actions(
    cancel_id: impl Into<SharedString>,
    cancel_label: impl Into<SharedString>,
    ok_id: impl Into<SharedString>,
    ok_label: impl Into<SharedString>,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
    on_ok: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    h_flex()
        .justify_end()
        .gap_2()
        .mt_3()
        .child(secondary_btn(cancel_id, cancel_label, move |_, w, cx| {
            on_cancel(w, cx)
        }))
        .child(primary_btn(ok_id, ok_label, move |_, w, cx| on_ok(w, cx)))
}

/// Status-bar profile label — plain tinted text (no outline chip).
pub fn status_tag(running: bool, label: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_xs()
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(if running {
            Theme::accent()
        } else {
            Theme::text_muted()
        })
        .child(label.into())
}

/// Inline notice line (routing save hints, etc.) — plain text, not Alert chrome.
pub fn notice_banner(
    id: impl Into<SharedString>,
    text: impl Into<SharedString>,
) -> impl IntoElement {
    div()
        .id(id.into())
        .w_full()
        .px_2()
        .py_1()
        .rounded_sm()
        .bg(Theme::accent_soft())
        .text_xs()
        .text_color(Theme::accent())
        .child(text.into())
}

/// Floating dropdown / context panel — solid elevated chrome so it never shows
/// through to group tabs / table underneath.
pub fn menu_panel(
    id: impl Into<SharedString>,
    min_width: f32,
    body: impl IntoElement,
) -> impl IntoElement {
    div()
        .id(id.into())
        .min_w(px(min_width))
        .py_0p5()
        .bg(Theme::bg_elevated())
        .border_1()
        .border_color(Theme::border_light())
        .rounded_md()
        .shadow_lg()
        .occlude()
        .child(body)
}

/// Compact menu row (~22px) — denser than ghost Button / default PopupMenu items.
pub fn menu_item(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id.into())
        .h(px(MENU_ITEM_H))
        .px_2()
        .flex()
        .items_center()
        .text_sm()
        .text_color(Theme::text())
        .cursor_pointer()
        .hover(|s| s.bg(Theme::bg_hover()))
        .on_click(on_click)
        .child(label.into())
}

/// Checked-style menu row (active route, etc.).
pub fn menu_item_checked(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    checked: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let label = label.into();
    div()
        .id(id.into())
        .h(px(MENU_ITEM_H))
        .px_2()
        .flex()
        .items_center()
        .gap_1()
        .text_sm()
        .text_color(if checked {
            Theme::accent()
        } else {
            Theme::text()
        })
        .cursor_pointer()
        .hover(|s| s.bg(Theme::bg_hover()))
        .on_click(on_click)
        .child(div().w(px(12.)).child(if checked { "✓" } else { "" }))
        .child(label)
}

pub fn menu_separator() -> impl IntoElement {
    div()
        .h(px(1.))
        .mx_1()
        .my_0p5()
        .bg(Theme::border_light())
}

pub fn menu_label(text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .h(px(18.))
        .px_2()
        .flex()
        .items_center()
        .text_xs()
        .text_color(Theme::text_muted())
        .child(text.into())
}

/// Primary action — gpui-component primary [`Button`].
pub fn primary_btn(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    Button::new(id.into())
        .primary()
        .label(label)
        .with_size(Size::Small)
        .on_click(on_click)
}

/// Secondary / outline action — gpui-component outline [`Button`].
pub fn secondary_btn(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    Button::new(id.into())
        .outline()
        .label(label)
        .with_size(Size::Small)
        .on_click(on_click)
}

/// Compact square icon button (toolbar / log panel actions).
///
/// `icon_path` is an asset path such as `"icons/copy.svg"`.
pub fn icon_btn(
    id: impl Into<SharedString>,
    icon_path: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    Button::new(id.into())
        .ghost()
        .outline()
        .icon(icon_from_path(icon_path))
        .with_size(Size::Small)
        .on_click(on_click)
}

/// gpui-component [`TabBar`] — preferred for multi-tab rows (assigns stable tab ids).
pub fn tab_bar(
    id: impl Into<SharedString>,
    selected_index: usize,
    labels: impl IntoIterator<Item = SharedString>,
    on_click: impl Fn(&usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let mut bar = TabBar::new(id.into())
        .outline()
        .with_size(Size::Small)
        .selected_index(selected_index)
        .on_click(on_click);
    for label in labels {
        bar = bar.child(Tab::new().label(label));
    }
    bar
}
