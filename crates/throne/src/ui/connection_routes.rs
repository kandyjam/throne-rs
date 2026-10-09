use gpui::{
    div, prelude::*, px, size, App, Bounds, Context, Render, TitlebarOptions, Window, WindowBounds,
    WindowOptions,
};
use gpui_component::{scroll::ScrollableElement as _, Root};
use throne_domain::{ConnectionRouteTarget, RouteProfile, SimpleAction};

use crate::{
    theme::Theme,
    ui::widgets::{primary_btn, section_hint, settings_switch_row},
};

type Apply = Box<dyn Fn(&[(String, bool)], &mut App) -> Result<(), String>>;

pub fn open(
    route: RouteProfile,
    targets: Vec<ConnectionRouteTarget>,
    action: SimpleAction,
    apply: Apply,
    cx: &mut App,
) -> anyhow::Result<()> {
    let bounds = Bounds::centered(None, size(px(560.), px(480.)), cx);
    let title = format!("{} rules · {}", action.label(), route.name);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(title.into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|_| {
                let checked: Vec<bool> = targets
                    .iter()
                    .map(|t| route.has_simple_rule(&t.rule, action))
                    .collect();
                Targets {
                    route,
                    targets,
                    initial: checked.clone(),
                    checked,
                    action,
                    apply,
                    error: String::new(),
                }
            });
            cx.new(|cx| Root::new(view, window, cx))
        },
    )?;
    Ok(())
}

struct Targets {
    route: RouteProfile,
    targets: Vec<ConnectionRouteTarget>,
    initial: Vec<bool>,
    checked: Vec<bool>,
    action: SimpleAction,
    apply: Apply,
    error: String,
}

impl Render for Targets {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = div().size_full().p_4().flex().flex_col().gap_3().bg(Theme::bg_elevated())
            .text_color(Theme::text()).overflow_y_scrollbar()
            .child(section_hint("Check a target to add or move its rule here. Uncheck to remove it. Only changed targets are saved."));
        for (index, target) in self.targets.iter().enumerate() {
            let entity = cx.entity();
            let elsewhere = [
                SimpleAction::Bypass,
                SimpleAction::Proxy,
                SimpleAction::Block,
                SimpleAction::WarpBypass,
            ]
            .into_iter()
            .filter(|a| *a != self.action && self.route.has_simple_rule(&target.rule, *a))
            .map(|a| a.label())
            .collect::<Vec<_>>()
            .join(", ");
            let mut label = target.label.clone();
            if !elsewhere.is_empty() {
                label.push_str(&format!(" · moves from {elsewhere}"));
            }
            let mut row = div().flex().flex_col().gap_1();
            if target.separator_before {
                row = row.border_t_1().border_color(Theme::border_light()).pt_3();
            }
            row = row.child(settings_switch_row(
                format!("target-{index}"),
                label,
                self.checked[index],
                move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        this.checked[index] = !this.checked[index];
                        cx.notify();
                    });
                },
            ));
            if let Some((rule, action)) = self.route.covering_simple_rule(&target.rule, self.action)
            {
                row = row.child(section_hint(format!(
                    "Covered by {rule} in {}",
                    action.label()
                )));
            }
            body = body.child(row);
        }
        let entity = cx.entity();
        body.child(section_hint(self.error.clone()))
            .child(primary_btn(
                "save-targets",
                "Apply rules",
                move |_, window, cx| {
                    entity.update(cx, |this, cx| {
                        let changes = this
                            .targets
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| this.checked[*i] != this.initial[*i])
                            .map(|(i, target)| (target.rule.clone(), this.checked[i]))
                            .collect::<Vec<_>>();
                        match (this.apply)(&changes, cx) {
                            Ok(()) => window.remove_window(),
                            Err(error) => {
                                this.error = error;
                                cx.notify();
                            }
                        }
                    });
                },
            ))
    }
}

pub fn apply_changes(
    route: &mut RouteProfile,
    changes: &[(String, bool)],
    action: SimpleAction,
) -> Result<(), String> {
    let mut changed = route.clone();
    for (rule, enabled) in changes {
        if *enabled {
            changed.append_simple_rule(rule, action)?;
            for other in [
                SimpleAction::Bypass,
                SimpleAction::Proxy,
                SimpleAction::Block,
                SimpleAction::WarpBypass,
            ] {
                if other != action {
                    changed.remove_simple_rule(rule, other)?;
                }
            }
        } else {
            changed.remove_simple_rule(rule, action)?;
        }
    }
    *route = changed;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_one_target_removes_conflicts_and_preserves_other_rules() {
        let mut route = RouteProfile::new(1, "test");
        route
            .append_simple_rule("suffix:example.org", SimpleAction::Bypass)
            .unwrap();
        route
            .append_simple_rule("domain:other.org", SimpleAction::Bypass)
            .unwrap();
        apply_changes(
            &mut route,
            &[("suffix:example.org".into(), true)],
            SimpleAction::Proxy,
        )
        .unwrap();
        assert!(route.has_simple_rule("suffix:example.org", SimpleAction::Proxy));
        assert!(!route.has_simple_rule("suffix:example.org", SimpleAction::Bypass));
        assert!(route.has_simple_rule("domain:other.org", SimpleAction::Bypass));
        apply_changes(
            &mut route,
            &[("suffix:example.org".into(), false)],
            SimpleAction::Proxy,
        )
        .unwrap();
        assert!(!route.has_simple_rule("suffix:example.org", SimpleAction::Proxy));
    }

    #[test]
    fn invalid_batch_does_not_partially_mutate_route() {
        let mut route = RouteProfile::new(1, "test");
        let result = apply_changes(
            &mut route,
            &[
                ("suffix:example.org".into(), true),
                ("bad:rule".into(), true),
            ],
            SimpleAction::Proxy,
        );
        assert!(result.is_err());
        assert!(!route.has_simple_rule("suffix:example.org", SimpleAction::Proxy));
    }
}
