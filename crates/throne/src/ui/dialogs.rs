//! Secondary feature dialogs (settings / groups / add / routing / tun / hotkeys).

use gpui::{App, Entity, SharedString, Window, div, prelude::*, px};
use chrono::{Local, TimeZone};
use gpui_component::{
    input::InputState,
    scroll::ScrollableElement as _,
};

use throne_domain::{AppState, GroupId, ProfileId, RulesetMirror};

use crate::theme::Theme;
use crate::ui::routing::RoutingDraft;
use crate::ui::widgets::{
    dialog_actions, form_enable_interval_row, form_input_row, form_row, group_panel,
    hotkey_capture_row, input_area, input_field_row, mode_switch, primary_btn, secondary_btn,
    section_hint, settings_switch_row, tab_bar,
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

/// Upstream `DialogBasicSettings` tab bar (subset of Qt tabs we currently ship).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BasicSettingsTab {
    #[default]
    Common,
    Subscription,
}

impl BasicSettingsTab {
    pub const ALL: [Self; 2] = [Self::Common, Self::Subscription];

    pub fn label(self) -> &'static str {
        match self {
            Self::Common => "Common",
            Self::Subscription => "Subscription",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&t| t == self).unwrap_or(0)
    }

    pub fn from_index(ix: usize) -> Self {
        Self::ALL.get(ix).copied().unwrap_or_default()
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
        // ── Subscription (upstream DialogBasicSettings Subscription tab) ──
        net_use_proxy: bool,
        allow_stopping_active_profile: bool,
        sub_clear: bool,
        sub_show_change_popup: bool,
        net_insecure: bool,
        sub_send_hwid: bool,
        sub_auto_update_enable: bool,
        route_auto_update_enable: bool,
        /// Active tab (Common / Subscription).
        tab: BasicSettingsTab,
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
    /// Upstream DialogAutoSelector — live view while an Auto Selector is running.
    AutoSelectorStats {
        only_problems: bool,
        /// Highlighted member tag for Pin (empty = none).
        selected_member: String,
        notice: String,
    },
    /// Upstream DialogTrafficStats — historical series + breakdown.
    TrafficStats {
        /// 0=24h, 1=7d, 2=30d, 3=90d
        period: usize,
        /// 0=by profile, 1=by app
        tab: usize,
        summary: String,
        /// Preformatted breakdown lines (top-N).
        breakdown_lines: Vec<String>,
        /// Chart bars: (label, down, up)
        bars: Vec<(String, i64, i64)>,
        notice: String,
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
            net_use_proxy: s.net_use_proxy,
            allow_stopping_active_profile: s.allow_stopping_active_profile,
            sub_clear: s.sub_clear,
            sub_show_change_popup: s.sub_show_change_popup,
            net_insecure: s.net_insecure,
            sub_send_hwid: s.sub_send_hwid,
            sub_auto_update_enable: s.sub_auto_update_enabled(),
            route_auto_update_enable: s.route_auto_update_enabled(),
            tab: BasicSettingsTab::Common,
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

/// Toggle ids for Basic Settings → Subscription checkboxes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BasicSubToggle {
    NetUseProxy,
    AllowStoppingActive,
    SubClear,
    SubShowChangePopup,
    NetInsecure,
    SubSendHwid,
    SubAutoUpdate,
    RouteAutoUpdate,
}

/// Build basic settings body with real gpui-component Inputs.
///
/// Layout mirrors upstream `DialogBasicSettings` `QTabWidget` + form grids:
/// Common (Inbound / Testing group boxes) · Subscription (2-col grid order).
///
/// `content_max_h` is the scrollable form area height (tabs + footer stay fixed).
pub fn basic_settings_body(
    inbound_address: &Entity<InputState>,
    inbound_port: &Entity<InputState>,
    test_url: &Entity<InputState>,
    remote_dns: &Entity<InputState>,
    direct_dns: &Entity<InputState>,
    log_level: &Entity<InputState>,
    user_agent: &Entity<InputState>,
    sub_custom_hwid: &Entity<InputState>,
    sub_auto_minutes: &Entity<InputState>,
    route_auto_minutes: &Entity<InputState>,
    tab: BasicSettingsTab,
    content_max_h: f32,
    ruleset_mirror: RulesetMirror,
    adblock_enable: bool,
    net_use_proxy: bool,
    allow_stopping_active_profile: bool,
    sub_clear: bool,
    sub_show_change_popup: bool,
    net_insecure: bool,
    sub_send_hwid: bool,
    sub_auto_update_enable: bool,
    route_auto_update_enable: bool,
    on_set_tab: impl Fn(BasicSettingsTab, &mut Window, &mut App) + 'static,
    on_cycle_mirror: impl Fn(&mut Window, &mut App) + 'static,
    on_toggle_adblock: impl Fn(&mut Window, &mut App) + 'static,
    on_toggle_sub: impl Fn(BasicSubToggle, &mut Window, &mut App) + Clone + 'static,
    on_save: impl Fn(&mut Window, &mut App) + 'static,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let toggle = |id: &'static str, label: &'static str, checked: bool, kind: BasicSubToggle| {
        let on_toggle_sub = on_toggle_sub.clone();
        settings_switch_row(id, label, checked, move |_, w, cx| {
            on_toggle_sub(kind, w, cx);
        })
    };

    let labels: Vec<SharedString> = BasicSettingsTab::ALL
        .iter()
        .map(|t| SharedString::from(t.label()))
        .collect();
    let tabs = tab_bar("bs-tabs", tab.index(), labels, move |ix, w, cx| {
        on_set_tab(BasicSettingsTab::from_index(*ix), w, cx);
    });

    let content = match tab {
        // Upstream Common: QVBoxLayout → Inbound Settings groupBox + Testing groupBox.
        BasicSettingsTab::Common => {
            let inbound = div()
                .flex()
                .flex_col()
                .w_full()
                .child(section_hint(
                    "Allow LAN: set Listen Address to :: or 0.0.0.0 (or tray toggle), then restart",
                ))
                .child(form_input_row("Listen Address", inbound_address))
                .child(form_input_row("Listen Port", inbound_port));

            let testing = div()
                .flex()
                .flex_col()
                .w_full()
                .child(form_input_row("Latency Test URL", test_url));

            // Extra fields we ship on Common until dedicated Logging / Routing tabs land.
            let extras = div()
                .flex()
                .flex_col()
                .w_full()
                .child(form_input_row("Remote DNS", remote_dns))
                .child(form_input_row("Direct DNS", direct_dns))
                .child(form_input_row("Log level", log_level))
                .child(form_row(
                    "Rule-set mirror",
                    secondary_btn(
                        "bs-mirror",
                        ruleset_mirror.label(),
                        move |_, w, cx| on_cycle_mirror(w, cx),
                    ),
                ))
                .child(settings_switch_row(
                    "bs-adblock",
                    "Adblock rule-set (Start)",
                    adblock_enable,
                    move |_, w, cx| on_toggle_adblock(w, cx),
                ));

            div()
                .flex()
                .flex_col()
                .w_full()
                .child(group_panel("Inbound Settings", inbound))
                .child(group_panel("Testing", testing))
                .child(group_panel("Other", extras))
                .into_any_element()
        }
        // Upstream Subscription tab grid order (dialog_basic_settings.ui tab_3).
        BasicSettingsTab::Subscription => {
            let on_route_auto = on_toggle_sub.clone();
            let on_sub_auto = on_toggle_sub.clone();
            div()
                .flex()
                .flex_col()
                .w_full()
                // row 0 — Routing profiles auto update | Enable + Interval
                .child(form_enable_interval_row(
                    "bs-route-auto",
                    "Routing profiles auto update",
                    route_auto_update_enable,
                    route_auto_minutes,
                    move |_, w, cx| on_route_auto(BasicSubToggle::RouteAutoUpdate, w, cx),
                ))
                // row 1 — Subscription auto update | Enable + Interval
                .child(form_enable_interval_row(
                    "bs-sub-auto",
                    "Subscription auto update",
                    sub_auto_update_enable,
                    sub_auto_minutes,
                    move |_, w, cx| on_sub_auto(BasicSubToggle::SubAutoUpdate, w, cx),
                ))
                // row 2 — User Agent | input
                .child(form_input_row("User Agent", user_agent))
                // row 3 — allow stopping
                .child(toggle(
                    "bs-allow-stop",
                    "Allow stopping the active profile",
                    allow_stopping_active_profile,
                    BasicSubToggle::AllowStoppingActive,
                ))
                // row 4 — clear servers
                .child(toggle(
                    "bs-sub-clear",
                    "Clear servers before updating subscription",
                    sub_clear,
                    BasicSubToggle::SubClear,
                ))
                // row 5 — change popup
                .child(toggle(
                    "bs-sub-diff",
                    "Show the changes window after a manual subscription update",
                    sub_show_change_popup,
                    BasicSubToggle::SubShowChangePopup,
                ))
                // row 8 — HWID
                .child(toggle(
                    "bs-sub-hwid",
                    "Enable sending HWID, device model, and OS version when updating subscription",
                    sub_send_hwid,
                    BasicSubToggle::SubSendHwid,
                ))
                // row 9 — Custom System Parameters | input
                .child(form_input_row(
                    "Custom System Parameters (optional)",
                    sub_custom_hwid,
                ))
                .child(section_hint(
                    "Format: hwid=value,os=value,osVersion=value,model=value · leave empty for defaults",
                ))
                // Upstream places these on the Miscellaneous tab; keep here until that tab lands.
                .child(div().h(px(8.)))
                .child(section_hint("Network (upstream: Miscellaneous)"))
                .child(toggle(
                    "bs-net-proxy",
                    "Use proxy",
                    net_use_proxy,
                    BasicSubToggle::NetUseProxy,
                ))
                .child(toggle(
                    "bs-net-insecure",
                    "Ignore TLS errors",
                    net_insecure,
                    BasicSubToggle::NetInsecure,
                ))
                .into_any_element()
        }
    };

    let scroll_h = content_max_h.max(200.);
    div()
        .flex()
        .flex_col()
        .w_full()
        // Tabs stay pinned above the scroll region.
        .child(div().flex_shrink_0().w_full().child(tabs))
        .child(
            // Fixed height is required for scrolling; without it the dialog grows past
            // the viewport and is clipped with no wheel target. Prefer gpui-component's
            // Scrollable (visible bar + reliable wheel) over bare overflow_y_scroll.
            div()
                .id("bs-scroll")
                .w_full()
                .mt_2()
                .h(px(scroll_h))
                .max_h(px(scroll_h))
                .overflow_y_scrollbar()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        // Extra bottom pad so the last field isn't flush against the footer.
                        .pb_3()
                        .child(content),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .w_full()
                .pt_1()
                .child(dialog_actions(
                    "bs-cancel",
                    "Cancel",
                    "bs-save",
                    "Save",
                    on_cancel,
                    on_save,
                )),
        )
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
///
/// Content scrolls when taller than the dialog; OK/Cancel stay pinned below.
pub fn edit_group_body(
    draft: &crate::ui::dialogs::EditGroupView,
    name: &Entity<InputState>,
    url: &Entity<InputState>,
    content_max_h: f32,
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

    let mut sections = div().flex().flex_col().w_full().child(group_panel("Common", common));

    if draft.is_subscription {
        sections = sections.child(group_panel(
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
        sections = sections.child(group_panel(
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

    // Scrollable body (upstream DialogEditGroup scrolls when Share/Subscription
    // push content past the dialog height); footer actions stay pinned.
    let scroll_h = content_max_h.max(200.);
    div()
        .flex()
        .flex_col()
        .w_full()
        .child(
            div()
                .id("edit-group-scroll")
                .w_full()
                .max_h(px(scroll_h))
                .overflow_y_scroll()
                .child(sections),
        )
        .child(dialog_actions(
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

/// One row in the Auto Selector stats table (core `AutoSelectorMember` subset).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoSelectorMemberRow {
    pub tag: String,
    pub display_name: String,
    pub rank: i32,
    pub state: String,
    pub selected: bool,
    pub qualified: bool,
    pub active: bool,
    pub average_ms: i32,
    pub failures: i32,
    pub last_error: String,
}

/// Snapshot of one running auto-selector group for the stats dialog.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoSelectorGroupView {
    pub tag: String,
    pub phase: String,
    pub selected: String,
    pub pinned: String,
    pub balance: bool,
    pub balance_mode: String,
    pub suspended: bool,
    pub members_total: i32,
    pub members_alive: i32,
    pub members_qualified: i32,
    pub last_switch_reason: String,
    pub members: Vec<AutoSelectorMemberRow>,
}

/// Live Auto Selector monitor (upstream `DialogAutoSelector`).
pub fn auto_selector_stats_body(
    groups: &[AutoSelectorGroupView],
    only_problems: bool,
    selected_member: &str,
    notice: &str,
    on_toggle_problems: impl Fn(&mut Window, &mut App) + 'static + Clone,
    on_select_member: impl Fn(String, &mut Window, &mut App) + 'static + Clone,
    on_recheck: impl Fn(&mut Window, &mut App) + 'static,
    on_pin: impl Fn(&mut Window, &mut App) + 'static,
    on_release: impl Fn(&mut Window, &mut App) + 'static,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    // Callbacks below adapt to Button/Switch's ClickEvent signature.
    let group = groups.first();
    let headline = match group {
        Some(g) if g.suspended => format!("Suspended · local network down · {}", g.tag),
        Some(g) => {
            let sel = if g.selected.is_empty() {
                "—".into()
            } else {
                g.selected.clone()
            };
            format!(
                "{} · phase {} · selected {sel}",
                if g.tag.is_empty() { "proxy" } else { &g.tag },
                if g.phase.is_empty() {
                    "—"
                } else {
                    &g.phase
                }
            )
        }
        None => "No auto-selector is running".into(),
    };
    let detail = match group {
        Some(g) => {
            let pin = if g.pinned.is_empty() {
                "automatic".into()
            } else {
                format!("pinned {}", g.pinned)
            };
            let bal = if g.balance {
                format!(" · balance {}", g.balance_mode)
            } else {
                String::new()
            };
            let reason = if g.last_switch_reason.is_empty() {
                String::new()
            } else {
                format!(" · last switch: {}", g.last_switch_reason)
            };
            format!(
                "members {} total · {} alive · {} qualified · {pin}{bal}{reason}",
                g.members_total, g.members_alive, g.members_qualified
            )
        }
        None => {
            "Start an Auto Selector profile, then open this window again.".into()
        }
    };

    let mut rows: Vec<AutoSelectorMemberRow> = groups
        .iter()
        .flat_map(|g| g.members.iter().cloned())
        .collect();
    if only_problems {
        rows.retain(|m| {
            m.state != "ok"
                || m.failures > 0
                || !m.last_error.is_empty()
                || m.state == "dead"
                || m.state == "degraded"
                || m.state == "cooldown"
        });
    }
    rows.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then_with(|| a.average_ms.cmp(&b.average_ms))
            .then_with(|| a.tag.cmp(&b.tag))
    });

    let mut table = div()
        .id("as-table")
        .w_full()
        .max_h(px(320.))
        .overflow_y_scroll()
        .border_1()
        .border_color(Theme::border_light())
        .rounded_sm()
        .child(
            div()
                .flex()
                .px_2()
                .py_1()
                .bg(Theme::bg_app())
                .text_xs()
                .text_color(Theme::text_muted())
                .child(div().w(px(36.)).child("#"))
                .child(div().w(px(72.)).child("State"))
                .child(div().w(px(64.)).child("Avg"))
                .child(div().w(px(40.)).child("Fail"))
                .child(div().flex_1().child("Member"))
                .child(div().w(px(48.)).child("Flags")),
        );

    if rows.is_empty() {
        table = table.child(
            div()
                .p_3()
                .text_sm()
                .text_color(Theme::text_muted())
                .child(if groups.is_empty() {
                    "Waiting for core snapshot…"
                } else if only_problems {
                    "No problem members right now."
                } else {
                    "No members reported."
                }),
        );
    } else {
        for (i, m) in rows.into_iter().enumerate() {
            let tag = m.tag.clone();
            let is_sel = selected_member == m.tag;
            let name = if m.display_name.is_empty() {
                m.tag.clone()
            } else {
                m.display_name.clone()
            };
            let avg = if m.average_ms > 0 {
                format!("{} ms", m.average_ms)
            } else {
                "—".into()
            };
            let flags = {
                let mut f = Vec::new();
                if m.selected {
                    f.push("sel");
                }
                if m.qualified {
                    f.push("ok");
                }
                if m.active {
                    f.push("act");
                }
                if f.is_empty() {
                    "—".into()
                } else {
                    f.join(" ")
                }
            };
            let err = if m.last_error.is_empty() {
                String::new()
            } else {
                format!(" · {}", m.last_error)
            };
            let on_row = on_select_member.clone();
            let bg = if is_sel {
                Theme::bg_selected()
            } else if i % 2 == 1 {
                Theme::bg_app()
            } else {
                Theme::bg_elevated()
            };
            let fg = if is_sel {
                Theme::text_on_selected()
            } else {
                Theme::text()
            };
            table = table.child(
                div()
                    .flex()
                    .items_center()
                    .px_2()
                    .py_1()
                    .bg(bg)
                    .text_sm()
                    .text_color(fg)
                    .cursor_pointer()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        move |_, window, cx| {
                            on_row(tag.clone(), window, cx);
                        },
                    )
                    .child(div().w(px(36.)).child(format!("{}", m.rank.max(0))))
                    .child(div().w(px(72.)).child(m.state.clone()))
                    .child(div().w(px(64.)).child(avg))
                    .child(div().w(px(40.)).child(format!("{}", m.failures)))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(format!("{name}{err}")),
                    )
                    .child(div().w(px(48.)).text_xs().child(flags)),
            );
        }
    }

    let notice = notice.to_string();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .w_full()
        .child(section_hint(&headline))
        .child(
            div()
                .text_xs()
                .text_color(Theme::text_muted())
                .child(detail),
        )
        .child(mode_switch(
            "as-only-problems",
            "Only problems",
            only_problems,
            move |_, window, cx| on_toggle_problems(window, cx),
        ))
        .child(table)
        .child(
            div()
                .text_xs()
                .text_color(Theme::text_muted())
                .child(if notice.is_empty() {
                    "Click a row, then Pin. Recheck forces a full sweep.".into()
                } else {
                    notice
                }),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .mt_2()
                .child(secondary_btn("as-recheck", "Recheck", move |_, w, cx| {
                    on_recheck(w, cx)
                }))
                .child(secondary_btn("as-pin", "Pin", move |_, w, cx| on_pin(w, cx)))
                .child(secondary_btn("as-release", "Release pin", move |_, w, cx| {
                    on_release(w, cx)
                }))
                .child(primary_btn("as-close", "Close", move |_, w, cx| {
                    on_close(w, cx)
                })),
        )
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

/// Historical Traffic Stats dashboard body (upstream DialogTrafficStats).
pub fn traffic_stats_body(
    period: usize,
    tab: usize,
    summary: &str,
    breakdown_lines: &[String],
    bars: &[(String, i64, i64)],
    notice: &str,
    on_period: impl Fn(usize, &mut Window, &mut App) + 'static + Clone,
    on_tab: impl Fn(usize, &mut Window, &mut App) + 'static + Clone,
    on_refresh: impl Fn(&mut Window, &mut App) + 'static,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    use crate::theme::Theme;
    use gpui::{PathBuilder, canvas, point, px};

    let period_labels = ["24 hours", "7 days", "30 days", "90 days"];
    let bars = bars.to_vec();
    let max_total = bars
        .iter()
        .map(|(_, d, u)| d.saturating_add(*u))
        .max()
        .unwrap_or(0)
        .max(1) as f32;

    div()
        .flex()
        .flex_col()
        .gap_2()
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .children(period_labels.iter().enumerate().map(|(i, label)| {
                    let on = on_period.clone();
                    let selected = period == i;
                    div()
                        .id(SharedString::from(format!("ts-period-{i}")))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_xs()
                        .cursor_pointer()
                        .bg(if selected {
                            Theme::accent_soft()
                        } else {
                            Theme::bg_elevated()
                        })
                        .text_color(if selected {
                            Theme::accent()
                        } else {
                            Theme::text()
                        })
                        .border_1()
                        .border_color(if selected {
                            Theme::accent()
                        } else {
                            Theme::border_light()
                        })
                        .child(*label)
                        .on_click(move |_, w, cx| on(i, w, cx))
                }))
                .child(div().flex_1())
                .child({
                    let on = on_refresh;
                    div()
                        .id("ts-refresh")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_xs()
                        .cursor_pointer()
                        .border_1()
                        .border_color(Theme::border_light())
                        .child("Refresh")
                        .on_click(move |_, w, cx| on(w, cx))
                }),
        )
        .child(
            div()
                .flex()
                .gap_2()
                .children(["By profile", "By app"].iter().enumerate().map(|(i, label)| {
                    let on = on_tab.clone();
                    let selected = tab == i;
                    div()
                        .id(SharedString::from(format!("ts-tab-{i}")))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_xs()
                        .cursor_pointer()
                        .bg(if selected {
                            Theme::bg_selected()
                        } else {
                            Theme::bg_elevated()
                        })
                        .text_color(if selected {
                            Theme::text_on_selected()
                        } else {
                            Theme::text()
                        })
                        .child(*label)
                        .on_click(move |_, w, cx| on(i, w, cx))
                })),
        )
        .child(
            div()
                .h(px(140.))
                .w_full()
                .border_1()
                .border_color(Theme::border_light())
                .bg(Theme::bg_elevated())
                .child(
                    canvas(
                        move |_, _, _| {},
                        move |bounds, _, window, _| {
                            if bars.is_empty() {
                                return;
                            }
                            let n = bars.len();
                            let w = f32::from(bounds.size.width).max(1.0);
                            let h = f32::from(bounds.size.height).max(1.0);
                            let pad = 8.0_f32;
                            let plot_w = (w - pad * 2.0).max(1.0);
                            let plot_h = (h - pad * 2.0).max(1.0);
                            let bar_w = (plot_w / n as f32) * 0.7;
                            let gap = (plot_w / n as f32) * 0.3;
                            for (i, (_, down, up)) in bars.iter().enumerate() {
                                let total = down.saturating_add(*up) as f32;
                                let bh = (total / max_total) * plot_h;
                                let x = bounds.origin.x + px(pad + i as f32 * (bar_w + gap));
                                let y = bounds.origin.y + px(pad + plot_h - bh);
                                // Stacked: down (bottom, blue) + up (top, green)
                                let down_h = if total > 0.0 {
                                    (*down as f32 / total) * bh
                                } else {
                                    0.0
                                };
                                let up_h = bh - down_h;
                                // Down bar
                                if down_h > 0.5 {
                                    let mut b = PathBuilder::fill();
                                    let y0 = y + px(up_h);
                                    b.move_to(point(x, y0));
                                    b.line_to(point(x + px(bar_w), y0));
                                    b.line_to(point(x + px(bar_w), y0 + px(down_h)));
                                    b.line_to(point(x, y0 + px(down_h)));
                                    b.close();
                                    if let Ok(path) = b.build() {
                                        window.paint_path(path, gpui::rgb(0x3299ff));
                                    }
                                }
                                // Up bar
                                if up_h > 0.5 {
                                    let mut b = PathBuilder::fill();
                                    b.move_to(point(x, y));
                                    b.line_to(point(x + px(bar_w), y));
                                    b.line_to(point(x + px(bar_w), y + px(up_h)));
                                    b.line_to(point(x, y + px(up_h)));
                                    b.close();
                                    if let Ok(path) = b.build() {
                                        window.paint_path(path, gpui::rgb(0x86c43f));
                                    }
                                }
                            }
                        },
                    )
                    .size_full(),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(Theme::text())
                .child(summary.to_string()),
        )
        .when(!notice.is_empty(), |el| {
            el.child(
                div()
                    .text_xs()
                    .text_color(Theme::warning())
                    .child(notice.to_string()),
            )
        })
        .child({
            let mut list = div()
                .id("ts-breakdown")
                .flex()
                .flex_col()
                .gap_1()
                .max_h(px(180.))
                .overflow_y_scroll();
            if breakdown_lines.is_empty() {
                list = list.child(
                    div()
                        .text_xs()
                        .text_color(Theme::text_muted())
                        .child("No traffic recorded in this period yet.".to_string()),
                );
            } else {
                for line in breakdown_lines {
                    list = list.child(
                        div()
                            .text_xs()
                            .text_color(Theme::text())
                            .child(line.clone()),
                    );
                }
            }
            list
        })
        .child(
            div()
                .flex()
                .justify_end()
                .child({
                    let on = on_close;
                    div()
                        .id("ts-close")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .text_xs()
                        .cursor_pointer()
                        .bg(Theme::accent())
                        .text_color(Theme::text_on_selected())
                        .child("Close")
                        .on_click(move |_, w, cx| on(w, cx))
                }),
        )
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
