//! DNSTT and MasterDNS identity and URL-test startup allowance.
//!
//! Upstream `DnsTunnelStartupAllowanceMs` adds 30s for DNSTT (and Xray xdns)
//! and 90s for MasterDNS. Chain inner hops and the xdns form stay unwired.

use serde_json::{json, Value};

use crate::ProfileType;

/// Extra milliseconds a URL test waits for this profile's tunnel to come up.
pub fn dns_tunnel_startup_allowance_ms(profile_type: ProfileType) -> u64 {
    match profile_type {
        ProfileType::Dnstt => 30_000,
        ProfileType::MasterDns => 90_000,
        _ => 0,
    }
}

/// Upstream `ExportIdentity`. Resolver lists are not part of the key, so a
/// subscription update that only changes resolvers replaces the same profile.
pub fn dns_tunnel_identity(profile_type: ProfileType, raw: &str) -> Option<String> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let identity = match profile_type {
        ProfileType::Dnstt => dnstt_identity(&value)?,
        ProfileType::MasterDns => masterdns_identity(&value)?,
        _ => return None,
    };
    Some(identity.to_string())
}

fn dnstt_identity(value: &Value) -> Option<Value> {
    if value.get("type").and_then(Value::as_str) != Some("dnstt") {
        return None;
    }
    let mode = text(value, "mode").to_ascii_lowercase();
    let mode = if mode.is_empty() {
        "dnstt".into()
    } else {
        mode
    };
    let mut identity = json!({
        "type": "dnstt",
        "mode": mode,
        "domain": text(value, "domain").to_ascii_lowercase(),
        "public_key": text(value, "public_key").trim().to_ascii_lowercase(),
    });
    if mode == "vaydns" && value.get("dnstt_compat") == Some(&Value::Bool(true)) {
        identity["dnstt_compat"] = Value::Bool(true);
    }
    if let Some(backend) = dnstt_backend_identity(value) {
        identity["backend"] = backend;
    }
    Some(identity)
}

fn dnstt_backend_identity(value: &Value) -> Option<Value> {
    for kind in ["socks", "shadowsocks", "ssh"] {
        let Some(options) = value.get(kind).filter(|item| item.is_object()) else {
            continue;
        };
        let mut identity = json!({"type": kind});
        match kind {
            "socks" => {
                if let Some(username) = options.get("username").and_then(Value::as_str) {
                    identity["username"] = json!(username);
                }
            }
            "ssh" => {
                if let Some(user) = options.get("user").and_then(Value::as_str) {
                    identity["user"] = json!(user);
                }
            }
            "shadowsocks" => {
                if let Some(method) = options.get("method").and_then(Value::as_str) {
                    identity["method"] = json!(method);
                }
            }
            _ => {}
        }
        return Some(identity);
    }
    None
}

fn masterdns_identity(value: &Value) -> Option<Value> {
    if value.get("type").and_then(Value::as_str) != Some("masterdns") {
        return None;
    }
    let domains = value
        .get("domain")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|domain| domain.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mode = text(value, "mode");
    let method = text(value, "encryption_method");
    Some(json!({
        "type": "masterdns",
        "domain": domains,
        "mode": if mode.is_empty() { "socks5" } else { mode },
        "encryption_method": if method.is_empty() { "xor" } else { method },
        "encryption_key": text(value, "encryption_key"),
    }))
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowance_matches_upstream_startup_budget() {
        assert_eq!(dns_tunnel_startup_allowance_ms(ProfileType::Dnstt), 30_000);
        assert_eq!(
            dns_tunnel_startup_allowance_ms(ProfileType::MasterDns),
            90_000
        );
        assert_eq!(dns_tunnel_startup_allowance_ms(ProfileType::Vless), 0);
    }

    #[test]
    fn identity_ignores_resolver_list_changes() {
        let first = dns_tunnel_identity(
            ProfileType::Dnstt,
            r#"{"type":"dnstt","mode":"vaydns","domain":"Tunnel.Example","public_key":"AB","dnstt_compat":true,"socks":{"username":"u"},"resolvers":["1.1.1.1"]}"#,
        )
        .unwrap();
        let second = dns_tunnel_identity(
            ProfileType::Dnstt,
            r#"{"type":"dnstt","mode":"VayDNS","domain":"tunnel.example","public_key":"ab","dnstt_compat":true,"socks":{"username":"u","password":"secret"},"resolvers":["8.8.8.8"]}"#,
        )
        .unwrap();
        assert_eq!(first, second);
        let parsed: Value = serde_json::from_str(&first).unwrap();
        assert_eq!(parsed["public_key"], "ab");
        assert_eq!(parsed["backend"]["username"], "u");
        assert!(parsed.get("resolvers").is_none());
    }
}
