//! Routing Settings dialog — parity with upstream `DialogManageRoutes` +
//! `RouteItem` / `RawRouteItem`.
//!
//! Tabs: Common · Hijack · Warp · DNS · Route  
//! Route actions: New (structured/raw/remote) · Clone · Export · Import · Edit ·
//! Delete · Update. Draft is committed only on OK.

use std::rc::Rc;

use gpui::{div, prelude::*, px, AnyElement, App, SharedString, Window};

use throne_domain::{
    AppSettings, AppState, DefaultOutbound, RouteProfile, RouteRule, SimpleAction,
};

use crate::theme::Theme;
use crate::ui::dialog_inputs::{NestedInputs, RoutingInputs};
use crate::ui::widgets::{
    editor_area, editor_area_tall, focus_field, focus_text_area, group_panel, input_area,
    input_area_tall, input_field_row, input_inline, mode_switch, notice_banner, primary_btn,
    secondary_btn, section_hint, tab_bar,
};

// ── Public draft types ──────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RoutingTab {
    Common,
    Hijack,
    Warp,
    Dns,
    #[default]
    Route,
}

impl RoutingTab {
    pub const ALL: [Self; 5] = [
        Self::Common,
        Self::Hijack,
        Self::Warp,
        Self::Dns,
        Self::Route,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Common => "Common",
            Self::Hijack => "Hijack",
            Self::Warp => "Warp",
            Self::Dns => "DNS",
            Self::Route => "Route",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RtFocus {
    #[default]
    None,
    RemoteDns,
    DirectDns,
    LocalOverride,
    CacheCap,
    DnsObject,
    DnsRules,
    DnsV4,
    DnsV6,
    DnsPort,
    RedirectAddr,
    RedirectPort,
    WarpEp,
    WarpPriv,
    WarpPub,
    WarpAddrs,
    WarpReserved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RouteEditorTab {
    #[default]
    Basic,
    Advanced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReFocus {
    #[default]
    Name,
    RemoteUrl,
    SimpleDirect,
    SimpleProxy,
    SimpleBlock,
    SimpleWarp,
    /// Advanced rule list selection (fake-caret fallback when Inputs are absent).
    RuleName,
    RawJson,
}

#[derive(Clone, Debug)]
pub struct RouteEditorDraft {
    pub edit_idx: Option<usize>,
    pub profile: RouteProfile,
    pub tab: RouteEditorTab,
    pub simple_direct: String,
    pub simple_proxy: String,
    pub simple_block: String,
    pub simple_warp: String,
    pub focus: ReFocus,
    pub selected_rule: Option<usize>,
}

impl RouteEditorDraft {
    pub fn from_profile(edit_idx: Option<usize>, mut profile: RouteProfile) -> Self {
        profile.ensure_default_dns_hijack();
        Self {
            edit_idx,
            simple_direct: profile.simple_rules_text(SimpleAction::Bypass),
            simple_proxy: profile.simple_rules_text(SimpleAction::Proxy),
            simple_block: profile.simple_rules_text(SimpleAction::Block),
            simple_warp: profile.simple_rules_text(SimpleAction::WarpBypass),
            profile,
            tab: RouteEditorTab::Basic,
            focus: ReFocus::Name,
            selected_rule: None,
        }
    }

    pub fn apply_simple_to_profile(&mut self) -> String {
        let mut err = String::new();
        err += &self
            .profile
            .update_simple_rules(&self.simple_direct, SimpleAction::Bypass);
        err += &self
            .profile
            .update_simple_rules(&self.simple_proxy, SimpleAction::Proxy);
        err += &self
            .profile
            .update_simple_rules(&self.simple_block, SimpleAction::Block);
        err += &self
            .profile
            .update_simple_rules(&self.simple_warp, SimpleAction::WarpBypass);
        err
    }

    pub fn reload_simple_from_profile(&mut self) {
        self.simple_direct = self.profile.simple_rules_text(SimpleAction::Bypass);
        self.simple_proxy = self.profile.simple_rules_text(SimpleAction::Proxy);
        self.simple_block = self.profile.simple_rules_text(SimpleAction::Block);
        self.simple_warp = self.profile.simple_rules_text(SimpleAction::WarpBypass);
    }
}

#[derive(Clone, Debug)]
pub struct RawEditorDraft {
    pub edit_idx: Option<usize>,
    pub name: String,
    pub raw_route: String,
    pub prevent_modifications: bool,
    pub focus: ReFocus,
}

impl RawEditorDraft {
    pub fn from_profile(edit_idx: Option<usize>, p: &RouteProfile) -> Self {
        Self {
            edit_idx,
            name: p.name.clone(),
            raw_route: if p.raw_route.trim().is_empty() {
                "{\n  \"rules\": []\n}".into()
            } else {
                p.raw_route.clone()
            },
            prevent_modifications: p.prevent_modifications,
            focus: ReFocus::Name,
        }
    }

    pub fn into_profile(self, id: i64) -> RouteProfile {
        let mut p = RouteProfile::new(id, self.name);
        p.is_raw = true;
        p.raw_route = self.raw_route;
        p.prevent_modifications = self.prevent_modifications;
        p
    }
}

#[derive(Clone, Debug, Default)]
pub enum RoutingNested {
    #[default]
    None,
    NewMenu,
    UpdateMenu {
        sel_is_remote: bool,
    },
    ImportPaste {
        text: String,
    },
    RouteEditor(RouteEditorDraft),
    RawEditor(RawEditorDraft),
    Notice {
        title: String,
        body: String,
    },
}

#[derive(Clone, Debug)]
pub struct RoutingDraft {
    pub tab: RoutingTab,
    pub routes: Vec<RouteProfile>,
    pub selected_idx: usize,
    pub active_id: i64,
    pub settings: AppSettings,
    pub focus: RtFocus,
    pub nested: RoutingNested,
    pub notice: String,
}

impl RoutingDraft {
    pub fn from_state(state: &AppState) -> Self {
        let routes: Vec<RouteProfile> = state.all_routes().into_iter().cloned().collect();
        let active_id = state
            .active_route()
            .map(|r| r.id)
            .or_else(|| routes.first().map(|r| r.id))
            .unwrap_or(-1);
        let selected_idx = routes.iter().position(|r| r.id == active_id).unwrap_or(0);
        Self {
            tab: RoutingTab::Route,
            routes,
            selected_idx,
            active_id,
            settings: state.settings().clone(),
            focus: RtFocus::None,
            nested: RoutingNested::None,
            notice: String::new(),
        }
    }

    pub fn selected(&self) -> Option<&RouteProfile> {
        self.routes.get(self.selected_idx)
    }

    pub fn validate_dns_rules(raw: &str) -> bool {
        for rule in raw.lines() {
            let t = rule.trim();
            if t.is_empty() {
                continue;
            }
            if !(t.starts_with("ruleset:")
                || t.starts_with("domain:")
                || t.starts_with("suffix:")
                || t.starts_with("regex:"))
            {
                return false;
            }
        }
        true
    }

    pub fn dns_rules_text(&self) -> String {
        self.settings.dns_server_rules.join("\n")
    }

    pub fn set_dns_rules_from_text(&mut self, text: &str) {
        self.settings.dns_server_rules = text
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
    }

    /// Apply keystroke into the focused field. `text` is the typed string
    /// (from GPUI `key_char` or a single-key fallback). Returns true if handled.
    pub fn handle_key(&mut self, text: Option<&str>, is_back: bool) -> bool {
        // Nested editors first
        match &mut self.nested {
            RoutingNested::ImportPaste { text: buf } => {
                edit_str(buf, text, is_back, true);
                return true;
            }
            RoutingNested::RouteEditor(ed) => {
                let field: &mut String = match ed.focus {
                    ReFocus::Name => &mut ed.profile.name,
                    ReFocus::RemoteUrl => &mut ed.profile.remote_url,
                    ReFocus::SimpleDirect => &mut ed.simple_direct,
                    ReFocus::SimpleProxy => &mut ed.simple_proxy,
                    ReFocus::SimpleBlock => &mut ed.simple_block,
                    ReFocus::SimpleWarp => &mut ed.simple_warp,
                    ReFocus::RuleName => {
                        if let Some(i) = ed.selected_rule {
                            if let Some(r) = ed.profile.rules.get_mut(i) {
                                edit_str(&mut r.name, text, is_back, false);
                            }
                        }
                        return true;
                    }
                    // Advanced multi-line attrs and raw JSON use real Inputs when present.
                    ReFocus::RawJson => return true,
                };
                let allow_nl = matches!(
                    ed.focus,
                    ReFocus::SimpleDirect
                        | ReFocus::SimpleProxy
                        | ReFocus::SimpleBlock
                        | ReFocus::SimpleWarp
                );
                edit_str(field, text, is_back, allow_nl);
                return true;
            }
            RoutingNested::RawEditor(ed) => {
                let field = match ed.focus {
                    ReFocus::Name => &mut ed.name,
                    _ => &mut ed.raw_route,
                };
                let allow_nl = !matches!(ed.focus, ReFocus::Name);
                edit_str(field, text, is_back, allow_nl);
                return true;
            }
            RoutingNested::NewMenu
            | RoutingNested::UpdateMenu { .. }
            | RoutingNested::Notice { .. }
            | RoutingNested::None => {}
        }

        // No field focused: auto-focus the primary editable of the active tab
        // (matches Qt: typing into a dialog with a default focus widget).
        if self.focus == RtFocus::None && (is_back || text.is_some()) {
            self.focus = match self.tab {
                RoutingTab::Dns => RtFocus::RemoteDns,
                RoutingTab::Warp => RtFocus::WarpEp,
                RoutingTab::Hijack => RtFocus::DnsPort,
                RoutingTab::Common | RoutingTab::Route => RtFocus::None,
            };
            if self.focus == RtFocus::None {
                return true;
            }
        }

        let s = &mut self.settings;
        let field: Option<(&mut String, bool)> = match self.focus {
            RtFocus::RemoteDns => Some((&mut s.remote_dns, false)),
            RtFocus::DirectDns => Some((&mut s.direct_dns, false)),
            RtFocus::LocalOverride => Some((&mut s.core_box_underlying_dns, false)),
            RtFocus::CacheCap => {
                return edit_i32_field(&mut s.dns_cache_capacity, text, is_back);
            }
            RtFocus::DnsObject => Some((&mut s.dns_object, true)),
            RtFocus::DnsV4 => Some((&mut s.dns_v4_resp, false)),
            RtFocus::DnsV6 => Some((&mut s.dns_v6_resp, false)),
            RtFocus::DnsPort => {
                return edit_i32_field(&mut s.dns_server_listen_port, text, is_back);
            }
            RtFocus::RedirectAddr => Some((&mut s.redirect_listen_address, false)),
            RtFocus::RedirectPort => {
                return edit_i32_field(&mut s.redirect_listen_port, text, is_back);
            }
            RtFocus::WarpEp => Some((&mut s.warp_ep, false)),
            RtFocus::WarpPriv => Some((&mut s.warp_private_key, false)),
            RtFocus::WarpPub => Some((&mut s.warp_public_key, false)),
            RtFocus::WarpAddrs => {
                let mut joined = s.warp_ifc_addrs.join(",");
                edit_str(&mut joined, text, is_back, false);
                s.warp_ifc_addrs = split_csv(&joined);
                return true;
            }
            RtFocus::WarpReserved => {
                let mut joined = s.warp_reserved.join(",");
                edit_str(&mut joined, text, is_back, false);
                s.warp_reserved = split_csv(&joined);
                return true;
            }
            RtFocus::DnsRules => {
                let mut t = self.dns_rules_text();
                edit_str(&mut t, text, is_back, true);
                self.set_dns_rules_from_text(&t);
                return true;
            }
            RtFocus::None => None,
        };
        if let Some((f, nl)) = field {
            edit_str(f, text, is_back, nl);
            return true;
        }
        // swallow keys while dialog open
        true
    }
}

fn edit_str(s: &mut String, text: Option<&str>, is_back: bool, allow_nl: bool) {
    if is_back {
        s.pop();
        return;
    }
    let Some(t) = text else {
        return;
    };
    for c in t.chars() {
        if c == '\n' || c == '\r' {
            if allow_nl {
                s.push('\n');
            }
            continue;
        }
        if !c.is_control() {
            s.push(c);
        }
    }
}

fn edit_i32_field(n: &mut i32, text: Option<&str>, is_back: bool) -> bool {
    let mut s = n.to_string();
    if is_back {
        s.pop();
    } else if let Some(t) = text {
        for c in t.chars() {
            if c.is_ascii_digit() {
                s.push(c);
            }
        }
    }
    *n = s.parse().unwrap_or(0);
    true
}

fn split_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

// ── Cycle helpers ───────────────────────────────────────────────────────────

const STRATEGIES: &[&str] = &["", "prefer_ipv4", "prefer_ipv6", "ipv4_only", "ipv6_only"];

/// Upstream remote_dns editable combo presets (dialog_manage_routes.ui).
const REMOTE_DNS_PRESETS: &[&str] = &[
    "tls://8.8.8.8",
    "tls://1.1.1.1",
    "8.8.8.8",
    "1.1.1.1",
    "https://dns.google/dns-query",
];

/// Upstream direct_dns editable combo presets.
const DIRECT_DNS_PRESETS: &[&str] = &[
    "localhost",
    "223.5.5.5",
    "119.29.29.29",
    "178.22.122.100",
    "77.88.8.8",
];

pub fn cycle_strategy(cur: &str) -> String {
    let i = STRATEGIES.iter().position(|s| *s == cur).unwrap_or(0);
    STRATEGIES[(i + 1) % STRATEGIES.len()].to_string()
}

fn cycle_preset(cur: &str, presets: &[&str]) -> String {
    let i = presets
        .iter()
        .position(|s| *s == cur)
        .unwrap_or(presets.len().wrapping_sub(1));
    presets[(i + 1) % presets.len()].to_string()
}

fn cycle_dns_final(cur: &str) -> String {
    if cur.eq_ignore_ascii_case("direct") {
        "remote".into()
    } else {
        "direct".into()
    }
}

fn strategy_label(s: &str) -> String {
    if s.is_empty() {
        "(default)".into()
    } else {
        s.into()
    }
}

// ── UI builders ─────────────────────────────────────────────────────────────

type ClickFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// Real Input + optional ▼ preset cycle (DNS remote/direct rows).
fn dns_input_with_preset(
    state: &gpui::Entity<gpui_component::input::InputState>,
    on_preset: ClickFn,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .flex_1()
        .child(input_inline(state))
        .child(secondary_btn("dns-preset", "▼", move |_, w, cx| {
            cx.stop_propagation();
            on_preset(w, cx);
        }))
}

/// Labeled line-edit (upstream QLineEdit) — gpui-component-styled focus field.
fn field_row(
    id: impl Into<SharedString>,
    label: &'static str,
    value: String,
    focused: bool,
    on_focus: ClickFn,
) -> impl IntoElement {
    let id: SharedString = id.into();
    div()
        .id(SharedString::from(format!("{id}-row")))
        .flex()
        .items_center()
        .gap_2()
        .mb_1p5()
        .child(
            div()
                .w(px(120.))
                .text_xs()
                .text_color(Theme::text_muted())
                .child(label),
        )
        .child(line_edit(id, value, focused, None, on_focus, None))
}

/// Upstream editable QComboBox / QLineEdit chrome.
/// Click to focus + type; optional ▼ cycles presets.
fn line_edit(
    id: impl Into<SharedString>,
    value: String,
    focused: bool,
    placeholder: Option<&str>,
    on_focus: ClickFn,
    on_preset: Option<ClickFn>,
) -> impl IntoElement {
    let id: SharedString = id.into();
    let preset_id = SharedString::from(format!("{id}-preset"));
    let mut row = div().id(id.clone()).flex().items_center().gap_1().flex_1();

    row = row.child(focus_field(
        SharedString::from(format!("{id}-box")),
        value,
        focused,
        placeholder,
        move |w, cx| on_focus(w, cx),
    ));

    if let Some(on_preset) = on_preset {
        row = row.child(secondary_btn(preset_id, "▼", move |_, w, cx| {
            cx.stop_propagation();
            on_preset(w, cx);
        }));
    }
    row
}

fn multi_field(
    id: impl Into<SharedString>,
    label: &'static str,
    value: &str,
    focused: bool,
    h: f32,
    on_focus: ClickFn,
) -> impl IntoElement {
    let id: SharedString = id.into();
    div()
        .id(id.clone())
        .flex()
        .flex_col()
        .mb_2()
        .child(section_hint(label))
        .child(focus_text_area(
            SharedString::from(format!("{id}-body")),
            value,
            focused,
            h,
            move |w, cx| on_focus(w, cx),
        ))
}

fn cycle_btn(id: impl Into<SharedString>, label: String, on_click: ClickFn) -> impl IntoElement {
    secondary_btn(id, label, move |_, w, cx| on_click(w, cx))
}

/// Main Routes dialog body (no nested editors — those are separate open_dialog layers).
pub fn routing_settings_view(
    draft: &RoutingDraft,
    inputs: Option<&RoutingInputs>,
    on_event: impl Fn(RoutingEvent, &mut Window, &mut App) + 'static + Clone,
) -> impl IntoElement {
    let on_event = Rc::new(on_event);
    build_main(draft, inputs, on_event)
}

/// Title for the independent nested routing dialog (if any).
pub fn routing_nested_title(draft: &RoutingDraft) -> Option<&'static str> {
    match &draft.nested {
        RoutingNested::None => None,
        RoutingNested::NewMenu => Some("New route profile"),
        RoutingNested::UpdateMenu { .. } => Some("Update remote profiles"),
        RoutingNested::ImportPaste { .. } => Some("Import routing profile"),
        RoutingNested::Notice { title, .. } => {
            // Leak-free: title is dynamic — caller uses owned string via nested_notice_title.
            let _ = title;
            Some("Notice")
        }
        RoutingNested::RouteEditor(_) => Some("Route Profile"),
        RoutingNested::RawEditor(_) => Some("Raw Route Profile"),
    }
}

/// Owned title when nested is a Notice (dynamic string).
pub fn routing_nested_title_owned(draft: &RoutingDraft) -> Option<String> {
    match &draft.nested {
        RoutingNested::Notice { title, .. } => Some(title.clone()),
        _ => routing_nested_title(draft).map(|s| s.to_string()),
    }
}

/// Preferred width for the nested routing dialog layer.
pub fn routing_nested_width(draft: &RoutingDraft) -> f32 {
    match &draft.nested {
        RoutingNested::RouteEditor(_) | RoutingNested::RawEditor(_) => 720.,
        RoutingNested::ImportPaste { .. } => 560.,
        RoutingNested::NewMenu | RoutingNested::UpdateMenu { .. } => 360.,
        RoutingNested::Notice { .. } => 420.,
        RoutingNested::None => 400.,
    }
}

/// Nested routing UI body for a stacked `open_dialog` layer (no local modal chrome).
pub fn routing_nested_view(
    draft: &RoutingDraft,
    inputs: Option<&RoutingInputs>,
    on_event: impl Fn(RoutingEvent, &mut Window, &mut App) + 'static + Clone,
) -> Option<AnyElement> {
    let on_event = Rc::new(on_event);
    build_nested(draft, inputs.and_then(|i| i.nested.as_ref()), on_event)
        .map(|el| el.into_any_element())
}

#[derive(Clone, Debug)]
pub enum RoutingEvent {
    Close,
    Ok,
    SetTab(RoutingTab),
    SetFocus(RtFocus),
    Toggle(&'static str),
    Cycle(&'static str),
    SelectRoute(usize),
    SelectActive(i64),
    /// Route list actions: new-menu, clone, export, import, edit, delete, update-menu, …
    Action(&'static str),
    NestedClose,
    NestedAction(&'static str),
    ReFocus(ReFocus),
    ReTab(RouteEditorTab),
    ReSelectRule(usize),
    ReCycleDefOut,
    ReCycleRuleOut,
    ReToggle(&'static str),
}

fn emit(on_event: &Rc<dyn Fn(RoutingEvent, &mut Window, &mut App)>, ev: RoutingEvent) -> ClickFn {
    let on_event = on_event.clone();
    Rc::new(move |w, cx| on_event(ev.clone(), w, cx))
}

fn build_main(
    draft: &RoutingDraft,
    inputs: Option<&RoutingInputs>,
    on_event: Rc<dyn Fn(RoutingEvent, &mut Window, &mut App)>,
) -> impl IntoElement {
    let selected_index = RoutingTab::ALL
        .iter()
        .position(|&t| t == draft.tab)
        .unwrap_or(0);
    let labels: Vec<SharedString> = RoutingTab::ALL
        .iter()
        .map(|t| SharedString::from(t.label()))
        .collect();
    let on_tabs = on_event.clone();
    let tabs = tab_bar("rt-tabs", selected_index, labels, move |ix, w, cx| {
        if let Some(&t) = RoutingTab::ALL.get(*ix) {
            on_tabs(RoutingEvent::SetTab(t), w, cx);
        }
    });

    let content = match draft.tab {
        RoutingTab::Common => tab_common(draft, &on_event).into_any_element(),
        RoutingTab::Hijack => tab_hijack(draft, inputs, &on_event).into_any_element(),
        RoutingTab::Warp => tab_warp(draft, inputs, &on_event).into_any_element(),
        RoutingTab::Dns => tab_dns(draft, inputs, &on_event).into_any_element(),
        RoutingTab::Route => tab_route(draft, &on_event).into_any_element(),
    };

    let on_ok = emit(&on_event, RoutingEvent::Ok);
    let on_cancel = emit(&on_event, RoutingEvent::Close);
    let notice = draft.notice.clone();

    // Body only — chrome from open_dialog. Sizing tracks upstream 800×600 dialog.
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_h(px(500.))
        .child(div().mb_2().child(tabs))
        .child(
            div()
                .id("rt-tab-body")
                .flex_1()
                .min_h(px(380.))
                .max_h(px(460.))
                .overflow_y_scroll()
                .child(content),
        )
        .when(!notice.is_empty(), |d| {
            d.child(div().mt_1().child(notice_banner("rt-notice", notice)))
        })
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .mt_3()
                .pt_2()
                .border_t_1()
                .border_color(Theme::border_light())
                .child(secondary_btn("rt-cancel", "Cancel", {
                    let on_cancel = on_cancel.clone();
                    move |_, w, cx| on_cancel(w, cx)
                }))
                .child(primary_btn("rt-ok", "OK", move |_, w, cx| on_ok(w, cx))),
        )
}

fn tab_common(
    draft: &RoutingDraft,
    on_event: &Rc<dyn Fn(RoutingEvent, &mut Window, &mut App)>,
) -> impl IntoElement {
    let s = &draft.settings;
    let f = draft.focus;

    // Active route combo as clickable list of names
    let mut route_combo = div().id("rt-active-list").flex().flex_col().gap_1().mb_3();
    for r in &draft.routes {
        let id = r.id;
        let sel = draft.active_id == id;
        let on = emit(on_event, RoutingEvent::SelectActive(id));
        route_combo = route_combo.child(
            div()
                .id(SharedString::from(format!("rt-act-{id}")))
                .px_2()
                .py_1()
                .rounded_sm()
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
                .child(format!("{}{}", if sel { "● " } else { "○ " }, r.name))
                .on_click(move |_, w, cx| on(w, cx)),
        );
    }

    div()
        .flex()
        .flex_col()
        .child(
            div()
                .text_xs()
                .text_color(Theme::text_muted())
                .mb_2()
                .child("Common — domain strategy, active profile, rule-set mirror"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .mb_2()
                .child(
                    div()
                        .w(px(140.))
                        .text_xs()
                        .text_color(Theme::text_muted())
                        .child("Resolve Domain Strategy"),
                )
                .child(cycle_btn(
                    "rt-resolve",
                    strategy_label(&s.resolve_domain_strategy),
                    emit(on_event, RoutingEvent::Cycle("resolve")),
                )),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .mb_2()
                .child(
                    div()
                        .w(px(140.))
                        .text_xs()
                        .text_color(Theme::text_muted())
                        .child("Default Domain Strategy"),
                )
                .child(cycle_btn(
                    "rt-def-strat",
                    strategy_label(&s.default_domain_strategy),
                    emit(on_event, RoutingEvent::Cycle("default_strategy")),
                )),
        )
        .child(
            div()
                .text_xs()
                .text_color(Theme::text_muted())
                .mb_1()
                .child("Routing Profile (active)"),
        )
        .child(route_combo)
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .mb_2()
                .child(
                    div()
                        .w(px(140.))
                        .text_xs()
                        .text_color(Theme::text_muted())
                        .child("Remote Rule-set Mirror"),
                )
                .child(cycle_btn(
                    "rt-mirror",
                    s.ruleset_mirror.label().to_string(),
                    emit(on_event, RoutingEvent::Cycle("mirror")),
                )),
        )
        .child(div().text_xs().text_color(Theme::text_muted()).child(
            "Tip: double-click a profile on the Route tab (or Edit) to open the rule editor.",
        ))
        // silence unused
        .when(f == RtFocus::None, |d| d)
}

fn tab_hijack(
    draft: &RoutingDraft,
    inputs: Option<&RoutingInputs>,
    on_event: &Rc<dyn Fn(RoutingEvent, &mut Window, &mut App)>,
) -> impl IntoElement {
    let s = &draft.settings;
    let f = draft.focus;
    let enabled = s.enable_dns_server;
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .mb_2()
                .child("DNS Server"),
        )
        .child(
            div()
                .mb_2()
                .child(mode_switch("rt-dns-en", "Enable", s.enable_dns_server, {
                    let on = emit(on_event, RoutingEvent::Toggle("dns_server"));
                    move |_, w, cx| on(w, cx)
                })),
        )
        .child(div().mb_2().child(mode_switch(
            "rt-dns-lan",
            "Allow Lan to Connect",
            s.dns_server_listen_lan,
            {
                let on = emit(on_event, RoutingEvent::Toggle("dns_lan"));
                move |_, w, cx| on(w, cx)
            },
        )))
        .when(enabled, |d| {
            if let Some(inp) = inputs {
                d.child(input_field_row("Listen Port", &inp.dns_port, 120.))
                    .child(input_field_row("IPv4 Response", &inp.dns_v4, 120.))
                    .child(input_field_row("IPv6 Response", &inp.dns_v6, 120.))
                    .child(section_hint(
                        "Rules (domain: / suffix: / regex: / ruleset:)",
                    ))
                    .child(div().mb_2().child(editor_area(&inp.dns_rules)))
            } else {
                d.child(field_row(
                    "rt-dns-port",
                    "Listen Port",
                    s.dns_server_listen_port.to_string(),
                    f == RtFocus::DnsPort,
                    emit(on_event, RoutingEvent::SetFocus(RtFocus::DnsPort)),
                ))
                .child(field_row(
                    "rt-dns-v4",
                    "IPv4 Response",
                    s.dns_v4_resp.clone(),
                    f == RtFocus::DnsV4,
                    emit(on_event, RoutingEvent::SetFocus(RtFocus::DnsV4)),
                ))
                .child(field_row(
                    "rt-dns-v6",
                    "IPv6 Response",
                    s.dns_v6_resp.clone(),
                    f == RtFocus::DnsV6,
                    emit(on_event, RoutingEvent::SetFocus(RtFocus::DnsV6)),
                ))
                .child(multi_field(
                    "rt-dns-rules",
                    "Rules (domain: / suffix: / regex: / ruleset:)",
                    &draft.dns_rules_text(),
                    f == RtFocus::DnsRules,
                    80.,
                    emit(on_event, RoutingEvent::SetFocus(RtFocus::DnsRules)),
                ))
            }
        })
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .mt_3()
                .mb_2()
                .child("Redirect Settings"),
        )
        .child(
            div()
                .mb_2()
                .child(mode_switch("rt-redir-en", "Enable", s.enable_redirect, {
                    let on = emit(on_event, RoutingEvent::Toggle("redirect"));
                    move |_, w, cx| on(w, cx)
                })),
        )
        .when(s.enable_redirect, |d| {
            if let Some(inp) = inputs {
                d.child(input_field_row("Listen Address", &inp.redirect_addr, 120.))
                    .child(input_field_row("Listen Port", &inp.redirect_port, 120.))
            } else {
                d.child(field_row(
                    "rt-redir-addr",
                    "Listen Address",
                    s.redirect_listen_address.clone(),
                    f == RtFocus::RedirectAddr,
                    emit(on_event, RoutingEvent::SetFocus(RtFocus::RedirectAddr)),
                ))
                .child(field_row(
                    "rt-redir-port",
                    "Listen Port",
                    s.redirect_listen_port.to_string(),
                    f == RtFocus::RedirectPort,
                    emit(on_event, RoutingEvent::SetFocus(RtFocus::RedirectPort)),
                ))
            }
        })
}

fn tab_warp(
    draft: &RoutingDraft,
    inputs: Option<&RoutingInputs>,
    on_event: &Rc<dyn Fn(RoutingEvent, &mut Window, &mut App)>,
) -> impl IntoElement {
    let s = &draft.settings;
    let f = draft.focus;
    let mut root = div()
        .flex()
        .flex_col()
        .child(
            div()
                .mb_2()
                .child(mode_switch("rt-warp-en", "Enable Warp", s.enable_warp, {
                    let on = emit(on_event, RoutingEvent::Toggle("warp"));
                    move |_, w, cx| on(w, cx)
                })),
        );
    root = if let Some(inp) = inputs {
        root.child(input_field_row("Endpoint", &inp.warp_ep, 120.))
            .child(input_field_row("Private Key", &inp.warp_priv, 120.))
            .child(input_field_row("Public Key", &inp.warp_pub, 120.))
            .child(input_field_row(
                "Interface Addresses",
                &inp.warp_addrs,
                120.,
            ))
            .child(input_field_row("Reserved", &inp.warp_reserved, 120.))
    } else {
        root.child(field_row(
            "rt-warp-ep",
            "Endpoint",
            s.warp_ep.clone(),
            f == RtFocus::WarpEp,
            emit(on_event, RoutingEvent::SetFocus(RtFocus::WarpEp)),
        ))
        .child(field_row(
            "rt-warp-priv",
            "Private Key",
            s.warp_private_key.clone(),
            f == RtFocus::WarpPriv,
            emit(on_event, RoutingEvent::SetFocus(RtFocus::WarpPriv)),
        ))
        .child(field_row(
            "rt-warp-pub",
            "Public Key",
            s.warp_public_key.clone(),
            f == RtFocus::WarpPub,
            emit(on_event, RoutingEvent::SetFocus(RtFocus::WarpPub)),
        ))
        .child(field_row(
            "rt-warp-addr",
            "Interface Addresses",
            s.warp_ifc_addrs.join(","),
            f == RtFocus::WarpAddrs,
            emit(on_event, RoutingEvent::SetFocus(RtFocus::WarpAddrs)),
        ))
        .child(field_row(
            "rt-warp-rsv",
            "Reserved",
            s.warp_reserved.join(","),
            f == RtFocus::WarpReserved,
            emit(on_event, RoutingEvent::SetFocus(RtFocus::WarpReserved)),
        ))
    };
    root.child(
        div()
            .mt_2()
            .child(secondary_btn("rt-warp-gen", "Generate Warp Config", {
                let on = emit(on_event, RoutingEvent::Action("warp-gen"));
                move |_, w, cx| on(w, cx)
            })),
    )
    .child(
        div()
            .mt_2()
            .text_xs()
            .text_color(Theme::text_muted())
            .child(
            "Generate requires a running core (GenWgKeyPair RPC). Fill fields manually if offline.",
        ),
    )
}

fn tab_dns(
    draft: &RoutingDraft,
    inputs: Option<&RoutingInputs>,
    on_event: &Rc<dyn Fn(RoutingEvent, &mut Window, &mut App)>,
) -> impl IntoElement {
    // Layout mirrors upstream DialogManageRoutes DNS tab.
    // When `inputs` is present, single-line fields use real gpui-component Input.
    let s = &draft.settings;
    let f = draft.focus;
    let use_obj = s.use_dns_object;
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .mb_2()
                .child("Simple DNS Settings"),
        )
        .when(!use_obj, |d| {
            d.child(
                // Remote DNS row: [label] [editable+▼] [Query Strategy] [cycle]
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .mb_1p5()
                    .child(
                        div()
                            .w(px(100.))
                            .text_xs()
                            .text_color(Theme::text_muted())
                            .child("Remote DNS"),
                    )
                    .child(if let Some(inp) = inputs {
                        dns_input_with_preset(
                            &inp.remote_dns,
                            emit(on_event, RoutingEvent::Cycle("remote_dns_preset")),
                        )
                        .into_any_element()
                    } else {
                        line_edit(
                            "rt-rdns",
                            s.remote_dns.clone(),
                            f == RtFocus::RemoteDns,
                            Some("tls://8.8.8.8"),
                            emit(on_event, RoutingEvent::SetFocus(RtFocus::RemoteDns)),
                            Some(emit(on_event, RoutingEvent::Cycle("remote_dns_preset"))),
                        )
                        .into_any_element()
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(Theme::text_muted())
                            .child("Query Strategy"),
                    )
                    .child(cycle_btn(
                        "rt-rdns-s",
                        strategy_label(&s.remote_dns_strategy),
                        emit(on_event, RoutingEvent::Cycle("remote_dns_strategy")),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .mb_1p5()
                    .child(
                        div()
                            .w(px(100.))
                            .text_xs()
                            .text_color(Theme::text_muted())
                            .child("Direct DNS"),
                    )
                    .child(if let Some(inp) = inputs {
                        dns_input_with_preset(
                            &inp.direct_dns,
                            emit(on_event, RoutingEvent::Cycle("direct_dns_preset")),
                        )
                        .into_any_element()
                    } else {
                        line_edit(
                            "rt-ddns",
                            s.direct_dns.clone(),
                            f == RtFocus::DirectDns,
                            Some("localhost"),
                            emit(on_event, RoutingEvent::SetFocus(RtFocus::DirectDns)),
                            Some(emit(on_event, RoutingEvent::Cycle("direct_dns_preset"))),
                        )
                        .into_any_element()
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(Theme::text_muted())
                            .child("Query Strategy"),
                    )
                    .child(cycle_btn(
                        "rt-ddns-s",
                        strategy_label(&s.direct_dns_strategy),
                        emit(on_event, RoutingEvent::Cycle("direct_dns_strategy")),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .mb_1p5()
                    .child(
                        div()
                            .w(px(100.))
                            .text_xs()
                            .text_color(Theme::text_muted())
                            .child("Local Override"),
                    )
                    .child(if let Some(inp) = inputs {
                        input_inline(&inp.local_override).into_any_element()
                    } else {
                        line_edit(
                            "rt-local",
                            s.core_box_underlying_dns.clone(),
                            f == RtFocus::LocalOverride,
                            Some("macOS Tun: e.g. 223.5.5.5"),
                            emit(on_event, RoutingEvent::SetFocus(RtFocus::LocalOverride)),
                            None,
                        )
                        .into_any_element()
                    }),
            )
            .child(
                // Default DNS server + FakeIP + DNS Routing (upstream same row)
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .mb_1p5()
                    .flex_wrap()
                    .child(
                        div()
                            .w(px(100.))
                            .text_xs()
                            .text_color(Theme::text_muted())
                            .child("Default DNS server"),
                    )
                    .child(cycle_btn(
                        "rt-final",
                        if s.dns_final_out.trim().eq_ignore_ascii_case("direct") {
                            "direct".into()
                        } else {
                            "remote".into()
                        },
                        emit(on_event, RoutingEvent::Cycle("dns_final_out")),
                    ))
                    .child(mode_switch("rt-fakeip", "Enable FakeIP", s.fake_dns, {
                        let on = emit(on_event, RoutingEvent::Toggle("fake_dns"));
                        move |_, w, cx| on(w, cx)
                    }))
                    .child(mode_switch(
                        "rt-dns-route",
                        "Enable DNS Routing",
                        s.enable_dns_routing,
                        {
                            let on = emit(on_event, RoutingEvent::Toggle("dns_routing"));
                            move |_, w, cx| on(w, cx)
                        },
                    )),
            )
            .child(
                // Cache Capacity + disable flags (upstream same row)
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .mb_1p5()
                    .flex_wrap()
                    .child(
                        div()
                            .w(px(100.))
                            .text_xs()
                            .text_color(Theme::text_muted())
                            .child("Cache Capacity"),
                    )
                    .child(if let Some(inp) = inputs {
                        div()
                            .w(px(120.))
                            .child(input_inline(&inp.cache_cap))
                            .into_any_element()
                    } else {
                        line_edit(
                            "rt-cache",
                            s.dns_cache_capacity.to_string(),
                            f == RtFocus::CacheCap,
                            Some("65536"),
                            emit(on_event, RoutingEvent::SetFocus(RtFocus::CacheCap)),
                            None,
                        )
                        .into_any_element()
                    })
                    .child(mode_switch(
                        "rt-dcache",
                        "Disable Cache",
                        s.dns_disable_cache,
                        {
                            let on = emit(on_event, RoutingEvent::Toggle("dns_disable_cache"));
                            move |_, w, cx| on(w, cx)
                        },
                    ))
                    .child(mode_switch(
                        "rt-dexpire",
                        "Disable Expire",
                        s.dns_disable_expire,
                        {
                            let on = emit(on_event, RoutingEvent::Toggle("dns_disable_expire"));
                            move |_, w, cx| on(w, cx)
                        },
                    ))
                    .child(mode_switch(
                        "rt-rmap",
                        "Reverse Mapping",
                        s.dns_reverse_mapping,
                        {
                            let on = emit(on_event, RoutingEvent::Toggle("dns_reverse_mapping"));
                            move |_, w, cx| on(w, cx)
                        },
                    )),
            )
        })
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .mt_2()
                .mb_2()
                .child("DNS Object Settings"),
        )
        .child(div().mb_2().child(mode_switch(
            "rt-use-obj",
            "Use DNS Object",
            s.use_dns_object,
            {
                let on = emit(on_event, RoutingEvent::Toggle("use_dns_object"));
                move |_, w, cx| on(w, cx)
            },
        )))
        .when(use_obj, |d| {
            let d = if let Some(inp) = inputs {
                d.child(section_hint("DNS Object (sing-box dns JSON)"))
                    .child(div().mb_2().child(input_area(&inp.dns_object)))
            } else {
                d.child(multi_field(
                    "rt-dns-obj",
                    "DNS Object (sing-box dns JSON)",
                    &s.dns_object,
                    f == RtFocus::DnsObject,
                    120.,
                    emit(on_event, RoutingEvent::SetFocus(RtFocus::DnsObject)),
                ))
            };
            d.child(
                div()
                    .flex()
                    .gap_2()
                    .child(secondary_btn("rt-fmt", "Format", {
                        let on = emit(on_event, RoutingEvent::Action("format-dns"));
                        move |_, w, cx| on(w, cx)
                    }))
                    .child(secondary_btn("rt-doc", "Document", {
                        let on = emit(on_event, RoutingEvent::Action("dns-doc"));
                        move |_, w, cx| on(w, cx)
                    })),
            )
        })
}

fn tab_route(
    draft: &RoutingDraft,
    on_event: &Rc<dyn Fn(RoutingEvent, &mut Window, &mut App)>,
) -> impl IntoElement {
    // Upstream Route tab: QGroupBox "Routing Profiles" → QListWidget + button row.
    let mut list = div()
        .id("rt-profiles")
        .flex()
        .flex_col()
        .gap_0p5()
        .min_h(px(260.))
        .max_h(px(320.))
        .overflow_y_scroll()
        .border_1()
        .border_color(Theme::border_light())
        .rounded_sm()
        .bg(Theme::bg_app())
        .p_1();

    for (i, r) in draft.routes.iter().enumerate() {
        let sel = draft.selected_idx == i;
        let on = emit(on_event, RoutingEvent::SelectRoute(i));
        let on_edit = emit(on_event, RoutingEvent::Action("edit"));
        let active = r.id == draft.active_id;
        list = list.child(
            div()
                .id(SharedString::from(format!("rt-p-{i}")))
                .px_2()
                .py_1p5()
                .rounded_sm()
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
                .child(format!(
                    "{}{}{}",
                    if sel { "● " } else { "○ " },
                    r.summary(),
                    if active { "  ← active" } else { "" }
                ))
                .on_click(move |_, w, cx| on(w, cx))
                .on_click(move |_, w, cx| {
                    let _ = (&on_edit, w, cx);
                }),
        );
    }

    let btn = |id: &'static str, label: &'static str, action: &'static str| {
        let on = emit(on_event, RoutingEvent::Action(action));
        secondary_btn(id, label, move |_, w, cx| on(w, cx))
    };

    group_panel(
        "Routing Profiles",
        div().flex().flex_col().child(list).child(
            // Upstream: New · Clone · Export · Import · Edit · Delete · Update
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .mt_2()
                .child(btn("rt-new", "New", "new-menu"))
                .child(btn("rt-clone", "Clone", "clone"))
                .child(btn("rt-export", "Export", "export"))
                .child(btn("rt-import", "Import", "import"))
                .child(btn("rt-edit", "Edit", "edit"))
                .child(btn("rt-del", "Delete", "delete"))
                .child(btn("rt-upd", "Update", "update-menu")),
        ),
    )
}

fn build_nested(
    draft: &RoutingDraft,
    nested_inputs: Option<&NestedInputs>,
    on_event: Rc<dyn Fn(RoutingEvent, &mut Window, &mut App)>,
) -> Option<impl IntoElement> {
    match &draft.nested {
        RoutingNested::None => None,
        RoutingNested::NewMenu => Some(
            popup_menu(
                "New route profile",
                &[
                    ("Structured profile", "new-structured"),
                    ("Raw profile", "new-raw"),
                    ("Remote profile", "new-remote"),
                ],
                on_event,
            )
            .into_any_element(),
        ),
        RoutingNested::UpdateMenu { sel_is_remote } => {
            let mut items: Vec<(&str, &str)> = Vec::new();
            if *sel_is_remote {
                items.push(("Update selected", "update-selected"));
            }
            items.push(("Update all", "update-all"));
            Some(popup_menu("Update remote profiles", &items, on_event).into_any_element())
        }
        RoutingNested::ImportPaste { text } => {
            let body = if let Some(inp) = nested_inputs.and_then(|n| n.as_import_paste()) {
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_xs()
                            .text_color(Theme::text_muted())
                            .mb_2()
                            .child(
                                "Paste a Throne route link, a remoteRoute link, a base64 blob, or a JSON rule array",
                            ),
                    )
                    .child(section_hint("Input"))
                    .child(div().mb_2().child(input_area(inp)))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .mt_2()
                            .child(secondary_btn("rt-imp-c", "Cancel", {
                                let on = emit(&on_event, RoutingEvent::NestedClose);
                                move |_, w, cx| on(w, cx)
                            }))
                            .child(primary_btn("rt-imp-ok", "OK", {
                                let on = emit(&on_event, RoutingEvent::NestedAction("import-ok"));
                                move |_, w, cx| on(w, cx)
                            })),
                    )
            } else {
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_xs()
                            .text_color(Theme::text_muted())
                            .mb_2()
                            .child(
                                "Paste a Throne route link, a remoteRoute link, a base64 blob, or a JSON rule array",
                            ),
                    )
                    .child(multi_field(
                        "rt-imp-text",
                        "Input",
                        text,
                        true,
                        140.,
                        emit(&on_event, RoutingEvent::NestedAction("focus-import")),
                    ))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .mt_2()
                            .child(secondary_btn("rt-imp-c", "Cancel", {
                                let on = emit(&on_event, RoutingEvent::NestedClose);
                                move |_, w, cx| on(w, cx)
                            }))
                            .child(primary_btn("rt-imp-ok", "OK", {
                                let on = emit(&on_event, RoutingEvent::NestedAction("import-ok"));
                                move |_, w, cx| on(w, cx)
                            })),
                    )
            };
            Some(body.into_any_element())
        }
        RoutingNested::Notice { body, .. } => Some(
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_sm()
                        .text_color(Theme::text())
                        .mb_3()
                        .child(body.clone()),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .child(primary_btn("rt-n-ok", "OK", {
                            let on = emit(&on_event, RoutingEvent::NestedClose);
                            move |_, w, cx| on(w, cx)
                        })),
                )
                .into_any_element(),
        ),
        RoutingNested::RouteEditor(ed) => {
            Some(route_editor_view(ed, nested_inputs, on_event).into_any_element())
        }
        RoutingNested::RawEditor(ed) => {
            Some(raw_editor_view(ed, nested_inputs, on_event).into_any_element())
        }
    }
}

fn popup_menu(
    _title: &'static str,
    items: &[(&str, &str)],
    on_event: Rc<dyn Fn(RoutingEvent, &mut Window, &mut App)>,
) -> impl IntoElement {
    // Body only — title comes from the independent open_dialog layer.
    let mut col = div().flex().flex_col().gap_1();
    for (label, action) in items {
        let action: &'static str = match *action {
            "new-structured" => "new-structured",
            "new-raw" => "new-raw",
            "new-remote" => "new-remote",
            "update-selected" => "update-selected",
            "update-all" => "update-all",
            other => Box::leak(other.to_string().into_boxed_str()),
        };
        let on = emit(&on_event, RoutingEvent::NestedAction(action));
        let label = (*label).to_string();
        col = col.child(
            div()
                .id(SharedString::from(format!("rt-menu-{action}")))
                .px_3()
                .py_2()
                .rounded_sm()
                .cursor_pointer()
                .hover(|e| e.bg(Theme::bg_hover()).text_color(Theme::accent()))
                .active(|e| e.bg(Theme::accent_soft()).text_color(Theme::accent()))
                .text_sm()
                .text_color(Theme::text())
                .child(label)
                .on_click(move |_, w, cx| on(w, cx)),
        );
    }
    div()
        .flex()
        .flex_col()
        .child(col)
        .child(
            div()
                .mt_2()
                .flex()
                .justify_end()
                .child(secondary_btn("rt-menu-c", "Cancel", {
                    let on = emit(&on_event, RoutingEvent::NestedClose);
                    move |_, w, cx| on(w, cx)
                })),
        )
}

fn route_editor_view(
    ed: &RouteEditorDraft,
    nested_inputs: Option<&NestedInputs>,
    on_event: Rc<dyn Fn(RoutingEvent, &mut Window, &mut App)>,
) -> impl IntoElement {
    // Layout tracks upstream RouteItem.ui: General · Remote · Basic/Advanced tabs · OK/Cancel.
    let f = ed.focus;
    let tab_labels: Vec<SharedString> = vec!["Basic".into(), "Advanced".into()];
    let tab_sel = if ed.tab == RouteEditorTab::Basic {
        0
    } else {
        1
    };
    let on_tabs = on_event.clone();
    let tabs = tab_bar("re-tabs", tab_sel, tab_labels, move |ix, w, cx| {
        let t = if *ix == 0 {
            RouteEditorTab::Basic
        } else {
            RouteEditorTab::Advanced
        };
        on_tabs(RoutingEvent::ReTab(t), w, cx);
    });

    let name_row = if let Some(re) = nested_inputs.and_then(|n| n.as_route_editor()) {
        input_field_row("Name", &re.name, 120.).into_any_element()
    } else {
        field_row(
            "re-name",
            "Name",
            ed.profile.name.clone(),
            f == ReFocus::Name,
            emit(&on_event, RoutingEvent::ReFocus(ReFocus::Name)),
        )
        .into_any_element()
    };

    // Upstream QGroupBox "General"
    let general = group_panel(
        "General",
        div().flex().flex_col().child(name_row).child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(120.))
                        .text_xs()
                        .text_color(Theme::text_muted())
                        .child("Default outbound"),
                )
                .child(cycle_btn(
                    "re-defout",
                    ed.profile.default_outbound.label(),
                    emit(&on_event, RoutingEvent::ReCycleDefOut),
                )),
        ),
    );

    let remote = if ed.profile.is_remote {
        let url_row = if let Some(re) = nested_inputs.and_then(|n| n.as_route_editor()) {
            input_field_row("URL", &re.url, 40.).into_any_element()
        } else {
            field_row(
                "re-url",
                "URL",
                ed.profile.remote_url.clone(),
                f == ReFocus::RemoteUrl,
                emit(&on_event, RoutingEvent::ReFocus(ReFocus::RemoteUrl)),
            )
            .into_any_element()
        };
        // Upstream QGroupBox "Remote source"
        group_panel(
            "Remote source",
            div().flex().flex_col().child(url_row).child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .mt_1()
                    .child(mode_switch(
                        "re-auto",
                        "Auto update",
                        ed.profile.auto_update,
                        {
                            let on = emit(&on_event, RoutingEvent::ReToggle("auto_update"));
                            move |_, w, cx| on(w, cx)
                        },
                    ))
                    .child(div().flex_1())
                    .child(secondary_btn("re-prev", "Preview", {
                        let on = emit(&on_event, RoutingEvent::NestedAction("remote-preview"));
                        move |_, w, cx| on(w, cx)
                    }))
                    .child(secondary_btn("re-fetch", "Fetch", {
                        let on = emit(&on_event, RoutingEvent::NestedAction("remote-fetch"));
                        move |_, w, cx| on(w, cx)
                    })),
            ),
        )
        .into_any_element()
    } else {
        div().into_any_element()
    };

    let body = match ed.tab {
        RouteEditorTab::Basic => {
            // Upstream: "How to use" + 2×2 QGroupBox QTextEdits.
            // Definite Input height (not h_full) — percentage height collapses without a
            // definite ancestor size. Dialog max_h + overflow handles viewport overflow.
            const RULE_H: f32 = 140.;
            let cell = |title: &'static str, field: AnyElement| {
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .border_1()
                    .border_color(Theme::border_light())
                    .rounded_md()
                    .p_2()
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .mb_1()
                            .text_color(Theme::text())
                            .child(title),
                    )
                    .child(div().w_full().h(px(RULE_H)).child(field))
            };
            let simple = if let Some(re) = nested_inputs.and_then(|n| n.as_route_editor()) {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(cell(
                                "Direct",
                                editor_area_tall(&re.simple_direct, RULE_H).into_any_element(),
                            ))
                            .child(cell(
                                "Proxy",
                                editor_area_tall(&re.simple_proxy, RULE_H).into_any_element(),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(cell(
                                "Block",
                                editor_area_tall(&re.simple_block, RULE_H).into_any_element(),
                            ))
                            .child(cell(
                                "Warp-bypass",
                                editor_area_tall(&re.simple_warp, RULE_H).into_any_element(),
                            )),
                    )
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(div().flex_1().child(multi_field(
                                "re-sd",
                                "Direct",
                                &ed.simple_direct,
                                f == ReFocus::SimpleDirect,
                                RULE_H,
                                emit(&on_event, RoutingEvent::ReFocus(ReFocus::SimpleDirect)),
                            )))
                            .child(div().flex_1().child(multi_field(
                                "re-sp",
                                "Proxy",
                                &ed.simple_proxy,
                                f == ReFocus::SimpleProxy,
                                RULE_H,
                                emit(&on_event, RoutingEvent::ReFocus(ReFocus::SimpleProxy)),
                            ))),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(div().flex_1().child(multi_field(
                                "re-sb",
                                "Block",
                                &ed.simple_block,
                                f == ReFocus::SimpleBlock,
                                RULE_H,
                                emit(&on_event, RoutingEvent::ReFocus(ReFocus::SimpleBlock)),
                            )))
                            .child(div().flex_1().child(multi_field(
                                "re-sw",
                                "Warp-bypass",
                                &ed.simple_warp,
                                f == ReFocus::SimpleWarp,
                                RULE_H,
                                emit(&on_event, RoutingEvent::ReFocus(ReFocus::SimpleWarp)),
                            ))),
                    )
                    .into_any_element()
            };
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .mb_2()
                        .child(secondary_btn("re-how", "How to use", {
                            let on = emit(&on_event, RoutingEvent::NestedAction("how-to-use"));
                            move |_, w, cx| on(w, cx)
                        }))
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .text_color(Theme::text_muted())
                                .child(
                                    "domain: / suffix: / keyword: / regex: / ruleset: / ip: / processName: / processPath:",
                                ),
                        ),
                )
                .child(simple)
                .into_any_element()
        }
        RouteEditorTab::Advanced => {
            // Upstream: Rules (list | attrs) horizontal, then Rule Settings.
            let mut list = div()
                .id("re-rules")
                .flex()
                .flex_col()
                .gap_0p5()
                .flex_1()
                .min_h(px(140.))
                .max_h(px(200.))
                .overflow_y_scroll()
                .border_1()
                .border_color(Theme::border_light())
                .rounded_sm()
                .bg(Theme::bg_app())
                .p_1();
            for (i, r) in ed.profile.rules.iter().enumerate() {
                let sel = ed.selected_rule == Some(i);
                let on = emit(&on_event, RoutingEvent::ReSelectRule(i));
                let label = format!(
                    "{} {} → {}",
                    if sel { "●" } else { "○" },
                    if r.name.is_empty() {
                        format!("rule_{i}")
                    } else {
                        r.name.clone()
                    },
                    DefaultOutbound::from_id(r.outbound_id).label()
                );
                list = list.child(
                    div()
                        .id(SharedString::from(format!("re-r-{i}")))
                        .px_2()
                        .py_1()
                        .rounded_sm()
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
                        .text_xs()
                        .child(label)
                        .on_click(move |_, w, cx| on(w, cx)),
                );
            }

            let list_col = div().w(px(220.)).flex().flex_col().child(list).child(
                // Upstream: New · Move Up · Move Down · Delete
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .mt_2()
                    .child(secondary_btn("re-new-r", "New", {
                        let on = emit(&on_event, RoutingEvent::NestedAction("rule-new"));
                        move |_, w, cx| on(w, cx)
                    }))
                    .child(secondary_btn("re-up-r", "Move Up", {
                        let on = emit(&on_event, RoutingEvent::NestedAction("rule-up"));
                        move |_, w, cx| on(w, cx)
                    }))
                    .child(secondary_btn("re-dn-r", "Move Down", {
                        let on = emit(&on_event, RoutingEvent::NestedAction("rule-down"));
                        move |_, w, cx| on(w, cx)
                    }))
                    .child(secondary_btn("re-del-r", "Delete", {
                        let on = emit(&on_event, RoutingEvent::NestedAction("rule-del"));
                        move |_, w, cx| on(w, cx)
                    })),
            );

            let detail = if let Some(i) = ed.selected_rule {
                if let Some(r) = ed.profile.rules.get(i) {
                    if let Some(re) = nested_inputs.and_then(|n| n.as_route_editor()) {
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.))
                            .child(input_field_row("Name", &re.rule_name, 70.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .mb_2()
                                    .child(
                                        div()
                                            .w(px(70.))
                                            .text_xs()
                                            .text_color(Theme::text_muted())
                                            .child("Outbound"),
                                    )
                                    .child(cycle_btn(
                                        "re-rout",
                                        DefaultOutbound::from_id(r.outbound_id).label(),
                                        emit(&on_event, RoutingEvent::ReCycleRuleOut),
                                    )),
                            )
                            .child(input_field_row("Protocol", &re.rule_protocol, 70.))
                            .child(section_hint("domain (one per line)"))
                            .child(div().mb_2().child(input_area_tall(&re.rule_domain, 56.)))
                            .child(section_hint("domain_suffix"))
                            .child(div().mb_2().child(input_area_tall(&re.rule_suffix, 56.)))
                            .child(section_hint("ip_cidr"))
                            .child(div().mb_1().child(input_area_tall(&re.rule_ip, 56.)))
                            .into_any_element()
                    } else {
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(Theme::text_muted())
                            .child("Select a rule")
                            .into_any_element()
                    }
                } else {
                    div().flex_1().into_any_element()
                }
            } else {
                div()
                    .flex_1()
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .p_2()
                    .child("Select a rule to edit attributes")
                    .into_any_element()
            };

            group_panel(
                "Rules",
                div()
                    .flex()
                    .gap_3()
                    .items_start()
                    .child(list_col)
                    .child(detail),
            )
            .into_any_element()
        }
    };

    // Body only — hosted as an independent open_dialog layer.
    // No forced min_h: dialog `.max_h` + built-in content scrollbar handles overflow.
    div()
        .flex()
        .flex_col()
        .w_full()
        .child(general)
        .child(remote)
        .child(div().mb_2().child(tabs))
        .child(body)
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .mt_3()
                .pt_2()
                .border_t_1()
                .border_color(Theme::border_light())
                .child(secondary_btn("re-c", "Cancel", {
                    let on = emit(&on_event, RoutingEvent::NestedClose);
                    move |_, w, cx| on(w, cx)
                }))
                .child(primary_btn("re-ok", "OK", {
                    let on = emit(&on_event, RoutingEvent::NestedAction("editor-ok"));
                    move |_, w, cx| on(w, cx)
                })),
        )
}

fn raw_editor_view(
    ed: &RawEditorDraft,
    nested_inputs: Option<&NestedInputs>,
    on_event: Rc<dyn Fn(RoutingEvent, &mut Window, &mut App)>,
) -> impl IntoElement {
    let f = ed.focus;
    let name_row = if let Some(raw) = nested_inputs.and_then(|n| n.as_raw_editor()) {
        input_field_row("Name", &raw.name, 120.).into_any_element()
    } else {
        field_row(
            "raw-name",
            "Name",
            ed.name.clone(),
            f == ReFocus::Name,
            emit(&on_event, RoutingEvent::ReFocus(ReFocus::Name)),
        )
        .into_any_element()
    };
    let json_field = if let Some(raw) = nested_inputs.and_then(|n| n.as_raw_editor()) {
        div()
            .mb_2()
            .child(section_hint("sing-box route JSON"))
            .child(input_area(&raw.json))
            .into_any_element()
    } else {
        multi_field(
            "raw-json",
            "sing-box route JSON",
            &ed.raw_route,
            f == ReFocus::RawJson || f != ReFocus::Name,
            200.,
            emit(&on_event, RoutingEvent::ReFocus(ReFocus::RawJson)),
        )
        .into_any_element()
    };
    div()
        .flex()
        .flex_col()
        .child(name_row)
        .child(div().mb_2().child(mode_switch(
            "raw-prev",
            "Prevent modifications (use verbatim)",
            ed.prevent_modifications,
            {
                let on = emit(&on_event, RoutingEvent::ReToggle("prevent_mod"));
                move |_, w, cx| on(w, cx)
            },
        )))
        .child(json_field)
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .mt_2()
                .child(secondary_btn("raw-c", "Cancel", {
                    let on = emit(&on_event, RoutingEvent::NestedClose);
                    move |_, w, cx| on(w, cx)
                }))
                .child(primary_btn("raw-ok", "OK", {
                    let on = emit(&on_event, RoutingEvent::NestedAction("raw-ok"));
                    move |_, w, cx| on(w, cx)
                })),
        )
}

// ── Event application (pure draft mutations) ────────────────────────────────

impl RoutingDraft {
    pub fn apply_event(&mut self, ev: RoutingEvent) -> RoutingSideEffect {
        match ev {
            RoutingEvent::Close => RoutingSideEffect::Close,
            RoutingEvent::Ok => RoutingSideEffect::Commit,
            RoutingEvent::SetTab(t) => {
                self.tab = t;
                // Upstream gives each tab a default focus widget so typing works immediately.
                self.focus = match t {
                    RoutingTab::Dns => RtFocus::RemoteDns,
                    RoutingTab::Warp => RtFocus::WarpEp,
                    RoutingTab::Hijack => RtFocus::DnsPort,
                    RoutingTab::Common | RoutingTab::Route => RtFocus::None,
                };
                RoutingSideEffect::None
            }
            RoutingEvent::SetFocus(f) => {
                self.focus = f;
                RoutingSideEffect::None
            }
            RoutingEvent::Toggle(name) => {
                self.toggle(name);
                RoutingSideEffect::None
            }
            RoutingEvent::Cycle(name) => {
                self.cycle(name);
                RoutingSideEffect::None
            }
            RoutingEvent::SelectRoute(i) => {
                if i < self.routes.len() {
                    self.selected_idx = i;
                }
                RoutingSideEffect::None
            }
            RoutingEvent::SelectActive(id) => {
                self.active_id = id;
                if let Some(i) = self.routes.iter().position(|r| r.id == id) {
                    self.selected_idx = i;
                }
                RoutingSideEffect::None
            }
            RoutingEvent::Action(a) => self.handle_action(a),
            RoutingEvent::NestedClose => {
                self.nested = RoutingNested::None;
                RoutingSideEffect::None
            }
            RoutingEvent::NestedAction(a) => self.handle_nested_action(a),
            RoutingEvent::ReFocus(f) => {
                if let RoutingNested::RouteEditor(ed) = &mut self.nested {
                    ed.focus = f;
                } else if let RoutingNested::RawEditor(ed) = &mut self.nested {
                    ed.focus = f;
                }
                RoutingSideEffect::None
            }
            RoutingEvent::ReTab(t) => {
                if let RoutingNested::RouteEditor(ed) = &mut self.nested {
                    if ed.tab == RouteEditorTab::Basic && t == RouteEditorTab::Advanced {
                        let err = ed.apply_simple_to_profile();
                        if !err.is_empty() {
                            self.notice = err;
                        }
                        ed.selected_rule = None;
                    } else if ed.tab == RouteEditorTab::Advanced && t == RouteEditorTab::Basic {
                        ed.reload_simple_from_profile();
                    }
                    ed.tab = t;
                }
                RoutingSideEffect::None
            }
            RoutingEvent::ReSelectRule(i) => {
                if let RoutingNested::RouteEditor(ed) = &mut self.nested {
                    ed.selected_rule = Some(i);
                    ed.focus = ReFocus::RuleName;
                }
                RoutingSideEffect::None
            }
            RoutingEvent::ReCycleDefOut => {
                if let RoutingNested::RouteEditor(ed) = &mut self.nested {
                    // proxy → direct → block → warp-bypass → proxy
                    ed.profile.default_outbound = match ed.profile.default_outbound {
                        DefaultOutbound::Proxy => DefaultOutbound::Direct,
                        DefaultOutbound::Direct => DefaultOutbound::Block,
                        DefaultOutbound::Block => DefaultOutbound::WarpBypass,
                        DefaultOutbound::WarpBypass | DefaultOutbound::Profile(_) => {
                            DefaultOutbound::Proxy
                        }
                    };
                }
                RoutingSideEffect::None
            }
            RoutingEvent::ReCycleRuleOut => {
                if let RoutingNested::RouteEditor(ed) = &mut self.nested {
                    if let Some(i) = ed.selected_rule {
                        if let Some(r) = ed.profile.rules.get_mut(i) {
                            let cur = DefaultOutbound::from_id(r.outbound_id);
                            let next = match cur {
                                DefaultOutbound::Proxy => DefaultOutbound::Direct,
                                DefaultOutbound::Direct => DefaultOutbound::Block,
                                DefaultOutbound::Block => DefaultOutbound::WarpBypass,
                                DefaultOutbound::WarpBypass | DefaultOutbound::Profile(_) => {
                                    DefaultOutbound::Proxy
                                }
                            };
                            r.outbound_id = next.as_id();
                        }
                    }
                }
                RoutingSideEffect::None
            }
            RoutingEvent::ReToggle(name) => {
                match name {
                    "auto_update" => {
                        if let RoutingNested::RouteEditor(ed) = &mut self.nested {
                            ed.profile.auto_update = !ed.profile.auto_update;
                        }
                    }
                    "prevent_mod" => {
                        if let RoutingNested::RawEditor(ed) = &mut self.nested {
                            ed.prevent_modifications = !ed.prevent_modifications;
                        }
                    }
                    _ => {}
                }
                RoutingSideEffect::None
            }
        }
    }

    fn toggle(&mut self, name: &str) {
        let s = &mut self.settings;
        match name {
            "dns_server" => s.enable_dns_server = !s.enable_dns_server,
            "dns_lan" => s.dns_server_listen_lan = !s.dns_server_listen_lan,
            "redirect" => s.enable_redirect = !s.enable_redirect,
            "warp" => s.enable_warp = !s.enable_warp,
            "fake_dns" => s.fake_dns = !s.fake_dns,
            "dns_routing" => s.enable_dns_routing = !s.enable_dns_routing,
            "dns_disable_cache" => s.dns_disable_cache = !s.dns_disable_cache,
            "dns_disable_expire" => s.dns_disable_expire = !s.dns_disable_expire,
            "dns_reverse_mapping" => s.dns_reverse_mapping = !s.dns_reverse_mapping,
            "use_dns_object" => s.use_dns_object = !s.use_dns_object,
            _ => {}
        }
    }

    fn cycle(&mut self, name: &str) {
        let s = &mut self.settings;
        match name {
            "resolve" => {
                s.resolve_domain_strategy = cycle_strategy(&s.resolve_domain_strategy);
            }
            "default_strategy" => {
                s.default_domain_strategy = cycle_strategy(&s.default_domain_strategy);
            }
            "mirror" => s.ruleset_mirror = s.ruleset_mirror.cycle(),
            "remote_dns_strategy" => {
                s.remote_dns_strategy = cycle_strategy(&s.remote_dns_strategy);
            }
            "direct_dns_strategy" => {
                s.direct_dns_strategy = cycle_strategy(&s.direct_dns_strategy);
            }
            // Upstream editable combo presets (▼)
            "remote_dns_preset" => {
                s.remote_dns = cycle_preset(&s.remote_dns, REMOTE_DNS_PRESETS);
                self.focus = RtFocus::RemoteDns;
            }
            "direct_dns_preset" => {
                s.direct_dns = cycle_preset(&s.direct_dns, DIRECT_DNS_PRESETS);
                self.focus = RtFocus::DirectDns;
            }
            "dns_final_out" => {
                s.dns_final_out = cycle_dns_final(&s.dns_final_out);
            }
            _ => {}
        }
    }

    fn handle_action(&mut self, a: &str) -> RoutingSideEffect {
        match a {
            "new-menu" => {
                self.nested = RoutingNested::NewMenu;
            }
            "update-menu" => {
                let sel_is_remote = self.selected().map(|r| r.is_remote).unwrap_or(false);
                self.nested = RoutingNested::UpdateMenu { sel_is_remote };
            }
            "clone" => {
                if let Some(r) = self.selected().cloned() {
                    let mut c = r;
                    c.name = format!("{} clone", c.name);
                    c.id = -1;
                    self.routes.push(c);
                    self.selected_idx = self.routes.len() - 1;
                    self.notice = "Cloned (save with OK)".into();
                }
            }
            "export" => {
                if let Some(r) = self.selected() {
                    let link = throne_import::to_share_link(r);
                    return RoutingSideEffect::CopyClipboard(link);
                }
            }
            "import" => return RoutingSideEffect::TryClipboardImport,
            "edit" => {
                if let Some(r) = self.selected().cloned() {
                    let idx = self.selected_idx;
                    if r.is_raw {
                        self.nested =
                            RoutingNested::RawEditor(RawEditorDraft::from_profile(Some(idx), &r));
                    } else {
                        self.nested = RoutingNested::RouteEditor(RouteEditorDraft::from_profile(
                            Some(idx),
                            r,
                        ));
                    }
                }
            }
            "delete" => {
                if self.routes.len() <= 1 {
                    self.nested = RoutingNested::Notice {
                        title: "Invalid operation".into(),
                        body: "Routing Profiles cannot be empty, try adding another profile or editing this one".into(),
                    };
                } else if self.selected_idx < self.routes.len() {
                    let removed = self.routes.remove(self.selected_idx);
                    if removed.id == self.active_id {
                        self.active_id = self.routes[0].id;
                    }
                    if self.selected_idx >= self.routes.len() {
                        self.selected_idx = self.routes.len() - 1;
                    }
                }
            }
            "format-dns" => {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&self.settings.dns_object)
                {
                    if let Ok(pretty) = serde_json::to_string_pretty(&v) {
                        self.settings.dns_object = pretty;
                        self.notice = "DNS object formatted".into();
                    }
                } else {
                    self.nested = RoutingNested::Notice {
                        title: "DNS".into(),
                        body: "Invalid json".into(),
                    };
                }
            }
            "dns-doc" => {
                self.nested = RoutingNested::Notice {
                    title: "DNS".into(),
                    body: "https://sing-box.sagernet.org/configuration/dns/".into(),
                };
            }
            "warp-gen" => return RoutingSideEffect::WarpGenerate,
            _ => {}
        }
        RoutingSideEffect::None
    }

    fn handle_nested_action(&mut self, a: &str) -> RoutingSideEffect {
        match a {
            "new-structured" => {
                let mut p = RouteProfile::new(-1, "New profile");
                p.ensure_default_dns_hijack();
                self.nested = RoutingNested::RouteEditor(RouteEditorDraft::from_profile(None, p));
            }
            "new-raw" => {
                let p = RouteProfile {
                    is_raw: true,
                    ..RouteProfile::new(-1, "New raw profile")
                };
                self.nested = RoutingNested::RawEditor(RawEditorDraft::from_profile(None, &p));
            }
            "new-remote" => {
                let mut p = RouteProfile::new(-1, "New remote");
                p.is_remote = true;
                p.ensure_default_dns_hijack();
                self.nested = RoutingNested::RouteEditor(RouteEditorDraft::from_profile(None, p));
            }
            "update-selected" => {
                self.nested = RoutingNested::None;
                if let Some(r) = self.selected() {
                    if r.is_remote && !r.remote_url.trim().is_empty() {
                        return RoutingSideEffect::UpdateRemotes(vec![r.clone()]);
                    }
                }
            }
            "update-all" => {
                self.nested = RoutingNested::None;
                let remotes: Vec<_> = self
                    .routes
                    .iter()
                    .filter(|r| r.is_remote && !r.remote_url.trim().is_empty())
                    .cloned()
                    .collect();
                if remotes.is_empty() {
                    self.nested = RoutingNested::Notice {
                        title: "No remote profiles".into(),
                        body: "There are no remote routing profiles to update.".into(),
                    };
                } else {
                    return RoutingSideEffect::UpdateRemotes(remotes);
                }
            }
            "import-ok" => {
                if let RoutingNested::ImportPaste { text } = &self.nested {
                    let text = text.clone();
                    self.nested = RoutingNested::None;
                    return RoutingSideEffect::ImportText(text);
                }
            }
            "editor-ok" => {
                if let RoutingNested::RouteEditor(mut ed) = std::mem::take(&mut self.nested) {
                    if ed.tab == RouteEditorTab::Basic {
                        let err = ed.apply_simple_to_profile();
                        if !err.is_empty() {
                            self.nested = RoutingNested::RouteEditor(ed);
                            self.notice = err;
                            return RoutingSideEffect::None;
                        }
                    }
                    let mut p = ed.profile;
                    if p.is_remote {
                        p.is_remote = true;
                    }
                    if let Some(idx) = ed.edit_idx {
                        if idx < self.routes.len() {
                            let id = self.routes[idx].id;
                            p.id = id;
                            if self.active_id == id || self.active_id == self.routes[idx].id {
                                self.active_id = id;
                            }
                            self.routes[idx] = p;
                        }
                    } else {
                        p.id = -1;
                        self.routes.push(p);
                        self.selected_idx = self.routes.len() - 1;
                    }
                    self.nested = RoutingNested::None;
                    self.notice = "Profile updated (save with OK)".into();
                }
            }
            "raw-ok" => {
                if let RoutingNested::RawEditor(ed) = std::mem::take(&mut self.nested) {
                    // validate JSON
                    if serde_json::from_str::<serde_json::Value>(&ed.raw_route).is_err() {
                        self.nested = RoutingNested::RawEditor(ed);
                        self.notice = "Invalid JSON in raw route".into();
                        return RoutingSideEffect::None;
                    }
                    if let Some(idx) = ed.edit_idx {
                        let id = self.routes.get(idx).map(|r| r.id).unwrap_or(-1);
                        let p = ed.into_profile(id);
                        if idx < self.routes.len() {
                            self.routes[idx] = p;
                        }
                    } else {
                        self.routes.push(ed.into_profile(-1));
                        self.selected_idx = self.routes.len() - 1;
                    }
                    self.nested = RoutingNested::None;
                }
            }
            "rule-new" => {
                if let RoutingNested::RouteEditor(ed) = &mut self.nested {
                    let n = ed.profile.rules.len();
                    ed.profile.rules.push(RouteRule {
                        name: format!("rule_{n}"),
                        action: "route".into(),
                        outbound_id: DefaultOutbound::Proxy.as_id(),
                        ..Default::default()
                    });
                    ed.selected_rule = Some(n);
                }
            }
            "rule-del" => {
                if let RoutingNested::RouteEditor(ed) = &mut self.nested {
                    if let Some(i) = ed.selected_rule {
                        if i < ed.profile.rules.len() {
                            ed.profile.rules.remove(i);
                            ed.selected_rule = None;
                        }
                    }
                }
            }
            "rule-up" => {
                if let RoutingNested::RouteEditor(ed) = &mut self.nested {
                    if let Some(i) = ed.selected_rule {
                        if i > 0 && i < ed.profile.rules.len() {
                            ed.profile.rules.swap(i, i - 1);
                            ed.selected_rule = Some(i - 1);
                        }
                    }
                }
            }
            "rule-down" => {
                if let RoutingNested::RouteEditor(ed) = &mut self.nested {
                    if let Some(i) = ed.selected_rule {
                        if i + 1 < ed.profile.rules.len() {
                            ed.profile.rules.swap(i, i + 1);
                            ed.selected_rule = Some(i + 1);
                        }
                    }
                }
            }
            "remote-preview" | "remote-fetch" => {
                if let RoutingNested::RouteEditor(ed) = &self.nested {
                    let url = ed.profile.remote_url.clone();
                    let apply = a == "remote-fetch";
                    if !url.trim().is_empty() {
                        return RoutingSideEffect::FetchRemote { url, apply };
                    }
                    self.notice = "Set a remote URL first".into();
                }
            }
            "focus-import" => {}
            "how-to-use" => {
                // Keep the editor open — surface help on the main Routes notice strip
                // (also visible after closing nested if user looks at footer).
                self.notice = "Simple rules (one per line): domain: / suffix: / keyword: / \
                    regex: / ruleset: / ip: / processName: / processPath:"
                    .into();
            }
            _ => {}
        }
        RoutingSideEffect::None
    }

    pub fn apply_imported_profile(&mut self, mut p: RouteProfile, was_legacy_array: bool) {
        if was_legacy_array {
            // open editor pre-filled
            p.name = if p.name.is_empty() {
                "Imported rules".into()
            } else {
                p.name
            };
            self.nested = RoutingNested::RouteEditor(RouteEditorDraft::from_profile(None, p));
        } else {
            p.id = -1;
            let name = p.name.clone();
            self.routes.push(p);
            self.selected_idx = self.routes.len() - 1;
            self.notice = format!("Imported «{name}»");
        }
    }

    pub fn apply_remote_fetch_to_editor(&mut self, incoming: RouteProfile, apply: bool) {
        if let RoutingNested::RouteEditor(ed) = &mut self.nested {
            if apply {
                let name = ed.profile.name.clone();
                let url = ed.profile.remote_url.clone();
                let auto = ed.profile.auto_update;
                ed.profile.rules = incoming.rules;
                ed.profile.is_raw = incoming.is_raw;
                ed.profile.raw_route = incoming.raw_route;
                if ed.profile.name.is_empty() {
                    ed.profile.name = name;
                }
                ed.profile.remote_url = url;
                ed.profile.auto_update = auto;
                ed.profile.is_remote = true;
                ed.reload_simple_from_profile();
                self.notice = "Remote rules applied".into();
            } else {
                self.notice = format!(
                    "Preview: «{}» · {} rules",
                    incoming.name,
                    incoming.rules.len()
                );
            }
        }
    }

    pub fn apply_remote_update_results(&mut self, updated: Vec<(i64, RouteProfile, String)>) {
        for (id, incoming, url_key) in updated {
            let slot = if id > 0 {
                self.routes.iter_mut().find(|r| r.id == id)
            } else {
                self.routes
                    .iter_mut()
                    .find(|r| r.is_remote && r.remote_url == url_key)
            };
            if let Some(slot) = slot {
                let name = slot.name.clone();
                let url = slot.remote_url.clone();
                let auto = slot.auto_update;
                let keep_id = slot.id;
                *slot = incoming;
                slot.id = keep_id;
                if slot.name.is_empty() {
                    slot.name = name;
                }
                slot.remote_url = url;
                slot.auto_update = auto;
                slot.is_remote = true;
            }
        }
    }
}

#[derive(Debug)]
pub enum RoutingSideEffect {
    None,
    Close,
    Commit,
    CopyClipboard(String),
    TryClipboardImport,
    ImportText(String),
    UpdateRemotes(Vec<RouteProfile>),
    FetchRemote { url: String, apply: bool },
    WarpGenerate,
}

// Fix Default for RoutingNested take
impl Default for RouteEditorDraft {
    fn default() -> Self {
        Self::from_profile(None, RouteProfile::new(-1, ""))
    }
}
impl Default for RawEditorDraft {
    fn default() -> Self {
        Self {
            edit_idx: None,
            name: String::new(),
            raw_route: String::new(),
            prevent_modifications: false,
            focus: ReFocus::Name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use throne_domain::AppState;

    fn draft_with_settings() -> RoutingDraft {
        let state = AppState::default();
        RoutingDraft::from_state(&state)
    }

    #[test]
    fn dns_fields_accept_typed_text_after_focus() {
        let mut d = draft_with_settings();
        d.tab = RoutingTab::Dns;
        d.focus = RtFocus::LocalOverride;
        d.settings.core_box_underlying_dns.clear();
        d.handle_key(Some("223"), false);
        d.handle_key(Some("."), false);
        d.handle_key(Some("5"), false);
        d.handle_key(Some("."), false);
        d.handle_key(Some("5"), false);
        d.handle_key(Some("."), false);
        d.handle_key(Some("5"), false);
        assert_eq!(d.settings.core_box_underlying_dns, "223.5.5.5");
        d.handle_key(None, true); // pop last '5'
        assert_eq!(d.settings.core_box_underlying_dns, "223.5.5.");
        d.handle_key(None, true); // pop '.'
        assert_eq!(d.settings.core_box_underlying_dns, "223.5.5");
    }

    #[test]
    fn dns_tab_auto_focuses_remote_and_accepts_input() {
        let mut d = draft_with_settings();
        let _ = d.apply_event(RoutingEvent::SetTab(RoutingTab::Dns));
        assert_eq!(d.focus, RtFocus::RemoteDns);
        d.settings.remote_dns.clear();
        d.handle_key(Some("8.8.8.8"), false);
        assert_eq!(d.settings.remote_dns, "8.8.8.8");
    }

    #[test]
    fn remote_dns_preset_cycles_upstream_list() {
        let mut d = draft_with_settings();
        d.settings.remote_dns = "8.8.8.8".into();
        d.cycle("remote_dns_preset");
        assert_eq!(d.settings.remote_dns, "1.1.1.1");
        d.cycle("dns_final_out");
        assert_eq!(d.settings.dns_final_out, "direct");
        d.cycle("dns_final_out");
        assert_eq!(d.settings.dns_final_out, "remote");
    }
}
