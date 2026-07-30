//! Build a minimal sing-box core config from a Throne profile + settings.
//!
//! This is intentionally smaller than upstream `BuildSingBoxConfig` but enough
//! to make mixed-inbound proxy actually work for common outbound types.

use serde_json::{Map, Value, json};
use throne_domain::{
    AppSettings, DefaultOutbound, ParsedOutbound, Profile, ProfileType, RouteProfile, RouteRule,
    RulesetMirror,
};

use crate::CoreError;

/// Result of building a core load request payload.
#[derive(Debug, Clone)]
pub struct BuiltConfig {
    pub core_config_json: String,
    pub need_xray: bool,
    pub xray_config: String,
    /// Upstream `LoadConfigReq.tun_ipv4_cidr` — set when Tun inbound is present.
    /// Empty when Tun is off. Core uses this on Darwin to set system DNS to tunIP+1.
    pub tun_ipv4_cidr: String,
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
    let outbound = build_proxy_outbound(profile)?;
    // Always bind mixed inbound on IPv4 loopback for system-proxy compatibility.
    // Upstream often stores `::`; browsers + networksetup use 127.0.0.1.
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
    let mut tun_ipv4_cidr = String::new();
    if settings.tun_mode_enabled {
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
        // Darwin only accepts utunN (or empty = OS assigns). Linux/Windows accept
        // arbitrary names; "throne-tun" is rejected on macOS as "bad tun name".
        if let Some(name) = default_tun_interface_name() {
            tun.as_object_mut()
                .unwrap()
                .insert("interface_name".into(), json!(name));
        }
        // Linux: newer kernels need auto_redirect for system/mixed stacks (upstream default on).
        #[cfg(target_os = "linux")]
        {
            tun.as_object_mut()
                .unwrap()
                .insert("auto_redirect".into(), json!(true));
        }
        if !settings.disable_private_range_bypass {
            tun.as_object_mut().unwrap().insert(
                "route_exclude_address".into(),
                json!([
                    "127.0.0.0/8",
                    "10.0.0.0/8",
                    "172.16.0.0/12",
                    "192.168.0.0/16",
                    "169.254.0.0/16",
                    "224.0.0.0/4",
                    "255.255.255.255/32"
                ]),
            );
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

    let (dns, default_resolver_tag) = build_dns_section(settings, settings.tun_mode_enabled);

    let config = json!({
        "log": { "level": log_level, "timestamp": true },
        "dns": dns,
        "inbounds": inbounds,
        "outbounds": [
            outbound,
            { "type": "direct", "tag": "direct" }
        ],
        "route": {
            "rules": route_rules,
            "rule_set": rule_sets,
            "final": route_final,
            "auto_detect_interface": true,
            "default_domain_resolver": {
                "server": default_resolver_tag,
                "strategy": "prefer_ipv4"
            }
        },
        // clash_api enables TrafficManager used by QueryStats / QueryConnections.
        "experimental": {
            "clash_api": {
                "external_controller": "127.0.0.1:0",
                "default_mode": ""
            },
            "cache_file": {
                "enabled": true
            }
        }
    });

    let core_config_json =
        serde_json::to_string(&config).map_err(|e| CoreError::Config(e.to_string()))?;

    Ok(BuiltConfig {
        core_config_json,
        need_xray: false,
        xray_config: String::new(),
        tun_ipv4_cidr,
    })
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

fn normalize_listen_address(addr: &str) -> String {
    match addr.trim() {
        "" | "*" | "::" | "[::]" | "::0" | "localhost" => "127.0.0.1".into(),
        value => value
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|_| "127.0.0.1".into()),
    }
}

fn build_proxy_outbound(profile: &Profile) -> Result<Value, CoreError> {
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
    obj.insert("tag".into(), json!("proxy"));
    Some(v)
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
    let transport = o.transport.as_deref().unwrap_or("tcp");
    match transport {
        "ws" | "websocket" => {
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
        "http" | "h2" => {
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

/// Build DNS object + tag used by `route.default_domain_resolver`.
///
/// Upstream: on Darwin + Tun, `type: local` is forbidden (DNS loops into the
/// tunnel). Use remote DoH via proxy + a concrete UDP "underlying" IP for
/// bootstrap / direct (see `core_box_underlying_dns`).
fn build_dns_section(settings: &AppSettings, tun_enabled: bool) -> (Value, &'static str) {
    if !tun_enabled {
        return (
            json!({
                "servers": [ { "type": "local", "tag": "local" } ],
                "final": "local",
                "strategy": "prefer_ipv4"
            }),
            "local",
        );
    }

    // Tun path — never use OS `local` resolver as final on macOS.
    let underlying = resolve_underlying_dns(settings);
    // Upstream generate.cpp: only dns-remote gets detour=proxy.
    // dns-direct must NOT set detour=direct — sing-box rejects
    // "detour to an empty direct outbound makes no sense".
    let mut remote = build_dns_server_obj(&settings.remote_dns);
    if let Some(obj) = remote.as_object_mut() {
        obj.insert("tag".into(), json!("dns-remote"));
        obj.insert("detour".into(), json!("proxy"));
        obj.insert("domain_resolver".into(), json!("dns-local"));
    }

    let mut direct = build_dns_server_obj(&underlying);
    // Force UDP IP for direct/underlying — never `local` under Tun.
    if direct.get("type").and_then(|t| t.as_str()) == Some("local") {
        direct = json!({ "type": "udp", "server": underlying });
    }
    if let Some(obj) = direct.as_object_mut() {
        obj.insert("tag".into(), json!("dns-direct"));
        obj.insert("domain_resolver".into(), json!("dns-local"));
    }

    // Bootstrap resolver — no detour (plain dial), same as upstream dns-local.
    let local = json!({
        "type": "udp",
        "tag": "dns-local",
        "server": underlying
    });

    let final_tag = match settings.dns_final_out.trim().to_ascii_lowercase().as_str() {
        "direct" => "dns-direct",
        _ => "dns-remote",
    };

    (
        json!({
            "servers": [remote, direct, local],
            "final": final_tag,
            "strategy": "prefer_ipv4"
        }),
        "dns-local",
    )
}

/// Upstream `core_box_underlying_dns`, with a safe public fallback when empty
/// (Darwin Tun requires a real IP — empty is a hard error in Qt Throne).
fn resolve_underlying_dns(settings: &AppSettings) -> String {
    let t = settings.core_box_underlying_dns.trim();
    if !t.is_empty()
        && !t.eq_ignore_ascii_case("local")
        && !t.eq_ignore_ascii_case("localhost")
    {
        return t.to_string();
    }
    let d = settings.direct_dns.trim();
    if !d.is_empty()
        && !d.eq_ignore_ascii_case("local")
        && !d.eq_ignore_ascii_case("localhost")
        && !d.contains("://")
    {
        // bare IP / host OK
        if d.parse::<std::net::Ipv4Addr>().is_ok() {
            return d.to_string();
        }
    }
    // Public resolver used only as bootstrap for domain_resolver / direct DNS.
    "1.1.1.1".into()
}

fn build_dns_server_obj(address: &str) -> Value {
    let address = address.trim();
    if address.is_empty() || address.eq_ignore_ascii_case("local") || address == "localhost" {
        return json!({ "type": "local" });
    }

    let (ty, rest) = if let Some(rest) = address.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = address.strip_prefix("http://") {
        ("http", rest)
    } else if let Some(rest) = address.strip_prefix("udp://") {
        ("udp", rest)
    } else if let Some(rest) = address.strip_prefix("tls://") {
        ("tls", rest)
    } else if let Some(rest) = address.strip_prefix("quic://") {
        ("quic", rest)
    } else {
        ("udp", address)
    };
    let (host_port, path) = if ty == "https" || ty == "http" {
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

#[cfg(test)]
mod tests {
    use super::*;
    use throne_domain::{ParsedOutbound, Profile, ProfileType, RulesetMirror};

    #[test]
    fn tun_inbound_uses_platform_safe_interface_name() {
        let mut p = Profile::new(1, 1, "n1", ProfileType::Vless);
        p.outbound = ParsedOutbound {
            server: Some("1.2.3.4".into()),
            server_port: Some(443),
            uuid: Some("11111111-1111-1111-1111-111111111111".into()),
            ..Default::default()
        };
        let mut settings = AppSettings::default();
        settings.tun_mode_enabled = true;
        let built = build_load_config(&p, &settings, None).unwrap();
        let v: Value = serde_json::from_str(&built.core_config_json).unwrap();
        let tun = v["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|ib| ib["type"] == "tun")
            .expect("tun inbound");
        assert_eq!(tun["tag"], "tun-in");
        assert_eq!(tun["address"][0], "172.19.0.1/24");
        assert_eq!(built.tun_ipv4_cidr, "172.19.0.1/24");
        // Tun must not use OS `local` DNS as final (Darwin loops into the tunnel).
        assert_ne!(v["dns"]["final"], "local");
        assert_eq!(v["route"]["default_domain_resolver"]["server"], "dns-local");
        let servers = v["dns"]["servers"].as_array().unwrap();
        assert!(
            servers.iter().any(|s| s["tag"] == "dns-remote"),
            "expected dns-remote: {servers:?}"
        );
        assert!(
            servers
                .iter()
                .any(|s| s["tag"] == "dns-local" && s["type"] != "local"),
            "dns-local must be concrete under Tun: {servers:?}"
        );
        // sing-box rejects detour=direct on DNS servers.
        for s in servers {
            if s["tag"] == "dns-direct" || s["tag"] == "dns-local" {
                assert!(
                    s.get("detour").is_none(),
                    "dns server must not detour=direct: {s}"
                );
            }
        }
        #[cfg(target_os = "macos")]
        {
            // Empty/omitted name — kernel assigns utunN. "throne-tun" is invalid on Darwin.
            assert!(
                tun.get("interface_name").is_none()
                    || tun["interface_name"].as_str().is_some_and(|n| n.is_empty() || n.starts_with("utun")),
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
        let mut settings = AppSettings::default();
        settings.inbound_address = "::".into(); // upstream default on some installs
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
        assert_eq!(v["inbounds"][0]["listen"], "127.0.0.1");
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
        let mut settings = AppSettings::default();
        settings.inbound_address = "0.0.0.0".into();

        let config = build_load_config(&profile, &settings, None).unwrap();
        let value: Value = serde_json::from_str(&config.core_config_json).unwrap();
        assert_eq!(value["inbounds"][0]["listen"], "0.0.0.0");
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
}
