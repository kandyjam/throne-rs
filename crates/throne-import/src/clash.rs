//! Clash YAML `proxies:` list importer (subset of upstream `updateClash`).

use serde_json::json;
use throne_domain::{ParsedOutbound, ProfileType};

use crate::ImportedProfile;

pub fn try_import_clash(text: &str) -> Option<Vec<ImportedProfile>> {
    if !text.contains("proxies:") {
        return None;
    }
    let doc: serde_yaml::Value = serde_yaml::from_str(text).ok()?;
    let proxies = doc.get("proxies")?.as_sequence()?;
    let mut out = Vec::new();
    for p in proxies {
        if let Some(prof) = proxy_from_yaml(p) {
            out.push(prof);
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn proxy_from_yaml(v: &serde_yaml::Value) -> Option<ImportedProfile> {
    let map = v.as_mapping()?;
    let get =
        |k: &str| -> Option<&serde_yaml::Value> { map.get(serde_yaml::Value::String(k.into())) };
    let ty = get("type")?.as_str()?.to_ascii_lowercase();
    let name = get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&ty)
        .to_string();
    let server = get("server")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let port = get("port")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_i64().map(|n| n as u64))
                .or_else(|| v.as_str()?.parse().ok())
        })
        .unwrap_or(0) as u16;
    if server.is_empty() || port == 0 {
        return None;
    }

    let profile_type = match ty.as_str() {
        "ss" | "shadowsocks" => ProfileType::Shadowsocks,
        "vmess" => ProfileType::Vmess,
        "vless" => ProfileType::Vless,
        "trojan" => ProfileType::Trojan,
        "http" | "https" => ProfileType::Http,
        "socks" | "socks5" => ProfileType::Socks,
        "hysteria" => ProfileType::Hysteria,
        "hysteria2" | "hy2" => ProfileType::Hysteria2,
        "tuic" => ProfileType::Tuic,
        "wireguard" | "wg" => ProfileType::Wireguard,
        "ssh" => ProfileType::Ssh,
        "mieru" => ProfileType::Mieru,
        "anytls" => ProfileType::AnyTls,
        _ => ProfileType::Custom,
    };

    let mut outbound = ParsedOutbound {
        tag: Some(name.clone()),
        server: Some(server),
        server_port: Some(port),
        uuid: get("uuid").and_then(|v| v.as_str()).map(|s| s.to_string()),
        password: get("password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        username: get("username")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        method: get("cipher")
            .or_else(|| get("method"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        flow: get("flow").and_then(|v| v.as_str()).map(|s| s.to_string()),
        security: get("cipher")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        alter_id: get("alterId")
            .or_else(|| get("alter-id"))
            .and_then(|v| v.as_i64().map(|n| n as i32)),
        sni: get("sni")
            .or_else(|| get("servername"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        fp: get("client-fingerprint")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        path: get("ws-path")
            .or_else(|| get("path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        host: get("ws-host")
            .or_else(|| get("host"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        ..Default::default()
    };

    let network = get("network")
        .or_else(|| get("type")) // careful not to overwrite
        .and_then(|v| v.as_str())
        .filter(|s| *s != ty)
        .map(|s| s.to_string());
    if let Some(n) = get("network").and_then(|v| v.as_str()) {
        outbound.transport = Some(n.to_string());
    } else if let Some(n) = network {
        if n != ty {
            outbound.transport = Some(n);
        }
    }

    let tls = get("tls").and_then(|v| v.as_bool()).unwrap_or(false)
        || get("skip-cert-verify").is_some()
        || matches!(ty.as_str(), "trojan" | "hysteria2" | "tuic" | "anytls");
    outbound.tls = Some(tls);
    outbound.insecure = get("skip-cert-verify").and_then(|v| v.as_bool());

    if let Some(opts) = get("reality-opts").and_then(|v| v.as_mapping()) {
        let g = |k: &str| {
            opts.get(serde_yaml::Value::String(k.into()))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        };
        outbound.pbk = g("public-key");
        outbound.sid = g("short-id");
        outbound.security = Some("reality".into());
        outbound.tls = Some(true);
    }

    // Keep a JSON snapshot of the clash node for later full conversion.
    // Always include `"type"` so Qt Throne ParseFromJson can load Address/Name.
    if let Ok(json_v) = serde_yaml::from_value::<serde_json::Value>(v.clone()) {
        outbound.raw_json = Some(
            json!({
                "clash": json_v,
                "tag": name,
                "type": profile_type.as_str(),
            })
            .to_string(),
        );
    } else {
        outbound.raw_json = Some(outbound.to_db_json(profile_type));
    }

    Some(ImportedProfile {
        name,
        profile_type,
        outbound,
        source: format!("clash:{ty}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_clash() {
        let yaml = r#"
proxies:
  - name: "ss1"
    type: ss
    server: 1.2.3.4
    port: 8388
    cipher: aes-256-gcm
    password: "secret"
  - name: "vl1"
    type: vless
    server: a.example
    port: 443
    uuid: 11111111-1111-1111-1111-111111111111
    tls: true
    network: ws
    servername: a.example
"#;
        let list = try_import_clash(yaml).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].profile_type, ProfileType::Shadowsocks);
        assert_eq!(list[1].profile_type, ProfileType::Vless);
        assert_eq!(list[1].outbound.sni.as_deref(), Some("a.example"));
    }
}
