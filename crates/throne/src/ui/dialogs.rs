//! Secondary feature dialogs (settings / groups / add / routing / tun / hotkeys).

use gpui::{App, Entity, SharedString, Window, div, prelude::*, px};
use chrono::{Local, TimeZone};
use gpui_component::input::InputState;

use throne_domain::{AppState, GroupId, ProfileId, RulesetMirror};

use crate::theme::Theme;
use crate::ui::routing::RoutingDraft;
use crate::ui::widgets::{
    dialog_actions, group_panel, hotkey_capture_row, input_area, input_field_row, mode_switch,
    primary_btn, secondary_btn, section_hint,
};

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
    /// Upstream DialogManageGroups — group list + New / Update all.
    ManageGroups,
    /// Upstream DialogEditGroup (new or existing).
    EditGroup {
        /// `None` = new group (type can change); `Some` = edit (type locked).
        group_id: Option<GroupId>,
        is_subscription: bool,
        skip_auto_update: bool,
        auto_clear_unavailable: bool,
        front_proxy_id: i64,
        landing_proxy_id: i64,
    },
    /// Confirm remove group (upstream GroupItem remove QMessageBox).
    ConfirmRemoveGroup {
        group_id: GroupId,
        name: String,
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

    pub fn manage_groups_from_state(_state: &AppState) -> Self {
        Self::ManageGroups
    }

    pub fn edit_group_from_state(state: &AppState, group_id: GroupId) -> Option<Self> {
        let g = state.group(group_id)?;
        Some(Self::EditGroup {
            group_id: Some(group_id),
            is_subscription: !g.url.trim().is_empty(),
            skip_auto_update: g.skip_auto_update,
            auto_clear_unavailable: g.auto_clear_unavailable,
            front_proxy_id: g.front_proxy_id,
            landing_proxy_id: g.landing_proxy_id,
        })
    }

    pub fn edit_group_new() -> Self {
        Self::EditGroup {
            group_id: None,
            is_subscription: false,
            skip_auto_update: false,
            auto_clear_unavailable: false,
            front_proxy_id: -1,
            landing_proxy_id: -1,
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
    name_input: &Entity<InputState>,
    type_label: &str,
    on_save: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .child(section_hint(format!("Type: {type_label} · edit name below")))
        .child(div().mb_3().child(input_field_row("Name", name_input, 80.)))
        .child(dialog_actions(
            "ep-cancel",
            "Cancel",
            "ep-ok",
            "Save",
            on_cancel,
            on_save,
        ))
}

/// Build basic settings body with real gpui-component Inputs.
pub fn basic_settings_body(
    inbound_address: &Entity<InputState>,
    inbound_port: &Entity<InputState>,
    test_url: &Entity<InputState>,
    remote_dns: &Entity<InputState>,
    direct_dns: &Entity<InputState>,
    log_level: &Entity<InputState>,
    ruleset_mirror: RulesetMirror,
    adblock_enable: bool,
    on_cycle_mirror: impl Fn(&mut Window, &mut App) + 'static,
    on_toggle_adblock: impl Fn(&mut Window, &mut App) + 'static,
    on_save: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .child(section_hint(
            "Basic Settings · LAN: set Inbound address to 0.0.0.0, then restart (no auth)",
        ))
        .child(input_field_row("Inbound address", inbound_address, 150.))
        .child(input_field_row("Mixed / SOCKS port", inbound_port, 150.))
        .child(input_field_row("Test URL", test_url, 150.))
        .child(input_field_row("Remote DNS", remote_dns, 150.))
        .child(input_field_row("Direct DNS", direct_dns, 150.))
        .child(input_field_row("Log level", log_level, 150.))
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
            div().mb_2().child(mode_switch(
                "bs-adblock",
                "Adblock rule-set (Start)",
                adblock_enable,
                move |_, w, cx| on_toggle_adblock(w, cx),
            )),
        )
        .child(dialog_actions(
            "bs-cancel",
            "Cancel",
            "bs-save",
            "Save",
            on_cancel,
            on_save,
        ))
}

/// Upstream DialogManageGroups: list of GroupItems + New group / Update all.
pub fn manage_groups_body(
    state: &AppState,
    on_edit: impl Fn(GroupId, &mut Window, &mut App) + Clone + 'static,
    on_remove: impl Fn(GroupId, String, &mut Window, &mut App) + Clone + 'static,
    on_update: impl Fn(GroupId, &mut Window, &mut App) + Clone + 'static,
    on_new: impl Fn(&mut Window, &mut App) + 'static,
    on_update_all: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let mut list = div()
        .id("mg-list")
        .flex()
        .flex_col()
        .gap_2()
        .mb_3()
        .min_h(px(280.))
        .max_h(px(360.))
        .overflow_y_scroll();

    for g in state.all_groups() {
        let id = g.id;
        let on_edit = on_edit.clone();
        let on_remove = on_remove.clone();
        let on_update = on_update.clone();
        let name = if g.name.is_empty() {
            format!("Group {id}")
        } else {
            g.name.clone()
        };
        let name_for_remove = name.clone();
        let kind = match (g.archive, g.url.trim().is_empty()) {
            (true, true) => "Archive Basic",
            (true, false) => "Archive Subscription",
            (false, true) => "Basic",
            (false, false) => "Subscription",
        };
        let url = g.url.clone();
        let has_url = !url.trim().is_empty();
        let metadata = group_subscription_metadata(g);
        let count = g.profile_ids.len();

        let mut actions = div().flex().items_center().gap_1().flex_shrink_0();
        if has_url && !g.archive {
            actions = actions.child(secondary_btn(
                SharedString::from(format!("mg-upd-{id}")),
                "Update Subscription",
                move |_, w, cx| on_update(id, w, cx),
            ));
        }
        actions = actions
            .child(secondary_btn(
                SharedString::from(format!("mg-edit-{id}")),
                "Edit",
                move |_, w, cx| on_edit(id, w, cx),
            ))
            .child(secondary_btn(
                SharedString::from(format!("mg-rm-{id}")),
                "Remove",
                move |_, w, cx| on_remove(id, name_for_remove.clone(), w, cx),
            ));

        list = list.child(
            div()
                .id(SharedString::from(format!("mg-{id}")))
                .flex()
                .flex_col()
                .gap_1()
                .px_3()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(Theme::border_light())
                .bg(Theme::bg_app())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(Theme::accent())
                                .child(format!("{kind} ({count})")),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_sm()
                                .text_color(Theme::text())
                                .child(name),
                        )
                        .child(actions),
                )
                .when(has_url, |el| {
                    el.child(
                        div()
                            .text_xs()
                            .text_color(Theme::text_muted())
                            .child(url),
                    )
                })
                .when_some(metadata, |el, text| {
                    el.child(div().text_xs().text_color(Theme::text_muted()).child(text))
                }),
        );
    }

    div()
        .flex()
        .flex_col()
        .w_full()
        .child(list)
        .child(
            div()
                .flex()
                .gap_2()
                .child(primary_btn("mg-add", "New group", move |_, w, cx| on_new(w, cx)))
                .child(secondary_btn(
                    "mg-update-all",
                    "Update all subscriptions",
                    move |_, w, cx| on_update_all(w, cx),
                )),
        )
}

/// Upstream DialogEditGroup body.
pub fn edit_group_body(
    draft: &crate::ui::dialogs::EditGroupView,
    name: &Entity<InputState>,
    url: &Entity<InputState>,
    on_cycle_type: impl Fn(&mut Window, &mut App) + 'static,
    on_cycle_front: impl Fn(&mut Window, &mut App) + 'static,
    on_cycle_landing: impl Fn(&mut Window, &mut App) + 'static,
    on_toggle_auto_clear: impl Fn(&mut Window, &mut App) + 'static,
    on_toggle_skip_auto: impl Fn(&mut Window, &mut App) + 'static,
    on_copy_links: impl Fn(&mut Window, &mut App) + 'static,
    on_copy_deep: impl Fn(&mut Window, &mut App) + 'static,
    on_ok: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let type_label = if draft.is_subscription {
        "Subscription"
    } else {
        "Basic"
    };
    let type_locked = draft.group_id.is_some();
    let show_share = draft.group_id.is_some() && draft.profile_count > 0;

    let mut common = div().flex().flex_col();
    common = common.child(div().mb_2().child(input_field_row("Name", name, 120.)));
    common = common.child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .mb_2()
            .child(
                div()
                    .w(px(120.))
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .child("Type"),
            )
            .child(if type_locked {
                div()
                    .text_sm()
                    .text_color(Theme::text())
                    .child(type_label)
                    .into_any_element()
            } else {
                secondary_btn("eg-type", type_label, move |_, w, cx| on_cycle_type(w, cx))
                    .into_any_element()
            }),
    );
    common = common.child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .mb_2()
            .child(
                div()
                    .w(px(120.))
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .child("Front Proxy"),
            )
            .child(secondary_btn(
                "eg-front",
                draft.front_label.clone(),
                move |_, w, cx| on_cycle_front(w, cx),
            )),
    );
    common = common.child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .mb_2()
            .child(
                div()
                    .w(px(120.))
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .child("Landing Proxy"),
            )
            .child(secondary_btn(
                "eg-land",
                draft.landing_label.clone(),
                move |_, w, cx| on_cycle_landing(w, cx),
            )),
    );
    common = common.child(
        div().mb_1().child(mode_switch(
            "eg-clear",
            "Auto Clear Unavailable Profiles",
            draft.auto_clear_unavailable,
            move |_, w, cx| on_toggle_auto_clear(w, cx),
        )),
    );

    let mut root = div().flex().flex_col().w_full().child(group_panel("Common", common));

    if draft.is_subscription {
        root = root.child(group_panel(
            "Subscription",
            div()
                .flex()
                .flex_col()
                .child(div().mb_2().child(input_field_row("URL", url, 80.)))
                .child(mode_switch(
                    "eg-skip",
                    "Skip automatic update",
                    draft.skip_auto_update,
                    move |_, w, cx| on_toggle_skip_auto(w, cx),
                )),
        ));
    }

    if show_share {
        root = root.child(group_panel(
            "Share",
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(secondary_btn(
                    "eg-copy",
                    "Copy profile share links",
                    move |_, w, cx| on_copy_links(w, cx),
                ))
                .child(secondary_btn(
                    "eg-copy-deep",
                    "Copy profile share links (Deep Links)",
                    move |_, w, cx| on_copy_deep(w, cx),
                )),
        ));
    }

    root.child(dialog_actions(
        "eg-cancel",
        "Cancel",
        "eg-ok",
        "OK",
        on_cancel,
        on_ok,
    ))
}

/// View-model for Edit Group (labels resolved in main_window).
#[derive(Clone)]
pub struct EditGroupView {
    pub group_id: Option<GroupId>,
    pub is_subscription: bool,
    pub skip_auto_update: bool,
    pub auto_clear_unavailable: bool,
    pub front_label: SharedString,
    pub landing_label: SharedString,
    pub profile_count: usize,
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

/// Scrollable diff text for the SubscriptionDiff alert dialog.
/// Footer Close comes from gpui-component [`Dialog::alert`].
pub fn subscription_diff_body(body: &str) -> impl IntoElement {
    div()
        .id("sub-diff-scroll")
        .w_full()
        .max_h(px(360.))
        .overflow_y_scroll()
        .p_2()
        .border_1()
        .border_color(Theme::border_light())
        .rounded_sm()
        .text_sm()
        .text_color(Theme::text())
        .child(body.to_string())
}

pub fn add_input_body(
    text_input: &Entity<InputState>,
    type_hint: &str,
    on_save: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .child(section_hint(
            "New profile — paste share link(s), Clash YAML, or JSON",
        ))
        .child(div().mb_3().min_h(px(160.)).child(input_area(text_input)))
        .child(section_hint(format!("Detected type hint: {type_hint}")))
        .child(dialog_actions(
            "ai-cancel",
            "Cancel",
            "ai-ok",
            "OK",
            on_cancel,
            on_save,
        ))
}

pub fn detect_hint_for_text(text: &str) -> &'static str {
    detect_hint(text)
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
    mtu_input: &Entity<InputState>,
    vpn_strict_route: bool,
    disable_private_range_bypass: bool,
    on_toggle_strict: impl Fn(&mut Window, &mut App) + 'static,
    on_toggle_bypass: impl Fn(&mut Window, &mut App) + 'static,
    on_save: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .child(section_hint(
            "Tun Mode settings — applied on next Start when Tun is checked. \
             macOS/Linux require elevated ThroneCore (setuid). \
             On macOS also set Routing → Local override to a plain DNS IP \
             (same as upstream Throne; empty Local DNS + Tun will fail to start).",
        ))
        .child(input_field_row("MTU", mtu_input, 140.))
        .child(
            div().mb_2().child(mode_switch(
                "tun-strict",
                "Strict route",
                vpn_strict_route,
                move |_, w, cx| on_toggle_strict(w, cx),
            )),
        )
        .child(
            div().mb_3().child(mode_switch(
                "tun-bypass",
                "Bypass private LAN ranges (recommended)",
                !disable_private_range_bypass,
                move |_, w, cx| on_toggle_bypass(w, cx),
            )),
        )
        .child(dialog_actions(
            "tun-cancel",
            "Cancel",
            "tun-save",
            "Save",
            on_cancel,
            on_save,
        ))
}

/// Hotkey field index for capture callbacks (matches `HotkeySettings::focus`).
#[derive(Clone, Copy, Debug)]
pub enum HotkeyField {
    StartStop,
    Import,
    Save,
    UrlTest,
    CopyLogs,
}

pub fn hotkey_settings_body(
    start_stop: &str,
    import: &str,
    save: &str,
    url_test: &str,
    copy_logs: &str,
    on_capture: impl Fn(HotkeyField, String, &mut Window, &mut App) + 'static + Clone,
    on_save: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    fn bind(
        field: HotkeyField,
        on: impl Fn(HotkeyField, String, &mut Window, &mut App) + 'static + Clone,
    ) -> impl Fn(String, &mut Window, &mut App) + 'static {
        move |chord, window, cx| on(field, chord, window, cx)
    }
    div()
        .flex()
        .flex_col()
        .child(section_hint(
            "Click a field, then press the shortcut. Backspace clears. \
             Labels are stored in settings; global rebind lands with hotkey registration.",
        ))
        .child(hotkey_capture_row(
            "hk-start",
            "Start / Stop",
            start_stop,
            150.,
            bind(HotkeyField::StartStop, on_capture.clone()),
        ))
        .child(hotkey_capture_row(
            "hk-import",
            "Import clipboard",
            import,
            150.,
            bind(HotkeyField::Import, on_capture.clone()),
        ))
        .child(hotkey_capture_row(
            "hk-save-db",
            "Save database",
            save,
            150.,
            bind(HotkeyField::Save, on_capture.clone()),
        ))
        .child(hotkey_capture_row(
            "hk-url",
            "URL Test",
            url_test,
            150.,
            bind(HotkeyField::UrlTest, on_capture.clone()),
        ))
        .child(hotkey_capture_row(
            "hk-logs",
            "Copy logs",
            copy_logs,
            150.,
            bind(HotkeyField::CopyLogs, on_capture),
        ))
        .child(dialog_actions(
            "hk-cancel",
            "Cancel",
            "hk-save",
            "Save",
            on_cancel,
            on_save,
        ))
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
