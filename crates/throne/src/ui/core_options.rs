//! Native core and multiplex presets. Save patches only fields changed by this window.
use crate::{
    theme::Theme,
    ui::widgets::{dialog_actions, input_field_row, section_hint, settings_switch_row, tab_bar},
};
use gpui::{
    div, prelude::*, px, size, App, Bounds, Context, Entity, SharedString, TitlebarOptions, Window,
    WindowBounds, WindowOptions,
};
use gpui_component::{input::InputState, scroll::ScrollableElement as _, Root};
use throne_domain::AppSettings;
type Save = Box<dyn Fn(AppSettings, &mut App) -> Result<(), String>>;
const PROTOCOLS: [&str; 3] = ["smux", "yamux", "h2mux"];
const MODES: [&str; 2] = ["max_streams", "max_connections"];
const LABELS: [&str; 5] = [
    "Maximum streams",
    "Maximum connections",
    "Minimum streams",
    "Brutal upload (Mbps)",
    "Brutal download (Mbps)",
];

pub fn open(
    settings: AppSettings,
    save: impl Fn(AppSettings, &mut App) -> Result<(), String> + 'static,
    cx: &mut App,
) -> anyhow::Result<()> {
    let bounds = Bounds::centered(None, size(px(560.), px(650.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Core options".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| {
                let inputs = [
                    settings.mux_concurrency,
                    settings.mux_max_connections,
                    settings.mux_min_streams,
                    settings.mux_brutal_up_mbps,
                    settings.mux_brutal_down_mbps,
                ]
                .into_iter()
                .map(|value| {
                    cx.new(|cx| InputState::new(window, cx).default_value(value.to_string()))
                })
                .collect();
                Options {
                    settings,
                    inputs,
                    save: Box::new(save),
                    error: String::new(),
                }
            });
            cx.new(|cx| Root::new(view, window, cx))
        },
    )?;
    Ok(())
}
/// Merge only edited form fields into the latest settings, preserving concurrent changes.
pub fn apply_changed(initial: &AppSettings, edited: &AppSettings, current: &mut AppSettings) {
    macro_rules! copy_changed { ($($field:ident),+ $(,)?) => { $(if initial.$field != edited.$field { current.$field.clone_from(&edited.$field); })+ }; }
    copy_changed!(
        disable_mixed_inbound,
        xray_use_system_ca,
        mux_default_on,
        mux_protocol,
        mux_mode,
        mux_concurrency,
        mux_max_connections,
        mux_min_streams,
        mux_padding,
        mux_brutal,
        mux_brutal_up_mbps,
        mux_brutal_down_mbps
    );
}
fn read_numbers(mut settings: AppSettings, text: &[String]) -> Result<AppSettings, String> {
    if !PROTOCOLS.contains(&settings.mux_protocol.as_str()) {
        return Err("Choose smux, yamux or h2mux.".into());
    }
    if !MODES.contains(&settings.mux_mode.as_str()) {
        return Err("Choose a multiplex pool mode.".into());
    }
    let mut values = [0; 5];
    for (index, value) in values.iter_mut().enumerate() {
        *value = text
            .get(index)
            .and_then(|v| v.trim().parse::<i32>().ok())
            .filter(|v| *v >= 0)
            .ok_or_else(|| format!("{} must be a nonnegative whole number.", LABELS[index]))?;
    }
    if settings.mux_brutal && (values[3] == 0 || values[4] == 0) {
        return Err("Brutal upload and download must be greater than zero when enabled.".into());
    }
    [
        settings.mux_concurrency,
        settings.mux_max_connections,
        settings.mux_min_streams,
        settings.mux_brutal_up_mbps,
        settings.mux_brutal_down_mbps,
    ] = values;
    Ok(settings)
}
struct Options {
    settings: AppSettings,
    inputs: Vec<Entity<InputState>>,
    save: Save,
    error: String,
}
impl Render for Options {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = div()
            .id("core-options-fields")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scrollbar()
            .flex()
            .flex_col()
            .gap_2();
        for (index, label, value) in [
            (
                0,
                "Disable mixed SOCKS / HTTP inbound",
                self.settings.disable_mixed_inbound,
            ),
            (
                1,
                "Use system certificates for Xray TLS",
                self.settings.xray_use_system_ca,
            ),
            (
                2,
                "Enable multiplex by default",
                self.settings.mux_default_on,
            ),
            (3, "Multiplex padding", self.settings.mux_padding),
            (4, "TCP Brutal", self.settings.mux_brutal),
        ] {
            let entity = cx.entity();
            body = body.child(settings_switch_row(
                format!("core-option-{index}"),
                label,
                value,
                move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        let field = match index {
                            0 => &mut this.settings.disable_mixed_inbound,
                            1 => &mut this.settings.xray_use_system_ca,
                            2 => &mut this.settings.mux_default_on,
                            3 => &mut this.settings.mux_padding,
                            _ => &mut this.settings.mux_brutal,
                        };
                        *field = !*field;
                        cx.notify();
                    });
                },
            ));
        }
        for (index, title, choices, selected) in [
            (
                0,
                "Multiplex protocol",
                PROTOCOLS.as_slice(),
                self.settings.mux_protocol.as_str(),
            ),
            (
                1,
                "Multiplex pool limit",
                MODES.as_slice(),
                self.settings.mux_mode.as_str(),
            ),
        ] {
            let entity = cx.entity();
            body = body.child(section_hint(title)).child(tab_bar(
                format!("mux-choice-{index}"),
                choices
                    .iter()
                    .position(|v| *v == selected)
                    .unwrap_or(usize::MAX),
                choices.iter().map(|v| SharedString::from(*v)),
                move |choice, _, cx| {
                    if let Some(value) = choices.get(*choice) {
                        entity.update(cx, |this, cx| {
                            if index == 0 {
                                this.settings.mux_protocol = (*value).into();
                            } else {
                                this.settings.mux_mode = (*value).into();
                            }
                            cx.notify();
                        });
                    }
                },
            ));
        }
        for (label, input) in LABELS.into_iter().zip(&self.inputs) {
            body = body.child(input_field_row(label, input, 190.));
        }
        body = body.child(section_hint("Changes apply on the next profile start. Multiplex presets apply to supported protocols unless a profile overrides them. Brutal requires server support and positive bandwidth limits."))
            .child(section_hint("Disabling mixed inbound removes the local SOCKS / HTTP listener; TUN remains separate. System certificates supplement Xray TLS trust without disabling verification."));
        let entity = cx.entity();
        let actions = dialog_actions(
            "core-options-cancel",
            "Cancel",
            "core-options-save",
            "Save",
            |window, _| window.remove_window(),
            move |window, cx| {
                entity.update(cx, |this, cx| {
                    let text = this
                        .inputs
                        .iter()
                        .map(|i| i.read(cx).value().to_string())
                        .collect::<Vec<_>>();
                    match read_numbers(this.settings.clone(), &text)
                        .and_then(|settings| (this.save)(settings, cx))
                    {
                        Ok(()) => window.remove_window(),
                        Err(error) => {
                            this.error = error;
                            cx.notify();
                        }
                    }
                });
            },
        );
        div()
            .size_full()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .bg(Theme::bg_elevated())
            .text_color(Theme::text())
            .child(body)
            .when(!self.error.is_empty(), |el| {
                el.child(section_hint(self.error.clone()))
            })
            .child(actions)
    }
}
#[cfg(test)]
#[path = "core_options_tests.rs"]
mod tests;
