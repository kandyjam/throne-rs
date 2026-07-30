//! Route profile share import — upstream `RouteProfile::FromShareInput` /
//! `FromRemoteRoutesLink` (`kind: throne-route-profile`, `throne://route/…`).

use serde_json::Value;
use throne_domain::{DefaultOutbound, RouteProfile, RouteRule};

use crate::decode::decode_b64_flexible;
use crate::deeplink::{Deeplink, parse_deeplink};

#[derive(Debug, Clone, Default)]
pub struct RouteImportReport {
    pub routes: Vec<RouteProfile>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
}

/// Try to interpret text as one or more route profiles (not node share links).
pub fn try_import_routes(input: &str) -> Option<RouteImportReport> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(dl) = parse_deeplink(trimmed) {
        match dl {
            Deeplink::Route { payload } => {
                let mut report = import_route_payload(&payload);
                report.notes.push("throne://route/ deep link".into());
                return Some(report);
            }
            Deeplink::RemoteRoute { payload } => {
                let mut report = import_route_payload_as_remote(&payload);
                report.notes.push("throne://remoteRoute/ deep link".into());
                return Some(report);
            }
            _ => return None,
        }
    }

    if trimmed.contains("throne-route-profile") {
        return Some(import_route_payload(trimmed));
    }
    if let Some(decoded) = decode_b64_flexible(trimmed) {
        if decoded.contains("throne-route-profile") {
            let mut report = import_route_payload(&decoded);
            report.notes.push("base64 route payload".into());
            return Some(report);
        }
    }
    None
}

pub fn import_route_payload(text: &str) -> RouteImportReport {
    let mut report = RouteImportReport::default();
    let doc = match parse_json_doc(text) {
        Some(d) => d,
        None => {
            report
                .errors
                .push("Input is not valid JSON, base64, or a Throne route link".into());
            return report;
        }
    };

    if let Some(obj) = doc.as_object() {
        if obj.get("kind").and_then(|v| v.as_str()) != Some("throne-route-profile") {
            report
                .errors
                .push("Unrecognized route object (missing kind=throne-route-profile)".into());
            return report;
        }
        match profile_from_share_object(obj, &mut report.warnings) {
            Some(p) => report.routes.push(p),
            None => report.errors.push("failed to parse route profile".into()),
        }
        return report;
    }

    if let Some(arr) = doc.as_array() {
        let mut p = RouteProfile::new(0, "Imported rules");
        p.rules = rules_from_array(arr, &mut report.warnings);
        report.notes.push("legacy bare rule array".into());
        report.routes.push(p);
        return report;
    }

    report.errors.push("Unsupported route input".into());
    report
}

pub fn import_route_payload_as_remote(payload: &str) -> RouteImportReport {
    import_remote_route_payload(payload)
}

fn import_remote_route_payload(payload: &str) -> RouteImportReport {
    let mut report = RouteImportReport::default();
    if let Ok(doc) = serde_json::from_str::<Value>(payload) {
        if let Some(arr) = doc.as_array() {
            for item in arr {
                if let Some(p) = remote_from_value(item) {
                    report.routes.push(p);
                }
            }
        } else if let Some(p) = remote_from_value(&doc) {
            report.routes.push(p);
        } else if let Some(url) = doc.as_str() {
            report.routes.push(remote_profile(url, None, true));
        }
    } else {
        for line in payload.lines() {
            let u = line.trim();
            if u.starts_with("http://") || u.starts_with("https://") {
                report.routes.push(remote_profile(u, None, true));
            }
        }
    }
    if report.routes.is_empty() {
        report
            .errors
            .push("remoteRoute payload had no URLs".into());
    }
    report
}

fn remote_from_value(v: &Value) -> Option<RouteProfile> {
    if let Some(url) = v.as_str() {
        return Some(remote_profile(url, None, true));
    }
    let obj = v.as_object()?;
    let url = obj.get("url")?.as_str()?;
    let name = obj.get("name").and_then(|n| n.as_str());
    let auto = obj
        .get("autoUpdate")
        .or_else(|| obj.get("auto_update"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    Some(remote_profile(url, name, auto))
}

fn remote_profile(url: &str, name: Option<&str>, auto_update: bool) -> RouteProfile {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "remote".into());
    let mut p = RouteProfile::new(0, name.unwrap_or(&host));
    p.is_remote = true;
    p.remote_url = url.to_string();
    p.auto_update = auto_update;
    p
}

fn profile_from_share_object(
    root: &serde_json::Map<String, Value>,
    warnings: &mut Vec<String>,
) -> Option<RouteProfile> {
    let name = root
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Imported route")
        .to_string();
    let mut p = RouteProfile::new(0, name);

    if root.get("raw").and_then(|v| v.as_bool()).unwrap_or(false) {
        p.is_raw = true;
        p.prevent_modifications = root
            .get("prevent_modifications")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if let Some(route) = root.get("route") {
            p.raw_route = serde_json::to_string_pretty(route).unwrap_or_default();
        } else {
            warnings.push("raw route missing `route` object".into());
        }
        return Some(p);
    }

    if let Some(tok) = root.get("default_outbound").and_then(|v| v.as_str()) {
        p.default_outbound = DefaultOutbound::from_share_token(tok);
    } else if let Some(n) = root.get("default_outbound").and_then(|v| v.as_i64()) {
        p.default_outbound = DefaultOutbound::from_id(n);
    }

    if let Some(arr) = root.get("rules").and_then(|v| v.as_array()) {
        p.rules = rules_from_array(arr, warnings);
    }
    Some(p)
}

fn rules_from_array(arr: &[Value], warnings: &mut Vec<String>) -> Vec<RouteRule> {
    let mut rules = Vec::new();
    for (i, v) in arr.iter().enumerate() {
        let Some(obj) = v.as_object() else {
            warnings.push(format!("rule[{i}] is not an object"));
            continue;
        };
        let token = obj
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("custom")
            .to_string();
        let type_int = obj
            .get("type")
            .and_then(|v| v.as_i64())
            .map(|n| n as i32)
            .unwrap_or_else(|| RouteRule::type_from_token(&token));
        let outbound_id = obj
            .get("outbound")
            .and_then(|v| v.as_str())
            .map(|s| DefaultOutbound::from_share_token(s).as_id())
            .or_else(|| {
                obj.get("outbound")
                    .or_else(|| obj.get("outboundID"))
                    .and_then(|v| v.as_i64())
            })
            .unwrap_or(DefaultOutbound::Proxy.as_id());

        let mut rule = RouteRule {
            name: obj
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("rule_{}", i + 1)),
            rule_type: type_int,
            rule_type_token: if token.parse::<i64>().is_ok() {
                RouteRule::token_from_type(type_int).to_string()
            } else {
                token
            },
            outbound_id,
            invert: obj
                .get("invert")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            action: obj
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("route")
                .to_string(),
            network: obj
                .get("network")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            protocol: obj
                .get("protocol")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            ip_version: obj
                .get("ip_version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            ..Default::default()
        };
        rule.domain = string_list(obj.get("domain"));
        rule.domain_suffix = string_list(obj.get("domain_suffix"));
        rule.domain_keyword = string_list(obj.get("domain_keyword"));
        rule.domain_regex = string_list(obj.get("domain_regex"));
        rule.ip_cidr = string_list(obj.get("ip_cidr"));
        rule.process_name = string_list(obj.get("process_name"));
        rule.process_path = string_list(obj.get("process_path"));
        rule.inbound = string_list(obj.get("inbound"));
        rule.rule_set = string_list(obj.get("rule_set"));
        rules.push(rule);
    }
    rules
}

fn string_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

fn parse_json_doc(text: &str) -> Option<Value> {
    if let Ok(v) = serde_json::from_str(text) {
        return Some(v);
    }
    if let Some(decoded) = decode_b64_flexible(text) {
        if let Ok(v) = serde_json::from_str(&decoded) {
            return Some(v);
        }
    }
    None
}

/// Build upstream-compatible share object for export.
pub fn to_share_object(profile: &RouteProfile) -> Value {
    if profile.is_raw {
        let route: Value =
            serde_json::from_str(&profile.raw_route).unwrap_or(Value::Object(Default::default()));
        return serde_json::json!({
            "kind": "throne-route-profile",
            "v": 1,
            "name": profile.name,
            "raw": true,
            "prevent_modifications": profile.prevent_modifications,
            "route": route,
        });
    }
    let rules: Vec<Value> = profile
        .rules
        .iter()
        .map(|r| {
            let token = if r.rule_type_token.is_empty() {
                RouteRule::token_from_type(r.rule_type).to_string()
            } else {
                r.rule_type_token.clone()
            };
            serde_json::json!({
                "name": r.name,
                "type": token,
                "outbound": DefaultOutbound::from_id(r.outbound_id).to_share_token(),
                "domain": r.domain,
                "domain_suffix": r.domain_suffix,
                "domain_keyword": r.domain_keyword,
                "ip_cidr": r.ip_cidr,
                "process_name": r.process_name,
                "network": r.network,
                "invert": r.invert,
            })
        })
        .collect();
    serde_json::json!({
        "kind": "throne-route-profile",
        "v": 1,
        "name": profile.name,
        "default_outbound": profile.default_outbound.to_share_token(),
        "rules": rules,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn parse_share_object() {
        let json = r#"{
            "kind":"throne-route-profile",
            "v":1,
            "name":"Home",
            "default_outbound":"proxy",
            "rules":[
                {"name":"ads","type":"simple_address_block","outbound":"block","domain_suffix":["ads.example"]}
            ]
        }"#;
        let r = import_route_payload(json);
        assert_eq!(r.routes.len(), 1);
        assert_eq!(r.routes[0].name, "Home");
        assert_eq!(r.routes[0].rules.len(), 1);
        assert_eq!(r.routes[0].rules[0].domain_suffix[0], "ads.example");
        assert_eq!(r.routes[0].rules[0].outbound_id, DefaultOutbound::Block.as_id());
    }

    #[test]
    fn parse_route_deeplink() {
        let json = r#"{"kind":"throne-route-profile","v":1,"name":"DL","default_outbound":"direct","rules":[]}"#;
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes());
        let r = try_import_routes(&format!("throne://route/{b64}")).unwrap();
        assert_eq!(r.routes[0].name, "DL");
        assert_eq!(r.routes[0].default_outbound, DefaultOutbound::Direct);
    }
}
