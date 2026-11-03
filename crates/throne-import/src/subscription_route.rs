//! Subscription routing import (upstream 1.4.0-beta.5 `RouteUpdate`).
//!
//! A group with `import_sub_route` takes the `routing` response header, or the
//! first shipped `scheme://routing/` line in the body. `off` imports nothing.
//! Throne shares drop raw profiles and rules that name a local server. Happ
//! documents become simple rules plus a private-IP rule.

use serde_json::{Map, Value};
use throne_domain::{outbound_ids, DefaultOutbound, RouteProfile, RouteRule, SimpleAction};
use urlencoding::decode as percent_decode;

use crate::decode::decode_b64_flexible;
use crate::deeplink::{parse_deeplink, Deeplink};
use crate::route_share::import_route_payload;

const ENDPOINT_PREFERRED_BY: i32 = 13;
const DNS_HIJACK_OUTBOUND: i64 = -4;

#[derive(Debug, Clone)]
pub struct SubscriptionRouteParse {
    pub profile: RouteProfile,
    pub imported: usize,
    pub dropped: usize,
}

/// `None` means the header is empty or `off` and the body has no shipped link.
pub fn subscription_routing_value(header: Option<&str>, body: &str) -> Option<String> {
    let header = header.unwrap_or("").trim();
    if !header.is_empty() && !header.eq_ignore_ascii_case("off") {
        return Some(header.to_string());
    }
    shipped_routing_link(body)
}

/// First line whose scheme is non-empty and whose remainder is `://routing/`.
pub fn shipped_routing_link(body: &str) -> Option<String> {
    let text = maybe_decode_subscription_body(body);
    for line in text.lines() {
        let line = line.trim();
        if is_routing_link(line) {
            return Some(line.to_string());
        }
    }
    None
}

/// Parse one routing value. `Ok(None)` is an intentional `off` / empty value.
pub fn parse_subscription_route(text: &str) -> Result<Option<SubscriptionRouteParse>, String> {
    let text = text.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("off") {
        return Ok(None);
    }
    if text.len() >= "throne://route/".len()
        && text[.."throne://route/".len()].eq_ignore_ascii_case("throne://route/")
    {
        return from_throne_share(text).map(Some);
    }
    let text = strip_routing_verb(text)?;
    let Some(text) = text else {
        return Ok(None);
    };
    if text.is_empty() {
        return Ok(None);
    }
    let json = if text.starts_with('{') {
        text.to_string()
    } else {
        decode_lenient_base64(&text)
            .ok_or_else(|| "the routing profile is not valid base64".to_string())?
    };
    let document: Value = serde_json::from_str(&json)
        .map_err(|_| "the routing profile is not a JSON object".to_string())?;
    let Some(object) = document.as_object() else {
        return Err("the routing profile is not a JSON object".into());
    };
    from_happ(object).map(Some)
}

fn strip_routing_verb(text: &str) -> Result<Option<String>, String> {
    let lower = text.to_ascii_lowercase();
    let Some(marker) = lower.find("://routing/") else {
        return Ok(Some(text.to_string()));
    };
    let rest = &text[marker + "://routing/".len()..];
    let (verb, payload) = match rest.find('/') {
        Some(slash) => (&rest[..slash], Some(&rest[slash + 1..])),
        None => (rest, None),
    };
    let verb = verb.trim().to_ascii_lowercase();
    if verb == "off" {
        return Ok(None);
    }
    if verb != "add" && verb != "onadd" {
        return Err(format!("unknown routing link action \"{verb}\""));
    }
    let Some(payload) = payload else {
        return Ok(None);
    };
    let decoded = percent_decode(payload.trim())
        .map(|cow| cow.into_owned())
        .unwrap_or_else(|_| payload.trim().to_string());
    Ok(Some(decoded.trim().to_string()))
}

fn from_throne_share(text: &str) -> Result<SubscriptionRouteParse, String> {
    let payload = match parse_deeplink(text) {
        Some(Deeplink::Route { payload }) => payload,
        _ => text.to_string(),
    };
    let report = import_route_payload(&payload);
    let Some(mut profile) = report.routes.into_iter().next() else {
        let why = report
            .errors
            .first()
            .cloned()
            .unwrap_or_else(|| "could not parse a routing profile from the response".into());
        return Err(why);
    };
    if profile.is_raw {
        return Err("a raw routing profile is not accepted from a subscription".into());
    }
    profile.custom_dns = false;
    profile.dns = None;
    profile.endpoint_profile_ids.clear();
    profile.inner_hop_endpoint_ids.clear();
    let mut imported = 0usize;
    let mut dropped = 0usize;
    profile.rules.retain(|rule| {
        if subscription_safe_rule(rule) {
            imported += 1;
            true
        } else {
            dropped += 1;
            false
        }
    });
    if !builtin_outbound(profile.default_outbound.as_id()) {
        profile.default_outbound = DefaultOutbound::Proxy;
    }
    Ok(SubscriptionRouteParse {
        profile,
        imported,
        dropped,
    })
}

fn subscription_safe_rule(rule: &RouteRule) -> bool {
    if rule.rule_type == ENDPOINT_PREFERRED_BY {
        return false;
    }
    if !rule.override_address.is_empty() || !rule.override_port.is_empty() {
        return false;
    }
    if rule.action == "route" || rule.action == "bypass" {
        return builtin_outbound(rule.outbound_id) || rule.outbound_id == DNS_HIJACK_OUTBOUND;
    }
    matches!(
        rule.action.as_str(),
        "reject" | "hijack-dns" | "sniff" | "resolve" | "route-options"
    )
}

fn builtin_outbound(id: i64) -> bool {
    matches!(
        id,
        outbound_ids::PROXY | outbound_ids::DIRECT | outbound_ids::BLOCK
    )
}

fn from_happ(object: &Map<String, Value>) -> Result<SubscriptionRouteParse, String> {
    const KEYS: [&str; 8] = [
        "Name",
        "GlobalProxy",
        "DirectSites",
        "DirectIp",
        "ProxySites",
        "ProxyIp",
        "BlockSites",
        "BlockIp",
    ];
    if KEYS.iter().all(|key| happ_value(object, key).is_none()) {
        return Err("the routing profile is not in the Happ format".into());
    }
    let mut profile = RouteProfile::new(
        -1,
        happ_value(object, "Name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim(),
    );
    let global = happ_value(object, "GlobalProxy");
    let global_proxy = match global {
        Some(Value::Bool(value)) => *value,
        Some(other) => !other
            .as_str()
            .unwrap_or("")
            .trim()
            .eq_ignore_ascii_case("false"),
        None => true,
    };
    profile.default_outbound = if global_proxy {
        DefaultOutbound::Proxy
    } else {
        DefaultOutbound::Direct
    };

    let mut order = Vec::new();
    let mut push_action = |action: SimpleAction| {
        if !order.contains(&action) {
            order.push(action);
        }
    };
    for word in route_order_words(happ_value(object, "RouteOrder")) {
        match word.as_str() {
            "block" => push_action(SimpleAction::Block),
            "proxy" => push_action(SimpleAction::Proxy),
            "direct" => push_action(SimpleAction::Bypass),
            _ => {}
        }
    }
    for action in [
        SimpleAction::Block,
        SimpleAction::Proxy,
        SimpleAction::Bypass,
    ] {
        push_action(action);
    }

    let mut imported = 0usize;
    let mut dropped = 0usize;
    for action in order {
        let (sites_key, ip_key) = match action {
            SimpleAction::Block => ("BlockSites", "BlockIp"),
            SimpleAction::Proxy => ("ProxySites", "ProxyIp"),
            SimpleAction::Bypass => ("DirectSites", "DirectIp"),
            SimpleAction::WarpBypass => continue,
        };
        let mut private_ip = false;
        let mut lines = Vec::new();
        for ip_list in [false, true] {
            let key = if ip_list { ip_key } else { sites_key };
            for item in happ_list(happ_value(object, key)) {
                match happ_entry(&item, ip_list) {
                    HappEntry::Skip => {}
                    HappEntry::Dropped => dropped += 1,
                    HappEntry::PrivateIp => private_ip = true,
                    HappEntry::Line(line) => lines.push(line),
                }
            }
        }
        if private_ip {
            profile.rules.push(private_ip_rule(action));
            imported += 1;
        }
        for line in lines {
            if profile.append_simple_rule(&line, action).is_ok() {
                imported += 1;
            } else {
                dropped += 1;
            }
        }
    }
    Ok(SubscriptionRouteParse {
        profile,
        imported,
        dropped,
    })
}

fn private_ip_rule(action: SimpleAction) -> RouteRule {
    let (name, outbound_id, action_name) = match action {
        SimpleAction::Block => ("Private IP Block", outbound_ids::BLOCK, "reject"),
        SimpleAction::Proxy => ("Private IP Proxy", outbound_ids::PROXY, "route"),
        SimpleAction::Bypass => ("Private IP Bypass", outbound_ids::DIRECT, "route"),
        SimpleAction::WarpBypass => ("Private IP Bypass", outbound_ids::DIRECT, "route"),
    };
    RouteRule {
        name: name.into(),
        ip_is_private: true,
        outbound_id,
        action: action_name.into(),
        ..RouteRule::default()
    }
}

enum HappEntry {
    Skip,
    Dropped,
    PrivateIp,
    Line(String),
}

fn happ_entry(raw: &str, ip_list: bool) -> HappEntry {
    let entry = raw.trim();
    if entry.is_empty() {
        return HappEntry::Skip;
    }
    if entry.starts_with('!') || entry.contains('@') {
        return HappEntry::Dropped;
    }
    if (entry.contains('.') || entry.contains(':')) && is_ip_or_cidr(entry) {
        return HappEntry::Line(format!("ip:{entry}"));
    }
    let Some((prefix, value)) = entry.split_once(':') else {
        if ip_list {
            return HappEntry::Dropped;
        }
        return HappEntry::Line(format!("keyword:{}", entry.to_ascii_lowercase()));
    };
    let prefix = prefix.trim().to_ascii_lowercase();
    let value = value.trim();
    if value.is_empty() {
        return HappEntry::Dropped;
    }
    if prefix == "geoip" && value.eq_ignore_ascii_case("private") {
        return HappEntry::PrivateIp;
    }
    let line = if prefix == "geosite" || prefix == "geoip" {
        format!("ruleset:{prefix}-{}", value.to_ascii_lowercase())
    } else if prefix == "domain" {
        format!("suffix:{}", value.to_ascii_lowercase())
    } else if prefix == "full" {
        format!("domain:{}", value.to_ascii_lowercase())
    } else if prefix == "regexp" {
        format!("regex:{value}")
    } else if prefix == "keyword" {
        format!("keyword:{}", value.to_ascii_lowercase())
    } else {
        return HappEntry::Dropped;
    };
    HappEntry::Line(line)
}

fn is_ip_or_cidr(entry: &str) -> bool {
    if let Some((addr, prefix)) = entry.split_once('/') {
        let Ok(bits) = prefix.parse::<u8>() else {
            return false;
        };
        match addr.parse::<std::net::IpAddr>() {
            Ok(std::net::IpAddr::V4(_)) => bits <= 32,
            Ok(std::net::IpAddr::V6(_)) => bits <= 128,
            Err(_) => false,
        }
    } else {
        entry.parse::<std::net::IpAddr>().is_ok()
    }
}

fn route_order_words(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(|word| word.trim().to_ascii_lowercase())
            .filter(|word| !word.is_empty())
            .collect(),
        Some(Value::String(text)) => text
            .split(|ch: char| !ch.is_ascii_alphabetic())
            .map(|word| word.trim().to_ascii_lowercase())
            .filter(|word| !word.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn happ_value<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    object
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value)
}

fn happ_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(text)) => text
            .split(',')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn decode_lenient_base64(text: &str) -> Option<String> {
    let mut bytes = String::new();
    for ch in text.chars() {
        if ch.is_whitespace() {
            continue;
        }
        match ch {
            '-' => bytes.push('+'),
            '_' => bytes.push('/'),
            _ => bytes.push(ch),
        }
    }
    while bytes.ends_with('=') {
        bytes.pop();
    }
    while bytes.len() % 4 != 0 {
        bytes.push('=');
    }
    decode_b64_flexible(&bytes)
}

fn maybe_decode_subscription_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() || trimmed.contains("://") {
        return trimmed.to_string();
    }
    decode_b64_flexible(trimmed).unwrap_or_else(|| trimmed.to_string())
}

fn is_routing_link(line: &str) -> bool {
    let Some(sep) = line.find("://") else {
        return false;
    };
    if sep == 0 {
        return false;
    }
    let scheme = &line[..sep];
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '.' | '-')) {
        return false;
    }
    line[sep..].len() >= "://routing/".len()
        && line[sep..sep + "://routing/".len()].eq_ignore_ascii_case("://routing/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_and_empty_import_nothing() {
        assert!(parse_subscription_route("").unwrap().is_none());
        assert!(parse_subscription_route(" Off ").unwrap().is_none());
        assert!(parse_subscription_route("happ://routing/off")
            .unwrap()
            .is_none());
    }

    #[test]
    fn shipped_link_is_used_when_the_header_is_off() {
        let body = "vless://example\nhapp://routing/onadd/eyJHbG9iYWxQcm94eSI6ZmFsc2V9\n";
        let value = subscription_routing_value(Some("off"), body).unwrap();
        assert!(value.starts_with("happ://routing/"));
        let parsed = parse_subscription_route(&value).unwrap().unwrap();
        assert_eq!(parsed.profile.default_outbound, DefaultOutbound::Direct);
    }

    #[test]
    fn happ_excludes_private_ip_and_maps_entries() {
        let raw = r#"{
            "Name": "Happ",
            "GlobalProxy": "false",
            "RouteOrder": "proxy,block",
            "ProxySites": ["geosite:google", "example.com", "!drop", "geoip:private"],
            "BlockIp": ["10.0.0.0/8", "not-an-ip"]
        }"#;
        let parsed = parse_subscription_route(raw).unwrap().unwrap();
        assert_eq!(parsed.profile.name, "Happ");
        assert_eq!(parsed.profile.default_outbound, DefaultOutbound::Direct);
        assert!(parsed.profile.rules.iter().any(|rule| rule.ip_is_private));
        assert!(parsed.imported >= 2);
        assert!(parsed.dropped >= 2);
        let proxy_first = parsed
            .profile
            .rules
            .iter()
            .position(|rule| rule.name.contains("Proxy") || rule.rule_type == 1);
        let block_pos = parsed
            .profile
            .rules
            .iter()
            .position(|rule| rule.action == "reject" || rule.name.contains("Block"));
        assert!(proxy_first.unwrap() < block_pos.unwrap());
    }

    #[test]
    fn throne_share_rejects_raw_and_unsafe_rules() {
        let raw = r#"{"kind":"throne-route-profile","name":"Shared","default_outbound":"profile","rules":[
            {"type":"custom","action":"route","outbound":-1,"domain":["a.com"]},
            {"type":"custom","action":"route","outbound":7,"domain":["b.com"]},
            {"type":"custom","action":"hijack-dns","protocol":"dns"}
        ]}"#;
        let link = format!("throne://route/{}", throne_share_b64(raw));
        let parsed = parse_subscription_route(&link).unwrap().unwrap();
        assert_eq!(parsed.profile.default_outbound, DefaultOutbound::Proxy);
        assert_eq!(parsed.imported, 2);
        assert_eq!(parsed.dropped, 1);
        assert!(parsed.profile.dns.is_none());
    }

    fn throne_share_b64(json: &str) -> String {
        use base64::Engine;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes())
    }
}
