//! Real gpui-component [`InputState`] entities for dialog forms.
//!
//! While a dialog is open, these entities own the editable text. Save handlers
//! read via [`InputState::value`]; the legacy fake-caret keyboard path is
//! skipped when [`DialogInputs`] is active.

use gpui::{App, AppContext, Context, Entity, Window};
use gpui_component::input::InputState;

use crate::ui::route_completion::{dns_rule_completion_provider, simple_rule_completion_provider};
use throne_domain::{AppSettings, AppState};

/// Active dialog text fields backed by gpui-component Input.
#[derive(Clone)]
pub enum DialogInputs {
    Basic {
        inbound_address: Entity<InputState>,
        inbound_port: Entity<InputState>,
        test_url: Entity<InputState>,
        remote_dns: Entity<InputState>,
        direct_dns: Entity<InputState>,
        log_level: Entity<InputState>,
        user_agent: Entity<InputState>,
        sub_custom_hwid: Entity<InputState>,
        sub_auto_minutes: Entity<InputState>,
        route_auto_minutes: Entity<InputState>,
    },
    EditProfile {
        name: Entity<InputState>,
    },
    Tun {
        mtu: Entity<InputState>,
    },
    AddFromInput {
        text: Entity<InputState>,
    },
    /// Edit Group form (name + subscription URL).
    EditGroup {
        name: Entity<InputState>,
        url: Entity<InputState>,
    },
    /// Main Routing tabs (DNS / Warp / Hijack) + optional nested editor Inputs.
    Routing(RoutingInputs),
}

/// Nested route-editor / raw / import fields (created when nested opens).
#[derive(Clone)]
pub enum NestedInputs {
    /// Structured route editor: Name, remote URL, simple rules, advanced rule attrs.
    RouteEditor(RouteEditorInputs),
    /// Raw route editor: Name + sing-box route JSON.
    RawEditor(RawEditorInputs),
    /// Import paste multi-line.
    ImportPaste {
        text: Entity<InputState>,
    },
}

/// Real Inputs for the raw route profile editor.
#[derive(Clone)]
pub struct RawEditorInputs {
    pub name: Entity<InputState>,
    pub json: Entity<InputState>,
}

/// Real Inputs for the structured route profile editor.
#[derive(Clone)]
pub struct RouteEditorInputs {
    pub name: Entity<InputState>,
    pub url: Entity<InputState>,
    pub simple_direct: Entity<InputState>,
    pub simple_proxy: Entity<InputState>,
    pub simple_block: Entity<InputState>,
    pub simple_warp: Entity<InputState>,
    /// Advanced tab — bound to the currently selected rule (empty when none).
    pub rule_name: Entity<InputState>,
    pub rule_protocol: Entity<InputState>,
    pub rule_domain: Entity<InputState>,
    pub rule_suffix: Entity<InputState>,
    pub rule_ip: Entity<InputState>,
}

impl NestedInputs {
    pub fn route_editor<V: 'static>(
        window: &mut Window,
        cx: &mut Context<V>,
        name: &str,
        url: &str,
        simple_direct: &str,
        simple_proxy: &str,
        simple_block: &str,
        simple_warp: &str,
    ) -> Self {
        Self::RouteEditor(RouteEditorInputs {
            name: single_line(window, cx, name.to_string(), "Profile name"),
            url: single_line(window, cx, url.to_string(), "https://…"),
            simple_direct: multi_line_simple_rules(
                window,
                cx,
                simple_direct.to_string(),
                "domain: / suffix: / …",
                5,
            ),
            simple_proxy: multi_line_simple_rules(
                window,
                cx,
                simple_proxy.to_string(),
                "domain: / suffix: / …",
                5,
            ),
            simple_block: multi_line_simple_rules(
                window,
                cx,
                simple_block.to_string(),
                "domain: / suffix: / …",
                5,
            ),
            simple_warp: multi_line_simple_rules(
                window,
                cx,
                simple_warp.to_string(),
                "domain: / suffix: / …",
                5,
            ),
            rule_name: single_line(window, cx, String::new(), "Rule name"),
            rule_protocol: single_line(window, cx, String::new(), "tcp / udp / …"),
            rule_domain: multi_line(window, cx, String::new(), "one domain per line", 3),
            rule_suffix: multi_line(window, cx, String::new(), "one suffix per line", 3),
            rule_ip: multi_line(window, cx, String::new(), "one CIDR per line", 3),
        })
    }

    pub fn raw_editor<V: 'static>(
        window: &mut Window,
        cx: &mut Context<V>,
        name: &str,
        json: &str,
    ) -> Self {
        Self::RawEditor(RawEditorInputs {
            name: single_line(window, cx, name.to_string(), "Profile name"),
            json: multi_line(
                window,
                cx,
                json.to_string(),
                "{\n  \"rules\": []\n}",
                12,
            ),
        })
    }

    pub fn import_paste<V: 'static>(window: &mut Window, cx: &mut Context<V>, text: &str) -> Self {
        Self::ImportPaste {
            text: multi_line(
                window,
                cx,
                text.to_string(),
                "Paste share link / base64 / JSON…",
                8,
            ),
        }
    }

    pub fn as_route_editor(&self) -> Option<&RouteEditorInputs> {
        match self {
            Self::RouteEditor(r) => Some(r),
            _ => None,
        }
    }

    pub fn as_raw_editor(&self) -> Option<&RawEditorInputs> {
        match self {
            Self::RawEditor(r) => Some(r),
            _ => None,
        }
    }

    pub fn as_import_paste(&self) -> Option<&Entity<InputState>> {
        match self {
            Self::ImportPaste { text } => Some(text),
            _ => None,
        }
    }
}

/// Input entities for Routing Settings main tabs + optional nested editor Inputs.
#[derive(Clone)]
pub struct RoutingInputs {
    pub remote_dns: Entity<InputState>,
    pub direct_dns: Entity<InputState>,
    pub local_override: Entity<InputState>,
    pub cache_cap: Entity<InputState>,
    pub dns_object: Entity<InputState>,
    pub dns_v4: Entity<InputState>,
    pub dns_v6: Entity<InputState>,
    pub dns_port: Entity<InputState>,
    pub redirect_addr: Entity<InputState>,
    pub redirect_port: Entity<InputState>,
    pub dns_rules: Entity<InputState>,
    pub warp_ep: Entity<InputState>,
    pub warp_priv: Entity<InputState>,
    pub warp_pub: Entity<InputState>,
    pub warp_addrs: Entity<InputState>,
    pub warp_reserved: Entity<InputState>,
    /// Nested route editor / raw / import Inputs (None when no nested form open).
    pub nested: Option<NestedInputs>,
}

impl RoutingInputs {
    pub fn from_settings<V: 'static>(
        window: &mut Window,
        cx: &mut Context<V>,
        s: &AppSettings,
    ) -> Self {
        let dns_rules = s.dns_server_rules.join("\n");
        Self {
            remote_dns: single_line(window, cx, s.remote_dns.clone(), "tls://8.8.8.8"),
            direct_dns: single_line(window, cx, s.direct_dns.clone(), "localhost"),
            local_override: single_line(
                window,
                cx,
                s.core_box_underlying_dns.clone(),
                "macOS Tun: e.g. 223.5.5.5",
            ),
            cache_cap: single_line(window, cx, s.dns_cache_capacity.to_string(), "65536"),
            dns_object: multi_line(window, cx, s.dns_object.clone(), "sing-box dns JSON…", 6),
            dns_v4: single_line(window, cx, s.dns_v4_resp.clone(), "IPv4 response"),
            dns_v6: single_line(window, cx, s.dns_v6_resp.clone(), "IPv6 response"),
            dns_port: single_line(window, cx, s.dns_server_listen_port.to_string(), "Port"),
            redirect_addr: single_line(
                window,
                cx,
                s.redirect_listen_address.clone(),
                "Listen address",
            ),
            redirect_port: single_line(
                window,
                cx,
                s.redirect_listen_port.to_string(),
                "Port",
            ),
            dns_rules: multi_line_dns_rules(
                window,
                cx,
                dns_rules,
                "domain: / suffix: / regex: / ruleset:",
                4,
            ),
            warp_ep: single_line(window, cx, s.warp_ep.clone(), "Endpoint"),
            warp_priv: single_line(window, cx, s.warp_private_key.clone(), "Private key"),
            warp_pub: single_line(window, cx, s.warp_public_key.clone(), "Public key"),
            warp_addrs: single_line(window, cx, s.warp_ifc_addrs.join(","), "addr1,addr2"),
            warp_reserved: single_line(window, cx, s.warp_reserved.join(","), "0,0,0"),
            nested: None,
        }
    }

    pub fn clear_nested(&mut self) {
        self.nested = None;
    }

    pub fn set_nested(&mut self, nested: NestedInputs) {
        self.nested = Some(nested);
    }

    /// Write current Input values into settings (call before commit).
    pub fn apply_to_settings(&self, s: &mut AppSettings, cx: &App) {
        s.remote_dns = DialogInputs::read_string(&self.remote_dns, cx);
        s.direct_dns = DialogInputs::read_string(&self.direct_dns, cx);
        s.core_box_underlying_dns = DialogInputs::read_string(&self.local_override, cx);
        s.dns_cache_capacity = DialogInputs::read_string(&self.cache_cap, cx)
            .parse()
            .unwrap_or(s.dns_cache_capacity);
        s.dns_object = DialogInputs::read_string(&self.dns_object, cx);
        s.dns_v4_resp = DialogInputs::read_string(&self.dns_v4, cx);
        s.dns_v6_resp = DialogInputs::read_string(&self.dns_v6, cx);
        s.dns_server_listen_port = DialogInputs::read_string(&self.dns_port, cx)
            .parse()
            .unwrap_or(s.dns_server_listen_port);
        s.redirect_listen_address = DialogInputs::read_string(&self.redirect_addr, cx);
        s.redirect_listen_port = DialogInputs::read_string(&self.redirect_port, cx)
            .parse()
            .unwrap_or(s.redirect_listen_port);
        let rules = DialogInputs::read_string(&self.dns_rules, cx);
        s.dns_server_rules = rules
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        s.warp_ep = DialogInputs::read_string(&self.warp_ep, cx);
        s.warp_private_key = DialogInputs::read_string(&self.warp_priv, cx);
        s.warp_public_key = DialogInputs::read_string(&self.warp_pub, cx);
        s.warp_ifc_addrs = split_csv(&DialogInputs::read_string(&self.warp_addrs, cx));
        s.warp_reserved = split_csv(&DialogInputs::read_string(&self.warp_reserved, cx));
    }
}

fn split_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn single_line<V: 'static>(
    window: &mut Window,
    cx: &mut Context<V>,
    value: impl Into<String>,
    placeholder: &'static str,
) -> Entity<InputState> {
    let value = value.into();
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder)
            .default_value(value)
    })
}

fn multi_line<V: 'static>(
    window: &mut Window,
    cx: &mut Context<V>,
    value: impl Into<String>,
    placeholder: &'static str,
    rows: usize,
) -> Entity<InputState> {
    let value = value.into();
    cx.new(|cx| {
        InputState::new(window, cx)
            .multi_line(true)
            .rows(rows)
            .placeholder(placeholder)
            .default_value(value)
    })
}

/// Multi-line Input with upstream-style simple-rule autocomplete.
fn multi_line_simple_rules<V: 'static>(
    window: &mut Window,
    cx: &mut Context<V>,
    value: impl Into<String>,
    placeholder: &'static str,
    rows: usize,
) -> Entity<InputState> {
    let value = value.into();
    let provider = simple_rule_completion_provider();
    cx.new(|cx| {
        let mut state = InputState::new(window, cx)
            .multi_line(true)
            .rows(rows)
            .placeholder(placeholder)
            .default_value(value);
        state.lsp.completion_provider = Some(provider);
        state
    })
}

/// Multi-line Input with DNS-rules autocomplete (domain/suffix/regex/ruleset).
fn multi_line_dns_rules<V: 'static>(
    window: &mut Window,
    cx: &mut Context<V>,
    value: impl Into<String>,
    placeholder: &'static str,
    rows: usize,
) -> Entity<InputState> {
    let value = value.into();
    let provider = dns_rule_completion_provider();
    cx.new(|cx| {
        let mut state = InputState::new(window, cx)
            .multi_line(true)
            .rows(rows)
            .placeholder(placeholder)
            .default_value(value);
        state.lsp.completion_provider = Some(provider);
        state
    })
}

impl DialogInputs {
    pub fn basic<V: 'static>(
        window: &mut Window,
        cx: &mut Context<V>,
        state: &AppState,
    ) -> Self {
        let s = state.settings();
        Self::Basic {
            inbound_address: single_line(
                window,
                cx,
                s.inbound_address.clone(),
                "Inbound address",
            ),
            inbound_port: single_line(
                window,
                cx,
                s.inbound_socks_port.to_string(),
                "Port",
            ),
            test_url: single_line(window, cx, s.test_latency_url.clone(), "Test URL"),
            remote_dns: single_line(window, cx, s.remote_dns.clone(), "Remote DNS"),
            direct_dns: single_line(window, cx, s.direct_dns.clone(), "Direct DNS"),
            log_level: single_line(window, cx, s.log_level.clone(), "Log level"),
            user_agent: single_line(
                window,
                cx,
                s.user_agent.clone(),
                "Throne/<version> (default when empty)",
            ),
            sub_custom_hwid: single_line(
                window,
                cx,
                s.sub_custom_hwid_params.clone(),
                "hwid=…,os=…,osVersion=…,model=…",
            ),
            sub_auto_minutes: single_line(
                window,
                cx,
                s.sub_auto_update_minutes().to_string(),
                "30",
            ),
            route_auto_minutes: single_line(
                window,
                cx,
                s.route_auto_update_minutes().to_string(),
                "1440",
            ),
        }
    }

    pub fn edit_profile<V: 'static>(
        window: &mut Window,
        cx: &mut Context<V>,
        name: &str,
    ) -> Self {
        Self::EditProfile {
            name: single_line(window, cx, name.to_string(), "Profile name"),
        }
    }

    pub fn tun<V: 'static>(window: &mut Window, cx: &mut Context<V>, mtu: &str) -> Self {
        Self::Tun {
            mtu: single_line(window, cx, mtu.to_string(), "MTU"),
        }
    }

    pub fn add_from_input<V: 'static>(window: &mut Window, cx: &mut Context<V>) -> Self {
        Self::AddFromInput {
            text: multi_line(
                window,
                cx,
                String::new(),
                "Paste share link(s) / YAML / JSON…",
                8,
            ),
        }
    }

    pub fn edit_group<V: 'static>(
        window: &mut Window,
        cx: &mut Context<V>,
        name: &str,
        url: &str,
    ) -> Self {
        Self::EditGroup {
            name: single_line(window, cx, name.to_string(), "Group name"),
            url: single_line(window, cx, url.to_string(), "https://…"),
        }
    }

    pub fn routing<V: 'static>(
        window: &mut Window,
        cx: &mut Context<V>,
        settings: &AppSettings,
    ) -> Self {
        Self::Routing(RoutingInputs::from_settings(window, cx, settings))
    }

    pub fn as_routing(&self) -> Option<&RoutingInputs> {
        match self {
            Self::Routing(r) => Some(r),
            _ => None,
        }
    }

    pub fn read_string(entity: &Entity<InputState>, cx: &App) -> String {
        entity.read(cx).value().to_string()
    }

    pub fn set_string(
        entity: &Entity<InputState>,
        value: impl Into<String>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let value = value.into();
        entity.update(cx, |input, cx| {
            input.set_value(value, window, cx);
        });
    }
}
