//! Secondary feature dialogs (settings / groups / add / routing / tun / hotkeys).

use gpui::{App, SharedString, Window, div, prelude::*, px};
use chrono::{Local, TimeZone};

use throne_domain::{AppState, GroupId, ProfileId, ProfileType, RulesetMirror};

use crate::theme::Theme;
use crate::ui::routing::RoutingDraft;
use crate::ui::widgets::{form_row, modal_shell, mode_checkbox, primary_btn, secondary_btn};

pub fn format_subscription_info(info: &str) -> Option<String> {
    let value = |key: &str| -> Option<u64> {
        info.split(';').find_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            name.trim()
                .eq_ignore_ascii_case(key)
                .then(|| value.trim().parse().ok())
                .flatten()
        })
    };
    let total = value("total")?;
    let used = value("upload")
        .unwrap_or(0)
        .saturating_add(value("download").unwrap_or(0));
    let remaining = if total == 0 {
        "∞".to_string()
    } else {
        readable_size(total.saturating_sub(used))
    };
    let expiry = value("expire")
        .filter(|seconds| *seconds > 0)
        .and_then(|seconds| Local.timestamp_opt(seconds as i64, 0).single())
        .map(|time| time.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "Never".into());
    Some(format!(
        "Used: {} Remain: {remaining} Expire: {expiry}",
        readable_size(used)
    ))
}

fn readable_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value.fract() == 0.0 {
        format!("{value:.0}{}", UNITS[unit])
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

#[derive(Clone, Debug, Default)]
pub enum Dialog {
    #[default]
    None,
    BasicSettings {
        inbound_address: String,
        inbound_port: String,
        test_url: String,
        remote_dns: String,
        direct_dns: String,
        log_level: String,
        ruleset_mirror: RulesetMirror,
        adblock_enable: bool,
        /// Which field is focused for keyboard edit: 0 addr 1 port 2 test 3 rdns 4 ddns 5 log
        focus: usize,
    },
    ManageGroups {
        /// draft name for "add group"
        new_name: String,
        /// selected group for edit/delete
        selected: Option<GroupId>,
        edit_name: String,
        edit_url: String,
        focus_new: bool,
    },
    AddFromInput {
        text: String,
    },
    /// Full upstream-style Routes dialog (tabs + draft + nested editors).
    RoutingSettings(RoutingDraft),
    TunSettings {
        vpn_mtu: String,
        vpn_strict_route: bool,
        /// When true, private ranges are NOT excluded from TUN (upstream flag name).
        disable_private_range_bypass: bool,
        focus_mtu: bool,
    },
    HotkeySettings {
        start_stop: String,
        import: String,
        save: String,
        url_test: String,
        copy_logs: String,
        /// 0..4 field index
        focus: usize,
    },
    /// Rename a single profile (display name).
    EditProfile {
        id: ProfileId,
        name: String,
        /// Read-only type label shown in the dialog.
        type_label: String,
    },
    ConfirmDeleteUnavailable {
        group_id: GroupId,
        count: usize,
    },
    ConfirmUpdateAllSubscriptions,
    SubscriptionDiff {
        title: String,
        body: String,
    },
}

impl Dialog {
    pub fn basic_from_state(state: &AppState) -> Self {
        let s = state.settings();
        Self::BasicSettings {
            inbound_address: s.inbound_address.clone(),
            inbound_port: s.inbound_socks_port.to_string(),
            test_url: s.test_latency_url.clone(),
            remote_dns: s.remote_dns.clone(),
            direct_dns: s.direct_dns.clone(),
            log_level: s.log_level.clone(),
            ruleset_mirror: s.ruleset_mirror,
            adblock_enable: s.adblock_enable,
            focus: 0,
        }
    }

    pub fn manage_groups_from_state(state: &AppState) -> Self {
        let selected = Some(state.active_group_id());
        let (edit_name, edit_url) = selected
            .and_then(|id| state.group(id))
            .map(|g| (g.name.clone(), g.url.clone()))
            .unwrap_or_default();
        Self::ManageGroups {
            new_name: String::new(),
            selected,
            edit_name,
            edit_url,
            focus_new: true,
        }
    }

    pub fn add_from_input() -> Self {
        Self::AddFromInput {
            text: String::new(),
        }
    }

    pub fn routing_from_state(state: &AppState) -> Self {
        Self::RoutingSettings(RoutingDraft::from_state(state))
    }

    pub fn tun_from_state(state: &AppState) -> Self {
        let s = state.settings();
        Self::TunSettings {
            vpn_mtu: s.vpn_mtu.to_string(),
            vpn_strict_route: s.vpn_strict_route,
            disable_private_range_bypass: s.disable_private_range_bypass,
            focus_mtu: true,
        }
    }

    pub fn hotkey_from_state(state: &AppState) -> Self {
        let s = state.settings();
        let or_def = |v: &str, d: &str| {
            if v.trim().is_empty() {
                d.to_string()
            } else {
                v.to_string()
            }
        };
        Self::HotkeySettings {
            start_stop: or_def(&s.hk_start_stop, "Cmd/Ctrl+R"),
            import: or_def(&s.hk_import, "Cmd/Ctrl+V"),
            save: or_def(&s.hk_save, "Cmd/Ctrl+S"),
            url_test: or_def(&s.hk_url_test, "Cmd/Ctrl+T"),
            copy_logs: or_def(&s.hk_copy_logs, "Cmd/Ctrl+Shift+C"),
            focus: 0,
        }
    }

    pub fn edit_profile_from_state(state: &AppState) -> Option<Self> {
        let id = state.selected_profile_id()?;
        let p = state.profile(id)?;
        Some(Self::EditProfile {
            id,
            name: p.name.clone(),
            type_label: p.profile_type.as_str().to_string(),
        })
    }
}

/// Edit-profile body: rename only (outbound JSON editing stays future work).
pub fn edit_profile_body(
    name: &str,
    type_label: &str,
    on_save: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .text_xs()
                .text_color(Theme::text_muted())
                .mb_2()
                .child(format!("Type: {type_label} · type to rename")),
        )
        .child(
            div()
                .id("ep-name")
                .px_2()
                .py_2()
                .mb_3()
                .rounded_sm()
                .border_1()
                .border_color(Theme::accent())
                .bg(Theme::bg_app())
                .text_sm()
                .cursor_text()
                .on_click(|_, _, cx| cx.stop_propagation())
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(if name.is_empty() {
                    "Name…▌".to_string()
                } else {
                    format!("{name}▌")
                }),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(secondary_btn("ep-cancel", "Cancel", move |_, w, cx| {
                    on_cancel(w, cx)
                }))
                .child(primary_btn("ep-ok", "Save", move |_, w, cx| on_save(w, cx))),
        )
}

pub fn confirm_delete_unavailable_body(
    count: usize,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(format!("Remove {count} unavailable item(s)?"))
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(secondary_btn("du-cancel", "Cancel", move |_, w, cx| {
                    on_cancel(w, cx)
                }))
                .child(primary_btn("du-confirm", "Remove", move |_, w, cx| {
                    on_confirm(w, cx)
                })),
        )
}

/// Build basic settings body (fields are display + focus highlight; typing handled by parent).
pub fn basic_settings_body(
    inbound_address: &str,
    inbound_port: &str,
    test_url: &str,
    remote_dns: &str,
    direct_dns: &str,
    log_level: &str,
    ruleset_mirror: RulesetMirror,
    adblock_enable: bool,
    focus: usize,
    on_focus: impl Fn(usize, &mut Window, &mut App) + Clone + 'static,
    on_cycle_mirror: impl Fn(&mut Window, &mut App) + 'static,
    on_toggle_adblock: impl Fn(&mut Window, &mut App) + 'static,
    on_save: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let mk = |idx: usize, label: &'static str, val: String| {
        let on_focus = on_focus.clone();
        let focused = focus == idx;
        div()
            .id(SharedString::from(format!("bs-f-{idx}")))
            .flex()
            .items_center()
            .gap_3()
            .mb_2()
            .cursor_pointer()
            .on_click(move |_, window, cx| on_focus(idx, window, cx))
            .child(
                div()
                    .w(px(150.))
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
                    .border_color(if focused {
                        Theme::accent()
                    } else {
                        Theme::border_light()
                    })
                    .bg(Theme::bg_app())
                    .text_sm()
                    .text_color(Theme::text())
                    .child(if val.is_empty() && focused {
                        "▌".to_string()
                    } else if focused {
                        format!("{val}▌")
                    } else {
                        val
                    }),
            )
    };

    div()
        .flex()
        .flex_col()
        .child(div().text_xs().text_color(Theme::text_muted()).mb_3().child(
            "Basic Settings · LAN: set Inbound address to 0.0.0.0, then restart (no auth)",
        ))
        .child(mk(0, "Inbound address", inbound_address.to_string()))
        .child(mk(1, "Mixed / SOCKS port", inbound_port.to_string()))
        .child(mk(2, "Test URL", test_url.to_string()))
        .child(mk(3, "Remote DNS", remote_dns.to_string()))
        .child(mk(4, "Direct DNS", direct_dns.to_string()))
        .child(mk(5, "Log level", log_level.to_string()))
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .mb_2()
                .child(
                    div()
                        .w(px(150.))
                        .text_xs()
                        .text_color(Theme::text_muted())
                        .child("Rule-set mirror"),
                )
                .child(secondary_btn(
                    "bs-mirror",
                    ruleset_mirror.label(),
                    move |_, w, cx| on_cycle_mirror(w, cx),
                )),
        )
        .child(
            div()
                .mb_2()
                .child(mode_checkbox(
                    "bs-adblock",
                    "Adblock rule-set (Start)",
                    adblock_enable,
                    move |_, w, cx| on_toggle_adblock(w, cx),
                )),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .mt_4()
                .child(secondary_btn("bs-cancel", "Cancel", move |_, w, cx| {
                    on_cancel(w, cx)
                }))
                .child(primary_btn("bs-save", "Save", move |_, w, cx| on_save(w, cx))),
        )
}

pub fn manage_groups_body(
    state: &AppState,
    new_name: &str,
    selected: Option<GroupId>,
    edit_name: &str,
    edit_url: &str,
    focus_new: bool,
    on_select: impl Fn(GroupId, &mut Window, &mut App) + Clone + 'static,
    on_focus_new: impl Fn(&mut Window, &mut App) + 'static,
    on_focus_edit_name: impl Fn(&mut Window, &mut App) + 'static,
    on_focus_edit_url: impl Fn(&mut Window, &mut App) + 'static,
    on_add: impl Fn(&mut Window, &mut App) + 'static,
    on_apply: impl Fn(&mut Window, &mut App) + 'static,
    on_delete: impl Fn(&mut Window, &mut App) + 'static,
    on_update: impl Fn(GroupId, &mut Window, &mut App) + Clone + 'static,
    on_update_all: impl Fn(&mut Window, &mut App) + 'static,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let mut list = div()
        .id("mg-list")
        .flex()
        .flex_col()
        .gap_1()
        .mb_3()
        .max_h(px(280.))
        .overflow_y_scroll();
    for g in state.all_groups() {
        let id = g.id;
        let sel = selected == Some(id);
        let on_select = on_select.clone();
        let on_update = on_update.clone();
        let name = if g.name.is_empty() { format!("Group {id}") } else { g.name.clone() };
        let kind = match (g.archive, g.url.trim().is_empty()) {
            (true, true) => "Archive Basic",
            (true, false) => "Archive Subscription",
            (false, true) => "Basic",
            (false, false) => "Subscription",
        };
        let url = g.url.clone();
        let has_url = !url.trim().is_empty();
        let metadata = group_subscription_metadata(g);
        let mut header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(format!("{kind} ({})  {name}", g.profile_ids.len()));
        if has_url && !g.archive {
            header = header.child(secondary_btn(
                SharedString::from(format!("mg-update-{id}")),
                "Update Subscription",
                move |_, w, cx| on_update(id, w, cx),
            ));
        }
        list = list.child(
            div()
                .id(SharedString::from(format!("mg-{id}")))
                .flex()
                .flex_col()
                .gap_1()
                .px_2()
                .py_2()
                .rounded_sm()
                .border_1()
                .border_color(if sel { Theme::accent() } else { Theme::border_light() })
                .cursor_pointer()
                .bg(if sel {
                    Theme::bg_selected()
                } else {
                    Theme::bg_app()
                })
                .text_color(if sel {
                    Theme::text_on_selected()
                } else {
                    Theme::text()
                })
                .text_sm()
                .child(header)
                .when(has_url, |el| {
                    el.child(div().text_xs().text_color(Theme::text_muted()).child(url))
                })
                .when_some(metadata, |el, text| {
                    el.child(div().text_xs().text_color(Theme::text_muted()).child(text))
                })
                .on_click(move |_, w, cx| on_select(id, w, cx)),
        );
    }

    div()
        .flex()
        .flex_col()
        .child(div().text_xs().text_color(Theme::text_muted()).mb_2().child("Groups"))
        .child(list)
        .child(menu_sep())
        .child(div().text_xs().text_color(Theme::text_muted()).mb_1().child("Add new group"))
        .child(
            div()
                .id("mg-new")
                .px_2()
                .py_1()
                .mb_2()
                .rounded_sm()
                .border_1()
                .border_color(if focus_new {
                    Theme::accent()
                } else {
                    Theme::border_light()
                })
                .bg(Theme::bg_app())
                .text_sm()
                .cursor_pointer()
                .on_click(move |_, w, cx| on_focus_new(w, cx))
                .child(if focus_new {
                    format!("{}▌", new_name)
                } else if new_name.is_empty() {
                    "New group name…".into()
                } else {
                    new_name.to_string()
                }),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .mb_3()
                .child(primary_btn("mg-add", "New group", move |_, w, cx| on_add(w, cx)))
                .child(secondary_btn(
                    "mg-update-all",
                    "Update all subscriptions",
                    move |_, w, cx| on_update_all(w, cx),
                )),
        )
        .child(menu_sep())
        .child(div().text_xs().text_color(Theme::text_muted()).mb_1().child("Edit current Group"))
        .child(
            div()
                .id("mg-ename")
                .px_2()
                .py_1()
                .mb_1()
                .rounded_sm()
                .border_1()
                .border_color(Theme::border_light())
                .bg(Theme::bg_app())
                .text_sm()
                .cursor_pointer()
                .on_click(move |_, w, cx| on_focus_edit_name(w, cx))
                .child(format!("Name: {edit_name}")),
        )
        .child(
            div()
                .id("mg-eurl")
                .px_2()
                .py_1()
                .mb_2()
                .rounded_sm()
                .border_1()
                .border_color(Theme::border_light())
                .bg(Theme::bg_app())
                .text_sm()
                .cursor_pointer()
                .on_click(move |_, w, cx| on_focus_edit_url(w, cx))
                .child(if edit_url.is_empty() {
                    "Subscription URL: (none)".into()
                } else {
                    format!("Subscription URL: {edit_url}")
                }),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .child(primary_btn("mg-apply", "Apply", move |_, w, cx| on_apply(w, cx)))
                .child(secondary_btn("mg-del", "Delete current Group", move |_, w, cx| {
                    on_delete(w, cx)
                }))
                .child(secondary_btn("mg-close", "Close", move |_, w, cx| on_close(w, cx))),
        )
}

fn group_subscription_metadata(group: &throne_domain::Group) -> Option<String> {
    let mut parts = Vec::new();
    if group.sub_last_update > 0 {
        if let Some(time) = Local.timestamp_opt(group.sub_last_update, 0).single() {
            parts.push(format!("Last update: {}", time.format("%Y-%m-%d %H:%M")));
        }
    }
    if let Some(info) = format_subscription_info(&group.info) {
        parts.push(info);
    }
    (!parts.is_empty()).then(|| parts.join(" | "))
}

pub fn confirm_update_all_body(
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .child(div().mb_3().child("Update all subscriptions?"))
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(secondary_btn("sub-all-no", "No", move |_, w, cx| on_cancel(w, cx)))
                .child(primary_btn("sub-all-yes", "Yes", move |_, w, cx| on_confirm(w, cx))),
        )
}

pub fn subscription_diff_body(
    body: &str,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .id("sub-diff-scroll")
                .max_h(px(360.))
                .overflow_y_scroll()
                .p_2()
                .mb_3()
                .border_1()
                .border_color(Theme::border_light())
                .text_sm()
                .child(body.to_string()),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .child(primary_btn("sub-diff-close", "Close", move |_, w, cx| on_close(w, cx))),
        )
}

fn menu_sep() -> impl IntoElement {
    div()
        .h(px(1.))
        .w_full()
        .my_2()
        .bg(Theme::border_light())
}

pub fn add_input_body(
    text: &str,
    on_save: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .text_xs()
                .text_color(Theme::text_muted())
                .mb_2()
                .child("New profile — paste share link(s), Clash YAML, or JSON (type to edit)"),
        )
        .child(
            div()
                .id("ai-box")
                .min_h(px(160.))
                .p_2()
                .mb_3()
                .rounded_sm()
                .border_1()
                .border_color(Theme::accent())
                .bg(Theme::bg_app())
                .text_sm()
                .text_color(Theme::text())
                .cursor_text()
                // Capture clicks so they never fall through the modal stack.
                .on_click(|_, _, cx| cx.stop_propagation())
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(if text.is_empty() {
                    "▌".to_string()
                } else {
                    format!("{text}▌")
                }),
        )
        .child(
            div()
                .text_xs()
                .text_color(Theme::text_muted())
                .mb_3()
                .child(format!(
                    "Detected type hint: {}",
                    detect_hint(text)
                )),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(secondary_btn("ai-cancel", "Cancel", move |_, w, cx| {
                    on_cancel(w, cx)
                }))
                .child(primary_btn("ai-ok", "OK", move |_, w, cx| on_save(w, cx))),
        )
}

fn detect_hint(text: &str) -> &'static str {
    let t = text.trim();
    if t.is_empty() {
        return "(empty)";
    }
    if t.contains("proxies:") {
        return "Clash YAML";
    }
    if t.starts_with('{') || t.starts_with('[') {
        return "JSON subscription";
    }
    if t.contains("://") {
        return "Share link(s)";
    }
    "Unknown — will try import anyway"
}

pub fn tun_settings_body(
    vpn_mtu: &str,
    vpn_strict_route: bool,
    disable_private_range_bypass: bool,
    focus_mtu: bool,
    on_focus_mtu: impl Fn(&mut Window, &mut App) + 'static,
    on_toggle_strict: impl Fn(&mut Window, &mut App) + 'static,
    on_toggle_bypass: impl Fn(&mut Window, &mut App) + 'static,
    on_save: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .text_xs()
                .text_color(Theme::text_muted())
                .mb_3()
                .child(
                    "Tun Mode settings — applied on next Start when Tun is checked. \
                     macOS/Linux require elevated ThroneCore (setuid). \
                     On macOS also set Routing → Local override to a plain DNS IP \
                     (same as upstream Throne; empty Local DNS + Tun will fail to start).",
                ),
        )
        .child(
            div()
                .id("tun-mtu")
                .flex()
                .items_center()
                .gap_3()
                .mb_3()
                .cursor_pointer()
                .on_click(move |_, w, cx| on_focus_mtu(w, cx))
                .child(
                    div()
                        .w(px(140.))
                        .text_xs()
                        .text_color(Theme::text_muted())
                        .child("MTU"),
                )
                .child(
                    div()
                        .flex_1()
                        .px_2()
                        .py_1()
                        .rounded_sm()
                        .border_1()
                        .border_color(if focus_mtu {
                            Theme::accent()
                        } else {
                            Theme::border_light()
                        })
                        .bg(Theme::bg_app())
                        .text_sm()
                        .child(if focus_mtu {
                            format!("{vpn_mtu}▌")
                        } else {
                            vpn_mtu.to_string()
                        }),
                ),
        )
        .child(
            div().mb_2().child(mode_checkbox(
                "tun-strict",
                "Strict route",
                vpn_strict_route,
                move |_, w, cx| on_toggle_strict(w, cx),
            )),
        )
        .child(
            div().mb_3().child(mode_checkbox(
                "tun-bypass",
                "Bypass private LAN ranges (recommended)",
                !disable_private_range_bypass,
                move |_, w, cx| on_toggle_bypass(w, cx),
            )),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(secondary_btn("tun-cancel", "Cancel", move |_, w, cx| {
                    on_cancel(w, cx)
                }))
                .child(primary_btn("tun-save", "Save", move |_, w, cx| on_save(w, cx))),
        )
}

pub fn hotkey_settings_body(
    start_stop: &str,
    import: &str,
    save: &str,
    url_test: &str,
    copy_logs: &str,
    focus: usize,
    on_focus: impl Fn(usize, &mut Window, &mut App) + Clone + 'static,
    on_save: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let mk = |idx: usize, label: &'static str, val: &str| {
        let on_focus = on_focus.clone();
        let focused = focus == idx;
        let val = val.to_string();
        div()
            .id(SharedString::from(format!("hk-f-{idx}")))
            .flex()
            .items_center()
            .gap_3()
            .mb_2()
            .cursor_pointer()
            .on_click(move |_, w, cx| on_focus(idx, w, cx))
            .child(
                div()
                    .w(px(150.))
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
                    .border_color(if focused {
                        Theme::accent()
                    } else {
                        Theme::border_light()
                    })
                    .bg(Theme::bg_app())
                    .text_sm()
                    .child(if focused {
                        format!("{val}▌")
                    } else {
                        val
                    }),
            )
    };

    div()
        .flex()
        .flex_col()
        .child(
            div()
                .text_xs()
                .text_color(Theme::text_muted())
                .mb_3()
                .child(
                    "Hotkey labels (stored in settings). Runtime rebind of custom chords \
                     lands with global hotkey registration.",
                ),
        )
        .child(mk(0, "Start / Stop", start_stop))
        .child(mk(1, "Import clipboard", import))
        .child(mk(2, "Save database", save))
        .child(mk(3, "URL Test", url_test))
        .child(mk(4, "Copy logs", copy_logs))
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .mt_3()
                .child(secondary_btn("hk-cancel", "Cancel", move |_, w, cx| {
                    on_cancel(w, cx)
                }))
                .child(primary_btn("hk-save", "Save", move |_, w, cx| on_save(w, cx))),
        )
}

// Keep imports available for future dialog fields.
#[allow(dead_code)]
fn _pt() -> ProfileType {
    ProfileType::Vless
}

#[allow(dead_code)]
fn _fr() {
    let _ = form_row("x", "y");
    let _ = modal_shell("t", div(), |_, _, _| {});
}

#[cfg(test)]
mod tests {
    use super::format_subscription_info;

    #[test]
    fn subscription_info_formats_usage_remaining_and_expiry() {
        let text = format_subscription_info(
            "upload=10; download=15; total=100; expire=1785500000",
        )
        .unwrap();
        assert!(text.contains("Used: 25B"));
        assert!(text.contains("Remain: 75B"));
        assert!(text.contains("Expire:"));
    }

    #[test]
    fn subscription_info_requires_valid_total() {
        assert_eq!(format_subscription_info("upload=10; download=15"), None);
        assert_eq!(format_subscription_info("total=broken"), None);
    }

    #[test]
    fn zero_total_formats_as_unlimited_remaining() {
        let text = format_subscription_info("upload=25; total=0").unwrap();
        assert!(text.contains("Remain: ∞"));
    }
}
