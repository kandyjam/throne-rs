//! Build a minimal sing-box core config from a Throne profile + settings.
//!
//! This is intentionally smaller than upstream `BuildSingBoxConfig` but enough
//! to make mixed-inbound proxy actually work for common outbound types.

use serde_json::{Map, Value, json};
use throne_domain::{
    AppSettings, AutoSelectorConfig, DefaultOutbound, ParsedOutbound, Profile, ProfileType,
    RouteProfile, RouteRule, RulesetMirror, is_xray_full_config_member,
};

use crate::CoreError;

/// Result of building a core load request payload (maps to `LoadConfigReq`, 1.2.4 fields).
#[derive(Debug, Clone, Default)]
pub struct BuiltConfig {
    pub core_config_json: String,
    pub need_xray: bool,
    pub xray_config: String,
    /// Upstream `LoadConfigReq.tun_ipv4_cidr` — set when Tun inbound is present.
    /// Empty when Tun is off. Core uses this on Darwin to set system DNS to tunIP+1.
    pub tun_ipv4_cidr: String,
    /// Opaque full Xray configs, each its own gated instance (proto field 16).
    pub xray_full_configs: Vec<String>,
    /// Keep the shared Xray sidecar cold until dialed (auto-selector pools).
    pub xray_lazy_start: bool,
    /// Idle seconds for the shared sidecar; 0 keeps resident once started.
    pub xray_idle_seconds: i32,
    /// Idle seconds for full-config gates; 0 keeps resident (1.2.4 auto-selector).
    pub xray_full_idle_seconds: i32,
    /// Loopback DNS-in for Xray outbound resolution (e.g. `127.0.0.1:15353`).
    pub xray_outbound_dns_address: String,
    pub xray_outbound_dns_strategy: String,
}

/// Optional Auto Selector expansion for Start.
///
/// When set, `members` are emitted as `p{id}` outbounds and a group outbound
/// tagged `proxy` selects among them (urltest, or sticky best when only one).
#[derive(Debug, Clone)]
pub struct AutoSelectorBuild {
    pub config: AutoSelectorConfig,
    pub members: Vec<Profile>,
}

/// Build sing-box JSON for starting `profile` with the given settings.
///
/// When `route_profile` is set (e.g. "Bypass China"), its rules and remote
/// `rule_set` binaries are compiled into `route.rules` / `route.rule_set`.
pub fn build_load_config(
    profile: &Profile,
    settings: &AppSettings,
    route_profile: Option<&RouteProfile>,
) -> Result<BuiltConfig, CoreError> {
    build_load_config_ex(profile, settings, route_profile, None)
}

/// Like [`build_load_config`], with optional Auto Selector member expansion.
pub fn build_load_config_ex(
    profile: &Profile,
    settings: &AppSettings,
    route_profile: Option<&RouteProfile>,
    auto_selector: Option<&AutoSelectorBuild>,
) -> Result<BuiltConfig, CoreError> {
    let mut xray_full_configs = Vec::new();
    let mut xray_lazy_start = false;
    let mut xray_idle_seconds = 0;
    let mut xray_full_idle_seconds = 0;

    let (proxy_outbound, extra_outbounds, proxy_direct_domains) =
        if let Some(auto) = auto_selector {
            let built = build_auto_selector_outbounds(auto, settings)?;
            xray_full_configs = built.xray_full_configs;
            // Pool Xray members may be probe-only: keep sidecar cold between dials.
            // Idle must outlast the probe interval or it restarts every round.
            if built.any_xray_member || !xray_full_configs.is_empty() {
                xray_lazy_start = true;
                xray_idle_seconds = auto.config.interval_sec.max(60) * 2;
                if xray_idle_seconds < 120 {
                    xray_idle_seconds = 120;
                }
                // Resident on purpose: recycling would put an instance build in
                // front of every failover (upstream 1.2.4).
                xray_full_idle_seconds = 0;
            }
            (built.group, built.member_outbounds, built.domains)
        } else if profile.profile_type == ProfileType::AutoSelector {
            return Err(CoreError::Config(
                "Auto Selector requires resolved members — call resolve_auto_selector_members first"
                    .into(),
            ));
        } else if is_xray_full_config_member(profile) {
            let bridge = build_xray_full_member(profile)?;
            xray_full_configs.push(bridge.xray_config);
            let domains = collect_outbound_server_domains(profile);
            (bridge.socks_outbound, Vec::new(), domains)
        } else {
            let outbound = build_proxy_outbound(profile)?;
            let domains = collect_outbound_server_domains(profile);
            (outbound, Vec::new(), domains)
        };
    // Upstream `buildInboundSection`: mixed inbound listen = settings.inbound_address
    // as-is. Tray "Allow other devices to connect" sets `::` (or user sets
    // `0.0.0.0`); system-proxy *clients* still use loopback via `proxy_client_host`.
    let listen = normalize_listen_address(&settings.inbound_address);
    let port = settings.inbound_socks_port.clamp(1, 65535) as u16;
    let log_level = if settings.log_level.trim().is_empty() {
        "info"
    } else {
        settings.log_level.trim()
    };

    // Proven-working DNS for mixed+system-proxy on restricted networks (bisected):
    // default_domain_resolver MUST be `local` (OS resolver).
    let mut inbounds = vec![json!({
        "type": "mixed",
        "tag": "mixed-in",
        "listen": listen,
        "listen_port": port
    })];
    // TUN inbound when toolbar Tun is on (needs privileges on macOS/Linux).
    // Mirrors upstream `buildInboundSection` (generate.cpp).
    let mut tun_ipv4_cidr = String::new();
    let outbound = proxy_outbound;
    if settings.tun_mode_enabled {
        // Upstream buildDNSSection: Darwin Tun requires core_box_underlying_dns.
        validate_darwin_tun_underlying_dns(settings)?;
        tun_ipv4_cidr = normalize_tun_ipv4_cidr(&settings.vpn_tun_ipv4_cidr);
        // Platform stack defaults match upstream SettingsRepo (macOS → gvisor).
        let stack = default_tun_stack();
        let mut tun = json!({
            "type": "tun",
            "tag": "tun-in",
            "auto_route": true,
            "strict_route": settings.vpn_strict_route,
            "mtu": settings.vpn_mtu.clamp(1280, 65535),
            "stack": stack,
            "address": [tun_ipv4_cidr.clone()]
        });
        // Upstream genTunName(): macOS "" (omit); else "throne-tun".
        if let Some(name) = default_tun_interface_name() {
            tun.as_object_mut()
                .unwrap()
                .insert("interface_name".into(), json!(name));
        }
        // Upstream: Linux + vpn_auto_redirect (default true).
        #[cfg(target_os = "linux")]
        {
            tun.as_object_mut()
                .unwrap()
                .insert("auto_redirect".into(), json!(true));
        }
        // Upstream route_exclude_address (1.2.3+ / #1738 macOS DNS fix):
        // - loopback + broadcast always excluded
        // - private LAN ranges excluded when bypass is enabled
        // - on Darwin, punch a hole for the Tun subnet itself: system DNS is
        //   repointed at tunIP+1 (inside that subnet), and excluding the whole
        //   172.16.0.0/12 would black-hole every DNS query.
        if !settings.disable_private_range_bypass {
            let excludes = build_tun_route_exclude_addrs(&tun_ipv4_cidr);
            tun.as_object_mut()
                .unwrap()
                .insert("route_exclude_address".into(), json!(excludes));
        }
        inbounds.push(tun);
    }

    let (mut route_rules, mut rule_sets, route_final) =
        compile_route_section(route_profile, settings.ruleset_mirror);

    // Upstream: optional adblock remote rule-set + reject rule.
    if settings.adblock_enable {
        let adblock_url = apply_ruleset_mirror(
            "https://raw.githubusercontent.com/217heidai/adblockfilters/main/rules/adblocksingbox.srs",
            settings.ruleset_mirror,
        );
        rule_sets.push(json!({
            "type": "remote",
            "tag": "throne-adblocksingbox",
            "format": "binary",
            "url": adblock_url,
            "download_detour": "direct"
        }));
        route_rules.push(json!({
            "rule_set": ["throne-adblocksingbox"],
            "action": "reject"
        }));
    }

    let (dns, default_resolver_tag) =
        build_dns_section(settings, settings.tun_mode_enabled, &proxy_direct_domains)?;

    // Upstream buildRouteSection: default_domain_resolver = dns-direct (+ strategy);
    // auto_detect_interface only when Tun is on.
    let domain_strategy = if settings.default_domain_strategy.trim().is_empty() {
        "prefer_ipv4"
    } else {
        settings.default_domain_strategy.trim()
    };
    let mut route = json!({
        "rules": route_rules,
        "rule_set": rule_sets,
        "final": route_final,
        "default_domain_resolver": {
            "server": default_resolver_tag,
            "strategy": domain_strategy
        }
    });
    if settings.tun_mode_enabled {
        route
            .as_object_mut()
            .unwrap()
            .insert("auto_detect_interface".into(), json!(true));
    }

    let mut outbounds = extra_outbounds;
    outbounds.push(outbound);
    outbounds.push(json!({ "type": "direct", "tag": "direct" }));

    let config = json!({
        "log": { "level": log_level, "timestamp": true },
        "dns": dns,
        "inbounds": inbounds,
        "outbounds": outbounds,
        "route": route,
        // clash_api enables TrafficManager used by QueryStats / QueryConnections.
        // Upstream cache_file also sets store_fakeip / store_rdrc when applicable.
        "experimental": {
            "clash_api": {
                "external_controller": "127.0.0.1:0",
                "default_mode": ""
            },
            "cache_file": {
                "enabled": true,
                "store_fakeip": true,
                "store_rdrc": true
            }
        }
    });

    let core_config_json =
        serde_json::to_string(&config).map_err(|e| CoreError::Config(e.to_string()))?;

    // When any Xray path is used, point outbound DNS at sing-box loopback DNS-in
    // (upstream mainwindow_profile_lifecycle 1.2.4). Default port matches
    // SettingsRepo::core_dns_in_port when the GUI has not exposed it yet.
    let need_xray_dns = !xray_full_configs.is_empty() || xray_lazy_start;
    let (xray_outbound_dns_address, xray_outbound_dns_strategy) = if need_xray_dns {
        (
            "127.0.0.1:15353".to_string(),
            xray_outbound_domain_strategy(settings),
        )
    } else {
        (String::new(), String::new())
    };

    Ok(BuiltConfig {
        core_config_json,
        need_xray: false,
        xray_config: String::new(),
        tun_ipv4_cidr,
        xray_full_configs,
        xray_lazy_start,
        xray_idle_seconds,
        xray_full_idle_seconds,
        xray_outbound_dns_address,
        xray_outbound_dns_strategy,
    })
}

fn xray_outbound_domain_strategy(settings: &AppSettings) -> String {
    let s = settings.default_domain_strategy.trim();
    match s {
        "" | "prefer_ipv4" => "UseIPv4".into(),
        "prefer_ipv6" => "UseIPv6".into(),
        "ipv4_only" => "UseIPv4".into(),
        "ipv6_only" => "UseIPv6".into(),
        other => other.to_string(),
    }
}

/// Build route.rules + route.rule_set + final outbound tag from a Throne route profile.
fn compile_route_section(
    route_profile: Option<&RouteProfile>,
    mirror: RulesetMirror,
) -> (Vec<Value>, Vec<Value>, &'static str) {
    // Always inject sniff + DNS hijack first (mandatory in modern sing-box).
    let mut rules = vec![
        json!({ "action": "sniff" }),
        json!({ "protocol": "dns", "action": "hijack-dns" }),
    ];
    let mut needed_sets: Vec<String> = Vec::new();
    let mut saw_private = false;

    if let Some(rp) = route_profile {
        if rp.is_raw && !rp.raw_route.trim().is_empty() {
            if let Ok(raw) = serde_json::from_str::<Value>(&rp.raw_route) {
                if let Some(arr) = raw.get("rules").and_then(|r| r.as_array()) {
                    for r in arr {
                        rules.push(r.clone());
                    }
                }
                if let Some(arr) = raw.get("rule_set").and_then(|r| r.as_array()) {
                    // Re-mirror remote URLs so user mirror setting still applies.
                    let sets: Vec<Value> = arr
                        .iter()
                        .map(|rs| remap_raw_rule_set_url(rs, mirror))
                        .collect();
                    let fin = route_final_tag(rp.default_outbound);
                    return (rules, sets, fin);
                }
            }
        } else {
            for rule in &rp.rules {
                if rule.ip_is_private {
                    saw_private = true;
                }
                for rs in &rule.rule_set {
                    if !rs.is_empty() && !needed_sets.iter().any(|x| x == rs) {
                        needed_sets.push(rs.clone());
                    }
                }
                if let Some(j) = route_rule_to_json(rule) {
                    // Skip duplicate bare hijack-dns (we already injected one).
                    if is_bare_hijack_dns(&j) {
                        continue;
                    }
                    rules.push(j);
                }
            }
        }
        if !saw_private {
            // Ensure LAN stays direct even if profile omitted it.
            rules.insert(
                2,
                json!({
                    "ip_is_private": true,
                    "action": "route",
                    "outbound": "direct"
                }),
            );
        }
        let sets = needed_sets
            .iter()
            .filter_map(|tag| remote_rule_set_entry(tag, mirror))
            .collect();
        return (rules, sets, route_final_tag(rp.default_outbound));
    }

    // No route profile: minimal safe defaults.
    rules.push(json!({
        "ip_is_private": true,
        "action": "route",
        "outbound": "direct"
    }));
    (rules, Vec::new(), "proxy")
}

fn remap_raw_rule_set_url(rs: &Value, mirror: RulesetMirror) -> Value {
    let mut out = rs.clone();
    if let Some(obj) = out.as_object_mut() {
        if let Some(url) = obj.get("url").and_then(|u| u.as_str()) {
            obj.insert("url".into(), json!(apply_ruleset_mirror(url, mirror)));
        }
        obj.entry("download_detour".to_string())
            .or_insert_with(|| json!("direct"));
    }
    out
}

fn route_final_tag(d: DefaultOutbound) -> &'static str {
    match d {
        DefaultOutbound::Direct => "direct",
        DefaultOutbound::Block => "direct", // no block outbound; reject via rules
        DefaultOutbound::WarpBypass => "proxy",
        DefaultOutbound::Proxy | DefaultOutbound::Profile(_) => "proxy",
    }
}

/// True when the rule is only `{action:hijack-dns}` or `{protocol:dns,action:hijack-dns}`.
fn is_bare_hijack_dns(j: &Value) -> bool {
    if j.get("action").and_then(|a| a.as_str()) != Some("hijack-dns") {
        return false;
    }
    let Some(obj) = j.as_object() else {
        return false;
    };
    // Allow only action (+ optional protocol=dns). Any extra match field keeps the rule.
    for (k, v) in obj {
        match k.as_str() {
            "action" => {}
            "protocol" if v.as_str() == Some("dns") => {}
            _ => return false,
        }
    }
    true
}

fn outbound_tag_for_id(id: i64) -> &'static str {
    match DefaultOutbound::from_id(id) {
        DefaultOutbound::Direct => "direct",
        DefaultOutbound::Block => "direct",
        DefaultOutbound::WarpBypass => "proxy",
        DefaultOutbound::Proxy | DefaultOutbound::Profile(_) => "proxy",
    }
}

/// Convert one Throne `RouteRule` into a sing-box route rule object.
fn route_rule_to_json(rule: &RouteRule) -> Option<Value> {
    let mut obj = Map::new();
    let action = if rule.action.trim().is_empty() {
        "route"
    } else {
        rule.action.trim()
    };

    // Match conditions
    insert_str_list(&mut obj, "domain", &rule.domain);
    insert_str_list(&mut obj, "domain_suffix", &rule.domain_suffix);
    insert_str_list(&mut obj, "domain_keyword", &rule.domain_keyword);
    insert_str_list(&mut obj, "domain_regex", &rule.domain_regex);
    insert_str_list(&mut obj, "ip_cidr", &rule.ip_cidr);
    insert_str_list(&mut obj, "source_ip_cidr", &rule.source_ip_cidr);
    insert_str_list(&mut obj, "process_name", &rule.process_name);
    insert_str_list(&mut obj, "process_path", &rule.process_path);
    insert_str_list(&mut obj, "process_path_regex", &rule.process_path_regex);
    // Normalize rule_set tags so they match remote_rule_set_entry tags.
    {
        let tags: Vec<String> = rule
            .rule_set
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(rule_set_tag_name)
            .collect();
        if !tags.is_empty() {
            obj.insert("rule_set".into(), json!(tags));
        }
    }
    insert_str_list(&mut obj, "inbound", &rule.inbound);
    insert_str_list(&mut obj, "port", &rule.port);
    insert_str_list(&mut obj, "port_range", &rule.port_range);
    insert_str_list(&mut obj, "source_port", &rule.source_port);
    insert_str_list(&mut obj, "source_port_range", &rule.source_port_range);
    insert_str_list(&mut obj, "wifi_ssid", &rule.wifi_ssid);
    insert_str_list(&mut obj, "wifi_bssid", &rule.wifi_bssid);

    if rule.ip_is_private {
        obj.insert("ip_is_private".into(), json!(true));
    }
    if rule.source_ip_is_private {
        obj.insert("source_ip_is_private".into(), json!(true));
    }
    if rule.invert {
        obj.insert("invert".into(), json!(true));
    }
    if !rule.network.is_empty() {
        // network may be "tcp" / "udp" or JSON array string — keep as string or split
        if rule.network.contains(',') {
            let parts: Vec<&str> = rule.network.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
            obj.insert("network".into(), json!(parts));
        } else {
            obj.insert("network".into(), json!(rule.network));
        }
    }
    if !rule.protocol.is_empty() {
        obj.insert("protocol".into(), json!(rule.protocol));
    }
    if !rule.ip_version.is_empty() {
        obj.insert("ip_version".into(), json!(rule.ip_version));
    }

    // Action
    match action {
        "hijack-dns" => {
            if !obj.contains_key("protocol") {
                obj.insert("protocol".into(), json!("dns"));
            }
            obj.insert("action".into(), json!("hijack-dns"));
        }
        "reject" | "block" => {
            obj.insert("action".into(), json!("reject"));
            if !rule.reject_method.is_empty() {
                obj.insert("method".into(), json!(rule.reject_method));
            }
            if rule.no_drop {
                obj.insert("no_drop".into(), json!(true));
            }
        }
        "sniff" => {
            obj.insert("action".into(), json!("sniff"));
            if rule.sniff_override_dest {
                obj.insert("override_destination".into(), json!(true));
            }
        }
        _ => {
            // route (default)
            if matches!(DefaultOutbound::from_id(rule.outbound_id), DefaultOutbound::Block) {
                obj.insert("action".into(), json!("reject"));
            } else {
                obj.insert("action".into(), json!("route"));
                obj.insert(
                    "outbound".into(),
                    json!(outbound_tag_for_id(rule.outbound_id)),
                );
            }
        }
    }

    // Empty condition + bare route to proxy is a no-op final; drop noise.
    let keys = obj.len();
    if keys <= 2
        && obj.get("action").and_then(|a| a.as_str()) == Some("route")
        && obj.get("outbound").and_then(|a| a.as_str()) == Some("proxy")
        && !obj.contains_key("rule_set")
        && !obj.contains_key("domain")
        && !obj.contains_key("domain_suffix")
        && !obj.contains_key("ip_cidr")
        && !obj.contains_key("ip_is_private")
    {
        return None;
    }

    Some(Value::Object(obj))
}

fn insert_str_list(obj: &mut Map<String, Value>, key: &str, vals: &[String]) {
    let cleaned: Vec<&str> = vals
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if !cleaned.is_empty() {
        obj.insert(key.into(), json!(cleaned));
    }
}

/// Map a Throne rule-set tag to a remote `.srs` rule_set object.
fn remote_rule_set_entry(tag: &str, mirror: RulesetMirror) -> Option<Value> {
    let tag = tag.trim();
    if tag.is_empty() {
        return None;
    }
    let raw_url = if tag.starts_with("https://") || tag.starts_with("http://") {
        tag.to_string()
    } else if let Some(u) = well_known_rule_set_url(tag) {
        u
    } else {
        return None;
    };
    let url = apply_ruleset_mirror(&raw_url, mirror);
    Some(json!({
        "type": "remote",
        "tag": rule_set_tag_name(tag),
        "format": "binary",
        "url": url,
        // Download geosite without going through the proxy (bootstrap).
        "download_detour": "direct"
    }))
}

/// Rewrite `raw.githubusercontent.com/...` URLs through a jsDelivr mirror.
///
/// Mirrors upstream `Configs::get_jsdelivr_link`:
/// `https://raw.githubusercontent.com/{user}/{repo}/{ref}/{path...}`
/// → `{base}/{user}/{repo}@{ref}/{path...}`
pub fn apply_ruleset_mirror(link: &str, mirror: RulesetMirror) -> String {
    let Some(base) = mirror.jsdelivr_gh_base() else {
        return link.to_string();
    };
    const PREFIX: &str = "https://raw.githubusercontent.com/";
    let Some(rest) = link.strip_prefix(PREFIX) else {
        // Also accept http://
        let Some(rest) = link.strip_prefix("http://raw.githubusercontent.com/") else {
            return link.to_string();
        };
        return rewrite_gh_raw_path(base, rest);
    };
    rewrite_gh_raw_path(base, rest)
}

fn rewrite_gh_raw_path(base: &str, path_after_host: &str) -> String {
    let parts: Vec<&str> = path_after_host.split('/').filter(|s| !s.is_empty()).collect();
    // Expect: user / repo / ref / rest...
    if parts.len() < 3 {
        return format!("https://raw.githubusercontent.com/{path_after_host}");
    }
    let user = parts[0];
    let repo = parts[1];
    let git_ref = parts[2];
    let tail = if parts.len() > 3 {
        format!("/{}", parts[3..].join("/"))
    } else {
        String::new()
    };
    format!("{base}/{user}/{repo}@{git_ref}{tail}")
}

fn rule_set_tag_name(tag_or_url: &str) -> String {
    if let Some(name) = tag_or_url.rsplit('/').next() {
        name.trim_end_matches(".srs").to_string()
    } else {
        tag_or_url.to_string()
    }
}

/// Resolve a well-known rule-set tag to a remote `.srs` URL.
///
/// Uses the same table as upstream Qt Throne (`srslist.h` from
/// throneproj/routeprofiles). Falls back to MetaCubeX path convention for
/// unknown `geoip-*` / `geosite-*` tags, plus a few common aliases.
fn well_known_rule_set_url(tag: &str) -> Option<String> {
    let t = tag.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(u) = crate::rule_set_list::lookup_rule_set_url(t) {
        return Some(u.to_string());
    }
    // Common aliases seen in older / hand-edited profiles.
    let alias = match t {
        "geosite-anticensorship" => Some("geosite-category-anticensorship"),
        "geosite-geolocation-notcn" | "geosite-geolocation-!cn" => {
            Some("geosite-geolocation-!cn")
        }
        _ => None,
    };
    if let Some(a) = alias {
        if let Some(u) = crate::rule_set_list::lookup_rule_set_url(a) {
            return Some(u.to_string());
        }
    }
    // MetaCubeX convention (upstream default source for most tags).
    if let Some(rest) = t.strip_prefix("geosite-") {
        if !rest.is_empty() && !rest.contains('/') {
            return Some(format!(
                "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geosite/{rest}.srs"
            ));
        }
    }
    if let Some(rest) = t.strip_prefix("geoip-") {
        if !rest.is_empty() && !rest.contains('/') {
            return Some(format!(
                "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/{rest}.srs"
            ));
        }
    }
    None
}

/// Build a temporary sing-box config for batch URL-testing `profiles`.
///
/// Each profile becomes outbound tag `p{id}`; route final is `direct` so the box
/// only dials via the tested outbound tags (BatchURLTest picks them explicitly).
pub fn build_url_test_config(
    profiles: &[&Profile],
    settings: &AppSettings,
) -> Result<(String, Vec<String>), CoreError> {
    if profiles.is_empty() {
        return Err(CoreError::Config("no profiles to test".into()));
    }
    let log_level = if settings.log_level.trim().is_empty() {
        "info"
    } else {
        settings.log_level.trim()
    };
    let mut outbounds = Vec::new();
    let mut tags = Vec::new();
    for p in profiles {
        let mut ob = build_proxy_outbound(p)?;
        let tag = format!("p{}", p.id);
        if let Some(obj) = ob.as_object_mut() {
            obj.insert("tag".into(), json!(tag.clone()));
        }
        outbounds.push(ob);
        tags.push(tag);
    }
    outbounds.push(json!({ "type": "direct", "tag": "direct" }));

    let config = json!({
        "log": { "level": log_level, "timestamp": true },
        "dns": {
            "servers": [
                { "type": "local", "tag": "dns-local" },
                {
                    "type": "udp",
                    "tag": "dns-direct",
                    "server": "8.8.8.8",
                    "server_port": 53,
                    "domain_resolver": "dns-local"
                }
            ],
            "final": "dns-local",
            "strategy": "ipv4_only"
        },
        "inbounds": [],
        "outbounds": outbounds,
        "route": {
            "final": "direct",
            "auto_detect_interface": true,
            "default_domain_resolver": {
                "server": "dns-direct",
                "strategy": "ipv4_only"
            }
        }
    });
    let s = serde_json::to_string(&config).map_err(|e| CoreError::Config(e.to_string()))?;
    Ok((s, tags))
}

/// Inbound listen address. Defaults remain loopback, while an explicit IP enables LAN sharing.
/// Platform-valid TUN `interface_name`, or `None` to let the OS/core assign one.
///
/// macOS/Darwin rejects arbitrary names (`bad tun name: throne-tun`); only
/// `utunN` is valid, and omitting the field lets the kernel pick a free utun.
fn default_tun_interface_name() -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    {
        None
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some("throne-tun")
    }
}

/// Upstream `vpn_implementation` platform defaults.
fn default_tun_stack() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "gvisor"
    }
    #[cfg(target_os = "windows")]
    {
        "system"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "system"
    }
}

/// Validate / fall back Tun IPv4 CIDR (upstream default `172.19.0.1/24`).
fn normalize_tun_ipv4_cidr(raw: &str) -> String {
    let t = raw.trim();
    if is_plausible_ipv4_cidr(t) {
        t.to_string()
    } else {
        "172.19.0.1/24".into()
    }
}

/// Upstream `tunBypassablePrivateRanges` + unconditional loopback/broadcast.
///
/// On Darwin, `subtract_prefix` carves the Tun CIDR out of the exclude list so
/// system DNS at tunIP+1 stays on the Tun (upstream #1738 / 1.2.4).
fn build_tun_route_exclude_addrs(tun_ipv4_cidr: &str) -> Vec<String> {
    // Unconditional: never route loopback/broadcast into Tun.
    let mut excludes = vec!["127.0.0.0/8".into(), "255.255.255.255/32".into()];
    // Bypassable private ranges (upstream RouteProfile.h).
    let mut private = vec![
        "10.0.0.0/8".into(),
        "172.16.0.0/12".into(),
        "192.168.0.0/16".into(),
        "169.254.0.0/16".into(),
        "224.0.0.0/4".into(),
    ];
    #[cfg(target_os = "macos")]
    {
        private = subtract_ipv4_prefix(&private, tun_ipv4_cidr);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = tun_ipv4_cidr;
    }
    excludes.extend(private);
    excludes
}

/// Upstream `subtractPrefix` (IPv4 only — Tun default is IPv4).
///
/// When `hole` sits inside a range, replace that range with the complementary
/// siblings so the hole stays routed through Tun while the rest is still bypassed.
fn subtract_ipv4_prefix(ranges: &[String], hole: &str) -> Vec<String> {
    let Some(cut) = parse_ipv4_prefix(hole) else {
        return ranges.to_vec();
    };
    let mut out = Vec::new();
    for entry in ranges {
        let Some(range) = parse_ipv4_prefix(entry) else {
            out.push(entry.clone());
            continue;
        };
        // hole fully contains range → drop range
        if prefix_contains_v4(&cut, &range) {
            continue;
        }
        // range does not contain hole → keep as-is
        if !prefix_contains_v4(&range, &cut) {
            out.push(entry.clone());
            continue;
        }
        // range contains hole → emit siblings that cover range \ hole
        for bits in (range.bits + 1)..=cut.bits {
            let mut sibling = cut;
            sibling.bits = bits;
            let flipped = bits - 1;
            let byte = (flipped / 8) as usize;
            let bit = flipped % 8;
            sibling.addr[byte] ^= 0x80u8 >> bit;
            // zero host bits below `bits`
            for i in bits..32 {
                let b = (i / 8) as usize;
                let m = 0x80u8 >> (i % 8);
                sibling.addr[b] &= !m;
            }
            out.push(format_ipv4_prefix(&sibling));
        }
    }
    out
}

#[derive(Clone, Copy)]
struct Ipv4Prefix {
    addr: [u8; 4],
    bits: u8,
}

fn parse_ipv4_prefix(cidr: &str) -> Option<Ipv4Prefix> {
    let (addr, pref) = cidr.trim().split_once('/')?;
    let bits: u8 = pref.parse().ok()?;
    if bits > 32 {
        return None;
    }
    let parts: Vec<u8> = addr
        .split('.')
        .filter_map(|p| p.parse().ok())
        .collect();
    if parts.len() != 4 {
        return None;
    }
    let mut a = [parts[0], parts[1], parts[2], parts[3]];
    // Normalize network address: zero host bits
    for i in bits..32 {
        let b = (i / 8) as usize;
        let m = 0x80u8 >> (i % 8);
        a[b] &= !m;
    }
    Some(Ipv4Prefix { addr: a, bits })
}

fn format_ipv4_prefix(p: &Ipv4Prefix) -> String {
    format!("{}.{}.{}.{}/{}", p.addr[0], p.addr[1], p.addr[2], p.addr[3], p.bits)
}

fn prefix_contains_v4(outer: &Ipv4Prefix, inner: &Ipv4Prefix) -> bool {
    if outer.bits > inner.bits {
        return false;
    }
    let whole = (outer.bits / 8) as usize;
    if outer.addr[..whole] != inner.addr[..whole] {
        return false;
    }
    let rest = outer.bits % 8;
    if rest == 0 {
        return true;
    }
    let mask = 0xFFu8 << (8 - rest);
    (outer.addr[whole] & mask) == (inner.addr[whole] & mask)
}

fn is_plausible_ipv4_cidr(s: &str) -> bool {
    let Some((addr, pref)) = s.split_once('/') else {
        return false;
    };
    let Ok(prefix) = pref.parse::<u8>() else {
        return false;
    };
    if prefix > 32 {
        return false;
    }
    let parts: Vec<_> = addr.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|p| p.parse::<u8>().is_ok())
}

/// Normalize mixed-inbound listen host for sing-box.
///
/// Upstream passes `inbound_address` through unchanged. We only rewrite empty /
/// hostname placeholders so a missing value still binds loopback. Explicit
/// `::` / `0.0.0.0` (Allow LAN) and other IPs are preserved.
fn normalize_listen_address(addr: &str) -> String {
    match addr.trim() {
        "" | "localhost" => "127.0.0.1".into(),
        "*" => "0.0.0.0".into(),
        value => {
            let bare = value.trim_start_matches('[').trim_end_matches(']');
            bare.parse::<std::net::IpAddr>()
                .map(|ip| ip.to_string())
                .unwrap_or_else(|_| "127.0.0.1".into())
        }
    }
}

/// Intermediate result of expanding an Auto Selector into outbounds + full configs.
struct AutoSelectorOutbounds {
    group: Value,
    member_outbounds: Vec<Value>,
    domains: Vec<String>,
    xray_full_configs: Vec<String>,
    any_xray_member: bool,
}

/// Build member outbounds + upstream `auto-selector` group tagged `proxy`.
///
/// Matches Throne 1.2.3+ `buildAutoSelectorGroup` (generate.cpp). 1.2.4 adds
/// Xray full-config members as socks bridges + `xray_full_configs` instances.
fn build_auto_selector_outbounds(
    auto: &AutoSelectorBuild,
    settings: &AppSettings,
) -> Result<AutoSelectorOutbounds, CoreError> {
    if auto.members.is_empty() {
        return Err(CoreError::Config(
            "Auto selector produced no usable members".into(),
        ));
    }

    let mut domains = Vec::new();
    let mut member_outbounds = Vec::new();
    let mut xray_full_configs = Vec::new();
    let mut any_xray_member = false;
    let mut tags = Vec::new();
    let mut warm = Vec::new();
    let mut pinned_tag = None;
    let validity_mins = auto.config.result_validity_mins.max(0) as i64;

    for member in &auto.members {
        let tag = format!("p{}", member.id);
        let mut ob = if is_xray_full_config_member(member) {
            let bridge = build_xray_full_member(member)?;
            xray_full_configs.push(bridge.xray_config);
            any_xray_member = true;
            bridge.socks_outbound
        } else {
            // Regular Xray-backed leaves still share the sidecar; flag lazy start.
            if member_uses_xray_sidecar(member) {
                any_xray_member = true;
            }
            build_proxy_outbound(member)?
        };
        if let Some(obj) = ob.as_object_mut() {
            obj.insert("tag".into(), json!(tag.clone()));
        }
        member_outbounds.push(ob);
        // Warm prior: known-good (or known-bad rtt=0) results seed the core.
        if member.latency_ms != 0 && validity_mins > 0 {
            warm.push(json!({
                "tag": tag,
                "rtt": if member.latency_ms > 0 { member.latency_ms } else { 0 },
                "age": 0
            }));
        }
        if auto.config.pinned_id >= 0 && member.id == auto.config.pinned_id {
            pinned_tag = Some(tag.clone());
        }
        tags.push(json!(tag));
        domains.extend(collect_outbound_server_domains(member));
    }

    let test_url = if auto.config.test_url.trim().is_empty() {
        settings.test_latency_url.clone()
    } else {
        auto.config.test_url.clone()
    };
    let connectivity_url = if auto.config.connectivity_url.trim().is_empty() {
        test_url.clone()
    } else {
        auto.config.connectivity_url.clone()
    };

    let mut group = json!({
        "type": "auto-selector",
        "tag": "proxy",
        "outbounds": tags,
        "url": test_url,
        "interval": format!("{}s", auto.config.interval_sec.max(10)),
        "bench_interval": format!("{}s", auto.config.bench_interval_sec.max(auto.config.interval_sec)),
        "watch_interval": format!("{}s", auto.config.watch_interval_sec.max(5)),
        "active_size": auto.config.active_size.max(1),
        "sampling": auto.config.sampling.clamp(2, 60),
        "tolerance": auto.config.tolerance_ms.max(0),
        "expected": auto.config.expected.max(1),
        "dial_retries": auto.config.dial_retries.clamp(0, 5),
        "interrupt_exist_connections": auto.config.interrupt_on_switch,
        "connectivity_url": connectivity_url,
    });
    if let Some(obj) = group.as_object_mut() {
        if !warm.is_empty() {
            obj.insert("warm".into(), json!(warm));
        }
        if let Some(pin) = pinned_tag {
            obj.insert("pinned".into(), json!(pin));
        }
        if auto.config.max_rtt_ms > 0 {
            obj.insert(
                "max_rtt".into(),
                json!(format!("{}ms", auto.config.max_rtt_ms)),
            );
        }
        if auto.config.balance {
            obj.insert("balance".into(), json!(true));
            obj.insert("balance_mode".into(), json!(auto.config.balance_mode));
            obj.insert(
                "balance_interval".into(),
                json!(format!("{}s", auto.config.balance_interval_sec.max(5))),
            );
        }
    }

    Ok(AutoSelectorOutbounds {
        group,
        member_outbounds,
        domains,
        xray_full_configs,
        any_xray_member,
    })
}

/// One Xray full-config member: socks outbound in sing-box + opaque Xray JSON.
struct XrayFullBridge {
    socks_outbound: Value,
    xray_config: String,
}

/// Build a socks bridge outbound + Xray full config with a matching socks inbound.
///
/// Upstream Custom::Build for `xrayfullconfig` + generate.cpp soleXrayInbound path.
fn build_xray_full_member(profile: &Profile) -> Result<XrayFullBridge, CoreError> {
    let raw = xray_full_config_raw(profile).ok_or_else(|| {
        CoreError::Config(format!(
            "Xray full config profile '{}' has no config body",
            profile.name
        ))
    })?;
    let mut user_cfg: Value = serde_json::from_str(&raw)
        .map_err(|e| CoreError::Config(format!("invalid Xray full config JSON: {e}")))?;

    let port = reserve_loopback_port().map_err(|e| {
        CoreError::Config(format!(
            "Could not reserve a local port for the custom Xray full config bridge: {e}"
        ))
    })?;
    let auth = random_bridge_auth();

    // Sole inbound: only Throne's bridge (sibling subscription configs repeat ports).
    let bridge_inbound = json!({
        "tag": "throne-bridge",
        "port": port,
        "listen": "127.0.0.1",
        "protocol": "socks",
        "settings": {
            "auth": "password",
            "accounts": [{ "user": auth, "pass": auth }],
            "udp": true
        },
        "sniffing": {
            "enabled": true,
            "destOverride": ["http", "tls", "quic"]
        }
    });
    if let Some(obj) = user_cfg.as_object_mut() {
        obj.insert("inbounds".into(), json!([bridge_inbound]));
    } else {
        return Err(CoreError::Config(
            "Xray full config root must be a JSON object".into(),
        ));
    }

    let xray_config =
        serde_json::to_string(&user_cfg).map_err(|e| CoreError::Config(e.to_string()))?;
    let socks_outbound = json!({
        "type": "socks",
        "server": "127.0.0.1",
        "server_port": port,
        "username": auth,
        "password": auth,
    });
    Ok(XrayFullBridge {
        socks_outbound,
        xray_config,
    })
}

fn xray_full_config_raw(profile: &Profile) -> Option<String> {
    if let Ok(v) = serde_json::from_str::<Value>(&profile.outbound_json) {
        if let Some(cfg) = v.get("config").and_then(|c| c.as_str()) {
            if !cfg.trim().is_empty() {
                return Some(cfg.to_string());
            }
        }
        // Entire outbound_json may itself be the Xray document.
        if v.get("outbounds").is_some() && v.get("protocol").is_none() {
            if let Ok(s) = serde_json::to_string(&v) {
                return Some(s);
            }
        }
    }
    profile.outbound.raw_json.clone()
}

fn member_uses_xray_sidecar(member: &Profile) -> bool {
    matches!(
        member.profile_type,
        ProfileType::XrayVless | ProfileType::Vmess
    ) || member
        .outbound_json
        .contains("\"subtype\":\"xrayoutbound\"")
        || member
            .outbound
            .security
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("xray"))
}

fn reserve_loopback_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    // Drop listener so the core can bind the same port shortly after.
    drop(listener);
    Ok(port)
}

fn random_bridge_auth() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // 32 hex chars ≈ upstream GetRandomString(32) entropy for the bridge.
    format!("thr{:028x}", nanos)
}

fn build_proxy_outbound(profile: &Profile) -> Result<Value, CoreError> {
    if profile.profile_type == ProfileType::AutoSelector {
        return Err(CoreError::Config(
            "cannot build a leaf outbound from an Auto Selector".into(),
        ));
    }
    // Prefer a ready-made sing-box outbound object when outbound_json already has type.
    if let Ok(v) = serde_json::from_str::<Value>(&profile.outbound_json) {
        if let Some(out) = take_singbox_outbound(v) {
            return Ok(out);
        }
    }
    if let Some(raw) = &profile.outbound.raw_json {
        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            if let Some(out) = take_singbox_outbound(v) {
                return Ok(out);
            }
        }
    }

    from_parsed(profile.profile_type, &profile.outbound)
}

/// Accept a JSON object as a sing-box outbound if it has a known `type`, force
/// `tag=proxy`, and drop Clash/UI-only keys that would fail core decode.
fn take_singbox_outbound(mut v: Value) -> Option<Value> {
    let obj = v.as_object_mut()?;
    let ty = obj.get("type")?.as_str()?;
    if !is_singbox_outbound_type(ty) {
        return None;
    }
    // Keys that appear in Clash/export blobs but are not sing-box outbound fields.
    const STRIP: &[&str] = &[
        "name",
        "skip-cert-verify",
        "skip_cert_verify",
        "udp",
        "tfo",
        "mptcp",
        "smux",
        "client-fingerprint",
        "client_fingerprint",
        "ip-version",
        "ip_version",
        "dialer-proxy",
        "dialer_proxy",
    ];
    for k in STRIP {
        obj.remove(*k);
    }
    // Repair legacy / share-link exports that wrote `transport: { type: "tcp" }`.
    // sing-box rejects that; plain TCP is the default when transport is omitted.
    sanitize_outbound_transport(obj);
    obj.insert("tag".into(), json!("proxy"));
    Some(v)
}

/// Drop or normalize `transport` so the core never sees invalid types like `tcp`.
fn sanitize_outbound_transport(obj: &mut serde_json::Map<String, Value>) {
    let Some(tr) = obj.get("transport").and_then(|t| t.as_object()) else {
        return;
    };
    let Some(raw) = tr.get("type").and_then(|t| t.as_str()) else {
        obj.remove("transport");
        return;
    };
    match throne_domain::normalize_singbox_transport_type(raw) {
        None => {
            obj.remove("transport");
        }
        Some(canonical) if canonical != raw => {
            if let Some(tr) = obj.get_mut("transport").and_then(|t| t.as_object_mut()) {
                tr.insert("type".into(), json!(canonical));
            }
        }
        Some(_) => {}
    }
}

fn is_singbox_outbound_type(ty: &str) -> bool {
    matches!(
        ty,
        "shadowsocks"
            | "vmess"
            | "vless"
            | "trojan"
            | "hysteria"
            | "hysteria2"
            | "tuic"
            | "socks"
            | "http"
            | "wireguard"
            | "ssh"
            | "anytls"
            | "mieru"
            | "shadowtls"
            | "tor"
            | "direct"
            | "block"
            | "selector"
            | "urltest"
            | "auto-selector"
    )
}

fn from_parsed(ty: ProfileType, o: &ParsedOutbound) -> Result<Value, CoreError> {
    let server = o
        .server
        .clone()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CoreError::Config("profile has no server address".into()))?;
    let port = o.server_port.unwrap_or(443);

    let mut out = match ty {
        ProfileType::Shadowsocks => {
            let method = o.method.clone().unwrap_or_else(|| "aes-128-gcm".into());
            let password = o.password.clone().unwrap_or_default();
            json!({
                "type": "shadowsocks",
                "server": server,
                "server_port": port,
                "method": method,
                "password": password
            })
        }
        ProfileType::Vmess => {
            let uuid = o.uuid.clone().unwrap_or_default();
            let mut v = json!({
                "type": "vmess",
                "server": server,
                "server_port": port,
                "uuid": uuid,
                "security": o.security.clone().unwrap_or_else(|| "auto".into()),
                "alter_id": o.alter_id.unwrap_or(0)
            });
            apply_transport(&mut v, o);
            apply_tls(&mut v, o, true);
            v
        }
        ProfileType::Vless => {
            let uuid = o.uuid.clone().unwrap_or_default();
            let mut v = json!({
                "type": "vless",
                "server": server,
                "server_port": port,
                "uuid": uuid
            });
            if let Some(flow) = &o.flow {
                if !flow.is_empty() {
                    v.as_object_mut()
                        .unwrap()
                        .insert("flow".into(), json!(flow));
                }
            }
            if let Some(pe) = &o.packet_encoding {
                if !pe.is_empty() {
                    v.as_object_mut()
                        .unwrap()
                        .insert("packet_encoding".into(), json!(pe));
                }
            }
            apply_transport(&mut v, o);
            apply_tls(&mut v, o, true);
            v
        }
        ProfileType::Trojan => {
            let password = o.password.clone().unwrap_or_default();
            let mut v = json!({
                "type": "trojan",
                "server": server,
                "server_port": port,
                "password": password
            });
            apply_transport(&mut v, o);
            apply_tls(&mut v, o, true);
            v
        }
        ProfileType::Hysteria2 => {
            let password = o
                .password
                .clone()
                .or_else(|| o.uuid.clone())
                .unwrap_or_default();
            let mut v = json!({
                "type": "hysteria2",
                "server": server,
                "server_port": port,
                "password": password
            });
            if let Some(up) = o.up_mbps {
                v.as_object_mut()
                    .unwrap()
                    .insert("up_mbps".into(), json!(up));
            }
            if let Some(down) = o.down_mbps {
                v.as_object_mut()
                    .unwrap()
                    .insert("down_mbps".into(), json!(down));
            }
            if let Some(obfs) = &o.obfs {
                if !obfs.is_empty() {
                    v.as_object_mut().unwrap().insert(
                        "obfs".into(),
                        json!({ "type": "salamander", "password": obfs }),
                    );
                }
            }
            apply_tls(&mut v, o, true);
            v
        }
        ProfileType::Tuic => {
            let uuid = o.uuid.clone().unwrap_or_default();
            let password = o.password.clone().unwrap_or_default();
            let mut v = json!({
                "type": "tuic",
                "server": server,
                "server_port": port,
                "uuid": uuid,
                "password": password,
                "congestion_control": o.congestion_control.clone().unwrap_or_else(|| "bbr".into()),
                "udp_relay_mode": o.udp_relay_mode.clone().unwrap_or_else(|| "native".into())
            });
            apply_tls(&mut v, o, true);
            v
        }
        ProfileType::Socks => {
            let mut v = json!({
                "type": "socks",
                "server": server,
                "server_port": port,
                "version": "5"
            });
            if let Some(u) = &o.username {
                if !u.is_empty() {
                    v.as_object_mut()
                        .unwrap()
                        .insert("username".into(), json!(u));
                }
            }
            if let Some(p) = &o.password {
                if !p.is_empty() {
                    v.as_object_mut()
                        .unwrap()
                        .insert("password".into(), json!(p));
                }
            }
            v
        }
        ProfileType::Http => {
            let mut v = json!({
                "type": "http",
                "server": server,
                "server_port": port
            });
            if let Some(u) = &o.username {
                if !u.is_empty() {
                    v.as_object_mut()
                        .unwrap()
                        .insert("username".into(), json!(u));
                }
            }
            if let Some(p) = &o.password {
                if !p.is_empty() {
                    v.as_object_mut()
                        .unwrap()
                        .insert("password".into(), json!(p));
                }
            }
            if o.tls == Some(true) {
                v.as_object_mut()
                    .unwrap()
                    .insert("tls".into(), json!({ "enabled": true }));
            }
            v
        }
        other => {
            return Err(CoreError::Config(format!(
                "cannot auto-build outbound for type {}",
                other.display_name()
            )));
        }
    };

    if let Some(obj) = out.as_object_mut() {
        obj.insert("tag".into(), json!("proxy"));
    }
    Ok(out)
}

fn apply_transport(v: &mut Value, o: &ParsedOutbound) {
    let Some(obj) = v.as_object_mut() else {
        return;
    };
    let Some(transport) =
        throne_domain::normalize_singbox_transport_type(o.transport.as_deref().unwrap_or("tcp"))
    else {
        // Plain TCP / unknown → omit transport (sing-box default).
        return;
    };
    match transport {
        "ws" => {
            let mut t = json!({ "type": "ws" });
            if let Some(path) = &o.path {
                t.as_object_mut()
                    .unwrap()
                    .insert("path".into(), json!(path));
            }
            if let Some(host) = &o.host {
                t.as_object_mut()
                    .unwrap()
                    .insert("headers".into(), json!({ "Host": host }));
            }
            obj.insert("transport".into(), t);
        }
        "grpc" => {
            let mut t = json!({ "type": "grpc" });
            if let Some(sn) = o.service_name.as_ref().or(o.path.as_ref()) {
                t.as_object_mut()
                    .unwrap()
                    .insert("service_name".into(), json!(sn));
            }
            obj.insert("transport".into(), t);
        }
        "http" => {
            let mut t = json!({ "type": "http" });
            if let Some(path) = &o.path {
                t.as_object_mut()
                    .unwrap()
                    .insert("path".into(), json!(path));
            }
            if let Some(host) = &o.host {
                t.as_object_mut()
                    .unwrap()
                    .insert("host".into(), json!([host]));
            }
            obj.insert("transport".into(), t);
        }
        "quic" => {
            obj.insert("transport".into(), json!({ "type": "quic" }));
        }
        "httpupgrade" => {
            let mut t = json!({ "type": "httpupgrade" });
            if let Some(path) = &o.path {
                t.as_object_mut()
                    .unwrap()
                    .insert("path".into(), json!(path));
            }
            if let Some(host) = &o.host {
                t.as_object_mut()
                    .unwrap()
                    .insert("host".into(), json!(host));
            }
            obj.insert("transport".into(), t);
        }
        _ => {}
    }
}

fn apply_tls(v: &mut Value, o: &ParsedOutbound, default_on: bool) {
    let Some(obj) = v.as_object_mut() else {
        return;
    };
    let enabled = o.tls.unwrap_or(default_on)
        || o.security
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("tls") || s.eq_ignore_ascii_case("reality"));
    if !enabled {
        return;
    }
    let mut tls = json!({ "enabled": true });
    let t = tls.as_object_mut().unwrap();
    if let Some(sni) = o.sni.as_ref().or(o.host.as_ref()) {
        if !sni.is_empty() {
            t.insert("server_name".into(), json!(sni));
        }
    }
    if o.insecure == Some(true) {
        t.insert("insecure".into(), json!(true));
    }
    if let Some(alpn) = &o.alpn {
        if !alpn.is_empty() {
            let list: Vec<&str> = alpn.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            if !list.is_empty() {
                t.insert("alpn".into(), json!(list));
            }
        }
    }
    if let Some(fp) = &o.fp {
        if !fp.is_empty() {
            t.insert("utls".into(), json!({ "enabled": true, "fingerprint": fp }));
        }
    }
    // Reality
    if let Some(pbk) = &o.pbk {
        if !pbk.is_empty() {
            let mut reality = json!({ "enabled": true, "public_key": pbk });
            if let Some(sid) = &o.sid {
                reality
                    .as_object_mut()
                    .unwrap()
                    .insert("short_id".into(), json!(sid));
            }
            t.insert("reality".into(), reality);
        }
    }
    obj.insert("tls".into(), tls);
}

/// Upstream `buildDNSSection` (generate.cpp) + tag for `route.default_domain_resolver`.
///
/// Tun path mirrors Qt Throne:
/// - `dns-remote` detour=proxy, domain_resolver=dns-local
/// - `dns-direct` from `direct_dns`, domain_resolver=dns-local (no detour)
/// - `dns-local` from `core_box_underlying_dns` (required on Darwin Tun)
/// - proxy server hostnames → route to dns-direct
/// - final DNS rule from `dns_final_out`
/// - no top-level `dns.final` (rules carry the final server)
fn build_dns_section(
    settings: &AppSettings,
    tun_enabled: bool,
    proxy_domains: &[String],
) -> Result<(Value, &'static str), CoreError> {
    if !tun_enabled {
        // Mixed/system-proxy path: OS resolver is proven-stable here.
        return Ok((
            json!({
                "servers": [ { "type": "local", "tag": "local" } ],
                "final": "local",
                "strategy": "prefer_ipv4"
            }),
            "local",
        ));
    }

    let underlying = underlying_dns_address(settings)?;

    // remote — upstream: only dns-remote gets detour=proxy
    let mut remote = build_dns_obj(&settings.remote_dns, tun_enabled, &underlying);
    if let Some(obj) = remote.as_object_mut() {
        obj.insert("tag".into(), json!("dns-remote"));
        obj.insert("detour".into(), json!("proxy"));
        obj.insert("domain_resolver".into(), json!("dns-local"));
    }

    // direct — upstream: buildDnsObj(direct_dns); no detour
    let mut direct = build_dns_obj(&settings.direct_dns, tun_enabled, &underlying);
    if let Some(obj) = direct.as_object_mut() {
        obj.insert("tag".into(), json!("dns-direct"));
        obj.insert("domain_resolver".into(), json!("dns-local"));
    }

    // local — upstream: empty underlying → "local"; Darwin Tun rewrites to UDP underlying
    let dns_local_address = if settings.core_box_underlying_dns.trim().is_empty() {
        "local".to_string()
    } else {
        settings.core_box_underlying_dns.trim().to_string()
    };
    let mut local = build_dns_obj(&dns_local_address, tun_enabled, &underlying);
    if let Some(obj) = local.as_object_mut() {
        obj.insert("tag".into(), json!("dns-local"));
    }

    let use_direct_final = settings.dns_final_out.trim().eq_ignore_ascii_case("direct");
    let final_tag = if use_direct_final {
        "dns-direct"
    } else {
        "dns-remote"
    };
    let final_strategy = if use_direct_final {
        settings.direct_dns_strategy.trim()
    } else {
        settings.remote_dns_strategy.trim()
    };
    let direct_strategy = settings.direct_dns_strategy.trim();

    // Upstream rule order (subset we support): localhost → proxy domains → final
    let mut rules = vec![
        json!({
            "domain": "localhost",
            "action": "predefined",
            "query_type": "A",
            "rcode": "NOERROR",
            "answer": "localhost. IN A 127.0.0.1"
        }),
        json!({
            "domain": "localhost",
            "action": "predefined",
            "query_type": "AAAA",
            "rcode": "NXDOMAIN"
        }),
    ];

    // Upstream needDirectDnsRules: proxy server domains must not go via dns-remote
    // (chicken-egg before the outbound is up).
    if !proxy_domains.is_empty() {
        let mut rule = json!({
            "domain": proxy_domains,
            "action": "route",
            "server": "dns-direct"
        });
        if !direct_strategy.is_empty() {
            rule.as_object_mut()
                .unwrap()
                .insert("strategy".into(), json!(direct_strategy));
        }
        rules.push(rule);
    }

    let mut final_rule = json!({
        "action": "route",
        "server": final_tag
    });
    if !final_strategy.is_empty() {
        final_rule
            .as_object_mut()
            .unwrap()
            .insert("strategy".into(), json!(final_strategy));
    }
    rules.push(final_rule);

    // Upstream dns object: servers + rules + cache_* — no top-level final.
    let mut dns = json!({
        "servers": [remote, direct, local],
        "rules": rules,
        "cache_capacity": settings.dns_cache_capacity.max(0)
    });
    if let Some(obj) = dns.as_object_mut() {
        if settings.dns_disable_cache {
            obj.insert("disable_cache".into(), json!(true));
        }
        if settings.dns_disable_expire {
            obj.insert("disable_expire".into(), json!(true));
        }
        if settings.dns_reverse_mapping {
            obj.insert("reverse_mapping".into(), json!(true));
        }
    }

    // Upstream buildRouteSection always uses dns-direct as default_domain_resolver.
    Ok((dns, "dns-direct"))
}

/// Upstream `outboundServerDomains` / `getEntDomains` for a single profile.
/// Only non-IP server addresses — IPs need no DNS direct rule.
fn collect_outbound_server_domains(profile: &Profile) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |s: &str| {
        let t = s.trim().trim_matches(|c| c == '[' || c == ']');
        if t.is_empty() || t.parse::<std::net::IpAddr>().is_ok() {
            return;
        }
        if !out.iter().any(|x: &String| x.eq_ignore_ascii_case(t)) {
            out.push(t.to_string());
        }
    };
    if let Some(s) = &profile.outbound.server {
        push(s);
    }
    if let Ok(v) = serde_json::from_str::<Value>(&profile.outbound_json) {
        if let Some(s) = v.get("server").and_then(|x| x.as_str()) {
            push(s);
        }
        // Chain-like hop lists are rare in outbound_json; collect nested servers
        // when present (urltest/selector export blobs).
        if let Some(arr) = v.get("outbounds").and_then(|x| x.as_array()) {
            for item in arr {
                if let Some(s) = item.get("server").and_then(|x| x.as_str()) {
                    push(s);
                }
            }
        }
    }
    out
}

/// Upstream generate.cpp Darwin Tun guard:
/// "Local DNS and Tun mode do not work together, please set an IP…"
fn validate_darwin_tun_underlying_dns(settings: &AppSettings) -> Result<(), CoreError> {
    #[cfg(target_os = "macos")]
    {
        if !settings.tun_mode_enabled {
            return Ok(());
        }
        let t = settings.core_box_underlying_dns.trim();
        if t.is_empty() || t.eq_ignore_ascii_case("local") || t.eq_ignore_ascii_case("localhost") {
            return Err(CoreError::Config(
                "Local DNS and Tun mode do not work together, please set an IP to be used as the Local DNS server in the Routing Settings -> Local override".into(),
            ));
        }
    }
    let _ = settings;
    Ok(())
}

/// Concrete IP/host for dns-local under Tun (after Darwin validation).
fn underlying_dns_address(settings: &AppSettings) -> Result<String, CoreError> {
    validate_darwin_tun_underlying_dns(settings)?;
    let t = settings.core_box_underlying_dns.trim();
    if !t.is_empty()
        && !t.eq_ignore_ascii_case("local")
        && !t.eq_ignore_ascii_case("localhost")
    {
        return Ok(t.to_string());
    }
    // Non-Darwin: allow empty → "local" type; callers pass through build_dns_obj.
    Ok(String::new())
}

/// Upstream `buildDnsObj(address, ctx)`.
///
/// On Darwin + Tun, `local`/`localhost` is rewritten to UDP against
/// `core_box_underlying_dns` (must already be validated non-empty).
fn build_dns_obj(address: &str, tun_enabled: bool, underlying: &str) -> Value {
    let address = address.trim();
    if address.is_empty()
        || address.eq_ignore_ascii_case("local")
        || address.eq_ignore_ascii_case("localhost")
    {
        if tun_enabled {
            #[cfg(target_os = "macos")]
            {
                // Upstream: type udp + server = core_box_underlying_dns
                if !underlying.is_empty() {
                    return json!({ "type": "udp", "server": underlying });
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = underlying;
            }
        }
        return json!({ "type": "local" });
    }

    if let Some(ifc) = address.strip_prefix("dhcp://") {
        let ifc = if ifc == "auto" { "" } else { ifc };
        return json!({ "type": "dhcp", "interface": ifc });
    }

    let (ty, rest) = if let Some(rest) = address.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = address.strip_prefix("h3://") {
        ("h3", rest)
    } else if let Some(rest) = address.strip_prefix("http://") {
        ("http", rest)
    } else if let Some(rest) = address.strip_prefix("udp://") {
        ("udp", rest)
    } else if let Some(rest) = address.strip_prefix("tcp://") {
        ("tcp", rest)
    } else if let Some(rest) = address.strip_prefix("tls://") {
        ("tls", rest)
    } else if let Some(rest) = address.strip_prefix("quic://") {
        ("quic", rest)
    } else {
        ("udp", address)
    };

    let (host_port, path) = if ty == "https" || ty == "http" || ty == "h3" {
        match rest.split_once('/') {
            Some((host, path)) => (host, format!("/{path}")),
            None => (rest, "/dns-query".into()),
        }
    } else {
        (rest, String::new())
    };
    let (server, server_port) = host_port
        .rsplit_once(':')
        .filter(|(_, port)| port.chars().all(|ch| ch.is_ascii_digit()))
        .map(|(host, port)| (host.to_string(), port.parse::<u16>().ok()))
        .unwrap_or_else(|| (host_port.to_string(), None));

    let mut obj = json!({ "type": ty, "server": server });
    if let Some(port) = server_port {
        obj.as_object_mut()
            .unwrap()
            .insert("server_port".into(), json!(port));
    }
    if !path.is_empty() {
        obj.as_object_mut()
            .unwrap()
            .insert("path".into(), json!(path));
    }
    obj
}

/// Back-compat alias used by unit tests that still call the old name.
#[cfg(test)]
fn build_dns_server_obj(address: &str) -> Value {
    build_dns_obj(address, false, "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use throne_domain::{ParsedOutbound, Profile, ProfileType, RulesetMirror};

    fn tun_settings() -> AppSettings {
        let mut settings = AppSettings::default();
        settings.tun_mode_enabled = true;
        // Upstream Darwin Tun hard-requires Local override (core_box_underlying_dns).
        settings.core_box_underlying_dns = "223.5.5.5".into();
        settings.direct_dns = "223.5.5.5".into();
        settings.remote_dns = "8.8.8.8".into();
        settings
    }

    #[test]
    fn auto_selector_emits_native_group_with_members() {
        use throne_domain::AutoSelectorConfig;
        let selector = Profile::new(99, 1, "auto", ProfileType::AutoSelector);
        let mut a = Profile::new(1, 1, "a", ProfileType::Vless);
        a.outbound = ParsedOutbound {
            server: Some("a.example.com".into()),
            server_port: Some(443),
            uuid: Some("11111111-1111-1111-1111-111111111111".into()),
            ..Default::default()
        };
        a.latency_ms = 40;
        let mut b = Profile::new(2, 1, "b", ProfileType::Vless);
        b.outbound = ParsedOutbound {
            server: Some("b.example.com".into()),
            server_port: Some(443),
            uuid: Some("22222222-2222-2222-2222-222222222222".into()),
            ..Default::default()
        };
        b.latency_ms = 90;
        let mut cfg = AutoSelectorConfig::new_for_group(1, "auto");
        cfg.interval_sec = 60;
        let auto = AutoSelectorBuild {
            config: cfg,
            members: vec![a, b],
        };
        let built =
            build_load_config_ex(&selector, &AppSettings::default(), None, Some(&auto)).unwrap();
        let v: Value = serde_json::from_str(&built.core_config_json).unwrap();
        let outs = v["outbounds"].as_array().unwrap();
        assert!(outs.iter().any(|o| o["tag"] == "p1"));
        assert!(outs.iter().any(|o| o["tag"] == "p2"));
        let proxy = outs.iter().find(|o| o["tag"] == "proxy").expect("proxy");
        assert_eq!(proxy["type"], "auto-selector");
        assert_eq!(proxy["outbounds"].as_array().unwrap().len(), 2);
        assert!(proxy.get("url").is_some());
        assert!(proxy.get("connectivity_url").is_some());
        assert!(proxy.get("warm").is_some()); // latency_ms set on members
        assert_eq!(proxy["active_size"], 8);
    }

    #[test]
    fn subtract_ipv4_punches_tun_subnet_out_of_private_range() {
        let ranges = vec!["172.16.0.0/12".into()];
        let out = subtract_ipv4_prefix(&ranges, "172.19.0.1/24");
        // Hole 172.19.0.0/24 must not be covered as a single excluded /12.
        assert!(!out.iter().any(|s| s == "172.16.0.0/12"));
        // Siblings should still bypass the rest of 172.16/12.
        assert!(
            out.iter().any(|s| s.starts_with("172.")),
            "expected sibling prefixes, got {out:?}"
        );
        // The hole itself is never listed as an exclude.
        assert!(!out.iter().any(|s| s == "172.19.0.0/24" || s == "172.19.0.1/24"));
    }

    #[test]
    fn tun_inbound_matches_upstream_generate() {
        let mut p = Profile::new(1, 1, "n1", ProfileType::Vless);
        p.outbound = ParsedOutbound {
            server: Some("1.2.3.4".into()),
            server_port: Some(443),
            uuid: Some("11111111-1111-1111-1111-111111111111".into()),
            ..Default::default()
        };
        let settings = tun_settings();
        let built = build_load_config(&p, &settings, None).unwrap();
        let v: Value = serde_json::from_str(&built.core_config_json).unwrap();
        let tun = v["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|ib| ib["type"] == "tun")
            .expect("tun inbound");
        assert_eq!(tun["tag"], "tun-in");
        assert_eq!(tun["auto_route"], true);
        assert_eq!(tun["strict_route"], false);
        assert_eq!(tun["address"][0], "172.19.0.1/24");
        assert_eq!(built.tun_ipv4_cidr, "172.19.0.1/24");
        // Upstream: private ranges only (not bare proxy IPs).
        let excludes = tun["route_exclude_address"].as_array().unwrap();
        assert!(excludes.iter().any(|e| e.as_str() == Some("192.168.0.0/16")));
        assert!(!excludes.iter().any(|e| e.as_str() == Some("1.2.3.4/32")));
        #[cfg(target_os = "macos")]
        {
            // #1738: Tun subnet must not be fully excluded (macOS system DNS = tunIP+1).
            assert!(
                !excludes
                    .iter()
                    .any(|e| e.as_str() == Some("172.16.0.0/12")),
                "172.16.0.0/12 must be split around Tun CIDR on Darwin, raw exclude was {excludes:?}"
            );
            // Hole is 172.19.0.0/24; siblings like 172.19.1.0/24 are expected.
            assert!(
                !excludes.iter().any(|e| {
                    e.as_str()
                        .is_some_and(|s| s == "172.19.0.0/24" || s == "172.19.0.1/24")
                }),
                "Tun /24 hole must not appear in route_exclude_address: {excludes:?}"
            );
        }
        // Upstream buildRouteSection
        assert_eq!(v["route"]["default_domain_resolver"]["server"], "dns-direct");
        assert_eq!(v["route"]["auto_detect_interface"], true);
        // Upstream dns object has no top-level final
        assert!(v["dns"].get("final").is_none());
        let servers = v["dns"]["servers"].as_array().unwrap();
        assert!(servers.iter().any(|s| s["tag"] == "dns-remote" && s["detour"] == "proxy"));
        assert!(servers.iter().any(|s| {
            s["tag"] == "dns-local" && s["type"] == "udp" && s["server"] == "223.5.5.5"
        }));
        for s in servers {
            if s["tag"] == "dns-direct" || s["tag"] == "dns-local" {
                assert!(s.get("detour").is_none(), "no detour on {s}");
            }
        }
        #[cfg(target_os = "macos")]
        {
            assert!(
                tun.get("interface_name").is_none()
                    || tun["interface_name"]
                        .as_str()
                        .is_some_and(|n| n.is_empty() || n.starts_with("utun")),
                "macOS tun name must be utun* or omitted, got {:?}",
                tun.get("interface_name")
            );
            assert_eq!(tun["stack"], "gvisor");
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(tun["interface_name"], "throne-tun");
        }
    }

    #[test]
    fn tun_dns_routes_proxy_hostname_via_direct() {
        // Upstream CalculatePrerequisities → directDomains for getEntDomains.
        let mut p = Profile::new(1, 1, "hy2", ProfileType::Hysteria2);
        p.outbound = ParsedOutbound {
            server: Some("node.example.com".into()),
            server_port: Some(443),
            password: Some("x".into()),
            ..Default::default()
        };
        p.outbound_json = r#"{"type":"hysteria2","server":"node.example.com","server_port":443,"password":"x","tag":"proxy"}"#.into();
        let settings = tun_settings();
        let built = build_load_config(&p, &settings, None).unwrap();
        let v: Value = serde_json::from_str(&built.core_config_json).unwrap();
        let rules = v["dns"]["rules"].as_array().expect("dns.rules");
        let has_proxy_direct = rules.iter().any(|r| {
            r.get("server").and_then(|s| s.as_str()) == Some("dns-direct")
                && r.get("domain")
                    .and_then(|d| d.as_array())
                    .is_some_and(|a| a.iter().any(|x| x.as_str() == Some("node.example.com")))
        });
        assert!(
            has_proxy_direct,
            "proxy hostname must resolve via dns-direct: {rules:?}"
        );
        assert_eq!(v["route"]["default_domain_resolver"]["server"], "dns-direct");
        let servers = v["dns"]["servers"].as_array().unwrap();
        let direct = servers.iter().find(|s| s["tag"] == "dns-direct").unwrap();
        assert_eq!(direct["server"], "223.5.5.5");
        let remote = servers.iter().find(|s| s["tag"] == "dns-remote").unwrap();
        assert_eq!(remote["detour"], "proxy");
        assert_eq!(remote["domain_resolver"], "dns-local");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn tun_requires_underlying_dns_on_darwin() {
        // Upstream generate.cpp hard error when core_box_underlying_dns empty.
        let mut p = Profile::new(1, 1, "n1", ProfileType::Vless);
        p.outbound = ParsedOutbound {
            server: Some("1.2.3.4".into()),
            server_port: Some(443),
            uuid: Some("u".into()),
            ..Default::default()
        };
        let mut settings = AppSettings::default();
        settings.tun_mode_enabled = true;
        settings.core_box_underlying_dns.clear();
        let err = build_load_config(&p, &settings, None).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Local DNS and Tun mode do not work together"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn tun_collects_nested_outbound_hostnames_for_dns_direct() {
        // Nested hop hostnames (urltest/selector export) → dns-direct, like chain hops.
        let mut p = Profile::new(1, 1, "grp", ProfileType::Vless);
        p.outbound_json = r#"{
            "type": "urltest",
            "tag": "proxy",
            "outbounds": [
                {"type":"vless","server":"10.1.2.3","server_port":443,"uuid":"u","tag":"a"},
                {"type":"trojan","server":"node.group.example","server_port":443,"password":"x","tag":"b"}
            ]
        }"#
        .into();
        let settings = tun_settings();
        let built = build_load_config(&p, &settings, None).unwrap();
        let v: Value = serde_json::from_str(&built.core_config_json).unwrap();
        let rules = v["dns"]["rules"].as_array().unwrap();
        let has_member_host = rules.iter().any(|r| {
            r.get("server").and_then(|s| s.as_str()) == Some("dns-direct")
                && r.get("domain")
                    .and_then(|d| d.as_array())
                    .is_some_and(|a| a.iter().any(|x| x.as_str() == Some("node.group.example")))
        });
        assert!(
            has_member_host,
            "group member hostname must use dns-direct: {rules:?}"
        );
        // Bare IPs are not DNS domains (upstream outboundServerDomains skips IsIpAddress).
        let has_ip_domain = rules.iter().any(|r| {
            r.get("domain")
                .and_then(|d| d.as_array())
                .is_some_and(|a| a.iter().any(|x| x.as_str() == Some("10.1.2.3")))
        });
        assert!(!has_ip_domain);
    }

    #[test]
    fn builds_vless_config() {
        let mut p = Profile::new(1, 1, "n1", ProfileType::Vless);
        p.outbound = ParsedOutbound {
            server: Some("1.2.3.4".into()),
            server_port: Some(443),
            uuid: Some("11111111-1111-1111-1111-111111111111".into()),
            tls: Some(true),
            sni: Some("example.com".into()),
            ..Default::default()
        };
        let settings = AppSettings::default(); // inbound 127.0.0.1
        let built = build_load_config(&p, &settings, None).unwrap();
        let v: Value = serde_json::from_str(&built.core_config_json).unwrap();
        assert_eq!(v["inbounds"][0]["listen_port"], 2080);
        assert_eq!(v["inbounds"][0]["listen"], "127.0.0.1");
        // Legacy inbound sniff fields must not appear (sing-box ≥1.11 decode fails).
        assert!(v["inbounds"][0].get("sniff").is_none());
        assert!(v["inbounds"][0].get("sniff_override_destination").is_none());
        assert_eq!(v["outbounds"][0]["type"], "vless");
        assert_eq!(v["outbounds"][0]["tag"], "proxy");
        // No legacy block outbound.
        assert!(
            v["outbounds"]
                .as_array()
                .unwrap()
                .iter()
                .all(|o| o["type"] != "block")
        );
        assert_eq!(v["route"]["final"], "proxy");
        assert_eq!(v["route"]["rules"][0]["action"], "sniff");
        // OS local DNS is the only reliable default dial resolver here.
        assert_eq!(v["dns"]["final"], "local");
        assert_eq!(v["route"]["default_domain_resolver"]["server"], "local");
        assert_eq!(v["route"]["rules"][0]["action"], "sniff");
        // Default route: private → direct
        let rules = v["route"]["rules"].as_array().unwrap();
        assert!(
            rules.iter().any(|r| {
                r.get("ip_is_private") == Some(&json!(true))
                    && r.get("outbound") == Some(&json!("direct"))
            }),
            "expected private→direct rule: {rules:?}"
        );
    }

    #[test]
    fn binds_to_all_interfaces_when_lan_access_is_requested() {
        let mut profile = Profile::new(1, 1, "node", ProfileType::Vless);
        profile.outbound = ParsedOutbound {
            server: Some("1.2.3.4".into()),
            server_port: Some(443),
            uuid: Some("11111111-1111-1111-1111-111111111111".into()),
            ..Default::default()
        };

        // Tray "Allow other devices to connect" → `::` (current upstream).
        let mut settings = AppSettings::default();
        settings.inbound_address = "::".into();
        let config = build_load_config(&profile, &settings, None).unwrap();
        let value: Value = serde_json::from_str(&config.core_config_json).unwrap();
        assert_eq!(value["inbounds"][0]["listen"], "::");

        // Manual Basic Settings / older Allow LAN → `0.0.0.0`.
        settings.inbound_address = "0.0.0.0".into();
        let config = build_load_config(&profile, &settings, None).unwrap();
        let value: Value = serde_json::from_str(&config.core_config_json).unwrap();
        assert_eq!(value["inbounds"][0]["listen"], "0.0.0.0");

        // Bracketed IPv6 form also accepted.
        settings.inbound_address = "[::]".into();
        let config = build_load_config(&profile, &settings, None).unwrap();
        let value: Value = serde_json::from_str(&config.core_config_json).unwrap();
        assert_eq!(value["inbounds"][0]["listen"], "::");
    }

    #[test]
    fn compiles_bypass_china_like_route() {
        use throne_domain::{DefaultOutbound, RouteProfile, RouteRule, outbound_ids};

        let mut p = Profile::new(1, 1, "n1", ProfileType::Vless);
        p.outbound = ParsedOutbound {
            server: Some("1.2.3.4".into()),
            server_port: Some(443),
            uuid: Some("11111111-1111-1111-1111-111111111111".into()),
            tls: Some(true),
            ..Default::default()
        };

        let mut rp = RouteProfile::new(2, "Bypass China");
        rp.default_outbound = DefaultOutbound::Proxy;
        rp.rules = vec![
            RouteRule {
                name: "dns".into(),
                action: "hijack-dns".into(),
                ..Default::default()
            },
            RouteRule {
                name: "private".into(),
                ip_is_private: true,
                outbound_id: outbound_ids::DIRECT,
                ..Default::default()
            },
            RouteRule {
                name: "anticensor".into(),
                rule_set: vec!["geosite-anticensorship".into()],
                outbound_id: outbound_ids::PROXY,
                ..Default::default()
            },
            RouteRule {
                name: "cn-ip".into(),
                rule_set: vec!["geoip-cn".into()],
                outbound_id: outbound_ids::DIRECT,
                ..Default::default()
            },
            RouteRule {
                name: "cn-site".into(),
                rule_set: vec!["geosite-cn".into()],
                outbound_id: outbound_ids::DIRECT,
                ..Default::default()
            },
            RouteRule {
                name: "suffix-bypass".into(),
                domain_suffix: vec!["cn".into(), "local".into()],
                outbound_id: outbound_ids::DIRECT,
                ..Default::default()
            },
        ];

        let built = build_load_config(&p, &AppSettings::default(), Some(&rp)).unwrap();
        let v: Value = serde_json::from_str(&built.core_config_json).unwrap();
        assert_eq!(v["route"]["final"], "proxy");
        assert_eq!(v["route"]["rules"][0]["action"], "sniff");
        assert_eq!(v["route"]["rules"][1]["action"], "hijack-dns");

        let rules = v["route"]["rules"].as_array().unwrap();
        // Bare hijack-dns from profile should not duplicate (we already inject one).
        let hijack_count = rules
            .iter()
            .filter(|r| r.get("action").and_then(|a| a.as_str()) == Some("hijack-dns"))
            .count();
        assert_eq!(hijack_count, 1, "duplicate hijack-dns: {rules:?}");

        assert!(
            rules.iter().any(|r| {
                r.get("rule_set")
                    .and_then(|x| x.as_array())
                    .is_some_and(|a| a.iter().any(|t| t == "geosite-cn"))
                    && r.get("outbound") == Some(&json!("direct"))
            }),
            "missing geosite-cn→direct: {rules:?}"
        );
        assert!(
            rules.iter().any(|r| {
                r.get("domain_suffix")
                    .and_then(|x| x.as_array())
                    .is_some_and(|a| a.iter().any(|t| t == "cn"))
                    && r.get("outbound") == Some(&json!("direct"))
            }),
            "missing domain_suffix bypass: {rules:?}"
        );

        let sets = v["route"]["rule_set"].as_array().unwrap();
        let tags: Vec<&str> = sets
            .iter()
            .filter_map(|s| s.get("tag").and_then(|t| t.as_str()))
            .collect();
        assert!(tags.contains(&"geosite-cn"), "{tags:?}");
        assert!(tags.contains(&"geoip-cn"), "{tags:?}");
        assert!(tags.contains(&"geosite-anticensorship"), "{tags:?}");
        for s in sets {
            assert_eq!(s["type"], "remote");
            assert_eq!(s["format"], "binary");
            assert_eq!(s["download_detour"], "direct");
            let url = s["url"].as_str().unwrap();
            // Default mirror is Cloudflare jsDelivr (upstream default).
            assert!(
                url.contains("testingcf.jsdelivr.net/gh/")
                    || url.contains("jsdelivr.net/gh/"),
                "{url}"
            );
            assert!(url.ends_with(".srs"), "{url}");
            assert!(
                url.contains("MetaCubeX") || url.contains("Chocolate4U"),
                "{url}"
            );
        }
        // geosite-cn → MetaCubeX path via jsDelivr.
        let cn = sets
            .iter()
            .find(|s| s["tag"] == "geosite-cn")
            .unwrap();
        let cn_url = cn["url"].as_str().unwrap();
        assert!(
            cn_url.contains("MetaCubeX/meta-rules-dat@sing/geo/geosite/cn.srs")
                || cn_url.ends_with("/sing/geo/geosite/cn.srs"),
            "{cn_url}"
        );
    }

    #[test]
    fn jsdelivr_mirror_rewrites_github_raw() {
        let raw = "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/sing/geo/geoip/cn.srs";
        let cf = apply_ruleset_mirror(raw, RulesetMirror::Cloudflare);
        assert_eq!(
            cf,
            "https://testingcf.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@sing/geo/geoip/cn.srs"
        );
        let gh = apply_ruleset_mirror(raw, RulesetMirror::Github);
        assert_eq!(gh, raw);
        // Non-github left alone.
        let other = apply_ruleset_mirror("https://example.com/a.srs", RulesetMirror::Fastly);
        assert_eq!(other, "https://example.com/a.srs");
    }

    #[test]
    fn adblock_injects_reject_rule_set() {
        let mut p = Profile::new(1, 1, "n1", ProfileType::Vless);
        p.outbound = ParsedOutbound {
            server: Some("1.2.3.4".into()),
            server_port: Some(443),
            uuid: Some("11111111-1111-1111-1111-111111111111".into()),
            ..Default::default()
        };
        let mut settings = AppSettings::default();
        settings.adblock_enable = true;
        settings.ruleset_mirror = RulesetMirror::Github;
        let built = build_load_config(&p, &settings, None).unwrap();
        let v: Value = serde_json::from_str(&built.core_config_json).unwrap();
        let sets = v["route"]["rule_set"].as_array().unwrap();
        assert!(
            sets.iter()
                .any(|s| s["tag"] == "throne-adblocksingbox"),
            "{sets:?}"
        );
        let rules = v["route"]["rules"].as_array().unwrap();
        assert!(
            rules.iter().any(|r| {
                r.get("action").and_then(|a| a.as_str()) == Some("reject")
                    && r.get("rule_set")
                        .and_then(|x| x.as_array())
                        .is_some_and(|a| a.iter().any(|t| t == "throne-adblocksingbox"))
            }),
            "{rules:?}"
        );
    }

    #[test]
    fn parses_doh_and_udp_dns_addresses() {
        let doh = build_dns_server_obj("https://dns.google/dns-query");
        assert_eq!(doh["type"], "https");
        assert_eq!(doh["server"], "dns.google");
        assert_eq!(doh["path"], "/dns-query");
        let udp = build_dns_server_obj("1.1.1.1");
        assert_eq!(udp["type"], "udp");
        assert_eq!(udp["server"], "1.1.1.1");
        let local = build_dns_server_obj("localhost");
        assert_eq!(local["type"], "local");
    }

    #[test]
    fn uses_ready_singbox_outbound_json() {
        let mut p = Profile::new(1, 1, "ss", ProfileType::Shadowsocks);
        p.outbound_json = r#"{"type":"shadowsocks","server":"9.9.9.9","server_port":8388,"method":"aes-256-gcm","password":"x","name":"clash-only"}"#.into();
        let built = build_load_config(&p, &AppSettings::default(), None).unwrap();
        let v: Value = serde_json::from_str(&built.core_config_json).unwrap();
        assert_eq!(v["outbounds"][0]["server"], "9.9.9.9");
        assert_eq!(v["outbounds"][0]["tag"], "proxy");
        assert!(v["outbounds"][0].get("name").is_none());
    }

    #[test]
    fn builds_hysteria2_like_user_report() {
        let mut p = Profile::new(1, 1, "hy2", ProfileType::Hysteria2);
        p.outbound_json = r#"{
            "type":"hysteria2",
            "server":"1jp001.example.com",
            "server_port":8443,
            "password":"2029a6f1-96b3-4dd0-9fe0-94b7e296c0c5",
            "tls":{"enabled":true,"insecure":true,"server_name":"localhost"}
        }"#
        .into();
        let built = build_load_config(&p, &AppSettings::default(), None).unwrap();
        let v: Value = serde_json::from_str(&built.core_config_json).unwrap();
        assert_eq!(v["outbounds"][0]["type"], "hysteria2");
        assert_eq!(v["outbounds"][0]["tag"], "proxy");
        assert_eq!(v["outbounds"][0]["tls"]["server_name"], "localhost");
        assert!(v["inbounds"][0].get("sniff").is_none());
    }

    #[test]
    fn url_test_strips_legacy_tcp_transport() {
        // Repro: group URL test failed with
        // `outbounds[1].transport: unknown transport type: tcp`
        // when stored trojan beans included transport.type=tcp from share links.
        let mut trojan = Profile::new(56213, 1, "trojan-tcp", ProfileType::Trojan);
        trojan.outbound_json = r#"{
            "type":"trojan",
            "server":"1.1.1.1",
            "server_port":8080,
            "password":"secret",
            "tls":{"enabled":true,"server_name":"1.1.1.1"},
            "transport":{"type":"tcp"}
        }"#
        .into();
        let mut hy2 = Profile::new(56206, 1, "hy2", ProfileType::Hysteria2);
        hy2.outbound_json = r#"{
            "type":"hysteria2",
            "server":"tw.example.com",
            "server_port":8080,
            "password":"secret",
            "tls":{"enabled":true,"server_name":"localhost","insecure":true}
        }"#
        .into();
        let (cfg, tags) =
            build_url_test_config(&[&trojan, &hy2], &AppSettings::default()).unwrap();
        let v: Value = serde_json::from_str(&cfg).unwrap();
        assert_eq!(tags, vec!["p56213".to_string(), "p56206".to_string()]);
        let outs = v["outbounds"].as_array().unwrap();
        let t = outs.iter().find(|o| o["tag"] == "p56213").unwrap();
        assert_eq!(t["type"], "trojan");
        assert!(
            t.get("transport").is_none(),
            "tcp transport must be stripped for core decode: {t}"
        );
    }

    #[test]
    fn from_parsed_trojan_tcp_omits_transport() {
        let mut p = Profile::new(1, 1, "t", ProfileType::Trojan);
        p.outbound = ParsedOutbound {
            server: Some("1.1.1.1".into()),
            server_port: Some(443),
            password: Some("pw".into()),
            transport: Some("tcp".into()),
            tls: Some(true),
            sni: Some("1.1.1.1".into()),
            ..Default::default()
        };
        // Empty outbound_json forces from_parsed path.
        p.outbound_json = String::new();
        let built = build_load_config(&p, &AppSettings::default(), None).unwrap();
        let v: Value = serde_json::from_str(&built.core_config_json).unwrap();
        let proxy = &v["outbounds"][0];
        assert_eq!(proxy["type"], "trojan");
        assert!(proxy.get("transport").is_none(), "{proxy}");
    }

    #[test]
    fn take_singbox_normalizes_websocket_transport_type() {
        let mut p = Profile::new(1, 1, "v", ProfileType::Vmess);
        p.outbound_json = r#"{
            "type":"vmess",
            "server":"1.2.3.4",
            "server_port":443,
            "uuid":"u",
            "transport":{"type":"websocket","path":"/ray"}
        }"#
        .into();
        let built = build_load_config(&p, &AppSettings::default(), None).unwrap();
        let v: Value = serde_json::from_str(&built.core_config_json).unwrap();
        assert_eq!(v["outbounds"][0]["transport"]["type"], "ws");
    }
}
