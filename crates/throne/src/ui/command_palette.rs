//! Searchable native commands supplied by the main window.

use gpui::{
    div, prelude::*, px, size, App, Bounds, Context, Entity, Focusable, Render, Subscription,
    TitlebarOptions, Window, WindowBounds, WindowOptions,
};
use gpui_component::{
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement as _,
    Root,
};

use crate::{
    theme::Theme,
    ui::widgets::{secondary_btn, section_hint},
};

type Apply = Box<dyn Fn(&str, &mut App)>;

pub fn open(entries: Vec<(String, String)>, apply: Apply, cx: &mut App) -> anyhow::Result<()> {
    let bounds = Bounds::centered(None, size(px(560.), px(440.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Command Palette".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| Palette::new(entries, apply, window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        },
    )?;
    Ok(())
}

struct Palette {
    entries: Vec<(String, String)>,
    input: Entity<InputState>,
    apply: Apply,
    _subscription: Subscription,
}

impl Palette {
    fn new(
        entries: Vec<(String, String)>,
        apply: Apply,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search commands…"));
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::PressEnter { .. } => {
                    if let Some(index) =
                        matching_indices(&this.entries, &this.input.read(cx).value()).first()
                    {
                        this.run(*index, window, cx);
                    }
                }
                _ => {}
            },
        );
        input.read(cx).focus_handle(cx).focus(window, cx);
        Self {
            entries,
            input,
            apply,
            _subscription: subscription,
        }
    }

    fn run(&self, index: usize, window: &mut Window, cx: &mut App) {
        window.remove_window();
        (self.apply)(&self.entries[index].0, cx);
    }
}

impl Render for Palette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let matches = matching_indices(&self.entries, &self.input.read(cx).value());
        let mut list = div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap_2()
            .overflow_y_scrollbar();
        if matches.is_empty() {
            list = list.child(section_hint("No matching commands"));
        }
        for index in matches {
            let entity = cx.entity();
            list = list.child(secondary_btn(
                format!("command-{index}"),
                self.entries[index].1.clone(),
                move |_, window, cx| entity.update(cx, |this, cx| this.run(index, window, cx)),
            ));
        }
        div()
            .size_full()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .bg(Theme::bg_elevated())
            .text_color(Theme::text())
            .capture_key_down(|event, window, cx| {
                if event.keystroke.key == "escape" {
                    window.remove_window();
                    cx.stop_propagation();
                }
            })
            .child(Input::new(&self.input).cleanable(true))
            .child(section_hint("Enter runs the first result · Escape closes"))
            .child(list)
    }
}

fn matching_indices(entries: &[(String, String)], query: &str) -> Vec<usize> {
    let query = query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let tokens = query.split_whitespace().collect::<Vec<_>>();
    let mut matches = entries
        .iter()
        .enumerate()
        .filter_map(|(index, (_, label))| {
            let label = label.to_lowercase();
            tokens.iter().all(|token| label.contains(token)).then(|| {
                let rank = if query.is_empty() || label == query {
                    0
                } else if label.starts_with(&query) {
                    1
                } else {
                    2
                };
                (rank, index)
            })
        })
        .collect::<Vec<_>>();
    matches.sort_unstable();
    matches.into_iter().map(|(_, index)| index).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<(String, String)> {
        [
            ("dns", "Settings: DNS"),
            ("proxy", "Settings: Proxy"),
            ("settings", "Settings"),
            ("start", "Start Profile"),
            ("open", "Open Settings"),
        ]
        .into_iter()
        .map(|(key, label)| (key.into(), label.into()))
        .collect()
    }

    #[test]
    fn every_query_word_matches_case_insensitively_in_any_order() {
        assert_eq!(matching_indices(&entries(), "  DNS   SeTT "), [0]);
        assert!(matching_indices(&entries(), "dns profile").is_empty());
    }

    #[test]
    fn exact_then_prefix_then_other_matches_keep_entry_keys_and_order() {
        let entries = entries();
        let keys = matching_indices(&entries, "settings")
            .into_iter()
            .map(|i| entries[i].0.as_str())
            .collect::<Vec<_>>();
        assert_eq!(keys, ["settings", "dns", "proxy", "open"]);
        assert_eq!(matching_indices(&entries, " "), [0, 1, 2, 3, 4]);
        assert_eq!(matching_indices(&entries, "profile"), [3]);
    }

    #[test]
    fn no_match_has_no_executable_first_result() {
        assert!(matching_indices(&entries(), "missing").first().is_none());
        assert!(matching_indices(&[], "").first().is_none());
    }
}
