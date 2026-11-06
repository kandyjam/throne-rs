//! 1.4.0-beta.2 share-link TLS and Xray-only transport contracts.
use base64::Engine;
use serde_json::{json, Value};
use throne_domain::{ParsedOutbound, ProfileType};

use crate::ImportedProfile;

// Go crypto/tls CipherSuites accepted by the target native core.
const NATIVE_CIPHER_SUITES: &[&str] = &[
    "TLS_AES_128_GCM_SHA256",
    "TLS_AES_256_GCM_SHA384",
    "TLS_CHACHA20_POLY1305_SHA256",
    "TLS_ECDHE_ECDSA_WITH_AES_128_CBC_SHA",
    "TLS_ECDHE_ECDSA_WITH_AES_256_CBC_SHA",
    "TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA",
    "TLS_ECDHE_RSA_WITH_AES_256_CBC_SHA",
    "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
    "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384",
    "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
    "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
    "TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256",
    "TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256",
];

fn get<'a>(query: &'a [(String, String)], key: &str) -> Option<&'a str> {
    query
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

fn config_lines(text: &str) -> Vec<String> {
    let lines: Vec<_> = text
        .split([',', '\n'])
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
        .collect();
    if lines.iter().any(|s| s.contains("-----BEGIN")) {
        return lines;
    }
    let body = lines.concat().trim().to_string();
    if body.is_empty() {
        return Vec::new();
    }
    vec![
        "-----BEGIN ECH CONFIGS-----".into(),
        body,
        "-----END ECH CONFIGS-----".into(),
    ]
}

pub(crate) fn apply_link_tls(outbound: &mut Value, query: &[(String, String)]) {
    let Some(tls) = outbound.get_mut("tls").and_then(Value::as_object_mut) else {
        return;
    };
    if let Some(value) = get(query, "allowInsecure").or_else(|| get(query, "insecure")) {
        tls.insert("insecure".into(), json!(matches!(value, "1" | "true")));
    }
    if let Some(cs) = get(query, "cs").or_else(|| get(query, "tls_cipher_suites")) {
        let suites: Vec<_> = cs
            .split([':', ','])
            .map(str::trim)
            .filter(|s| NATIVE_CIPHER_SUITES.contains(s))
            .collect();
        tls.insert("cipher_suites".into(), json!(suites));
    }
    if get(query, "fp") == Some("unsafe") {
        if tls.get("reality").is_some_and(|r| r["enabled"] == true) {
            tls.insert(
                "utls".into(),
                json!({"enabled":true,"fingerprint":"random"}),
            );
        } else {
            tls.remove("utls");
        }
    }
    if let Some(pcs) = get(query, "pcs") {
        let hashes: Vec<_> = pcs
            .split(',')
            .filter_map(|hash| {
                let hex = hash.trim().replace(':', "");
                if hex.len() != 64 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return None;
                }
                let bytes: Vec<_> = (0..64)
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect();
                Some(base64::engine::general_purpose::STANDARD.encode(bytes))
            })
            .collect();
        if !hashes.is_empty() {
            tls.insert("certificate_sha256".into(), json!(hashes));
        }
    }
    let mut ech = json!({"enabled":false});
    if let Some(value) = get(query, "ech").map(str::trim) {
        let enabled = !matches!(value, "" | "0" | "false");
        ech["enabled"] = json!(enabled);
        if enabled && !matches!(value, "1" | "true") {
            if value.contains("://") || value.contains('.') {
                if let Some(scheme) = value.find("://") {
                    if let Some(plus) = value.find('+').filter(|i| *i > 0 && *i < scheme) {
                        ech["query_server_name"] = json!(value[..plus].trim());
                        ech["resolver"] = json!(value[plus + 1..].trim());
                    } else {
                        ech["resolver"] = json!(value);
                    }
                } else {
                    ech["query_server_name"] = json!(value);
                }
            } else {
                ech["config"] = json!(config_lines(value));
            }
        }
    }
    if let Some(value) = get(query, "ech_enabled") {
        ech["enabled"] = json!(value == "true");
    }
    if let Some(value) = get(query, "ech_config") {
        ech["config"] = json!(config_lines(value));
    }
    if let Some(value) = get(query, "ech_config_path") {
        ech["config_path"] = json!(value);
    }
    if let Some(value) = get(query, "ech_server_name") {
        ech["query_server_name"] = json!(value);
    }
    if ech["enabled"] == true {
        tls.insert("ech".into(), ech);
    }
}

/// Accept the legacy Python repr subset without rewriting quoted string contents.
fn python_repr_json(raw: &str) -> Option<String> {
    let mut chars = raw.chars().peekable();
    let mut out = String::with_capacity(raw.len());
    while let Some(c) = chars.next() {
        if matches!(c, '\'' | '"') {
            out.push('"');
            let mut closed = false;
            while let Some(next) = chars.next() {
                if next == c {
                    closed = true;
                    break;
                }
                if next == '\\' {
                    let escaped = chars.next()?;
                    if c == '\'' && escaped == '\'' {
                        out.push('\'');
                    } else {
                        out.push('\\');
                        out.push(escaped);
                    }
                } else {
                    if c == '\'' && next == '"' {
                        out.push('\\');
                    }
                    out.push(next);
                }
            }
            if !closed {
                return None;
            }
            out.push('"');
        } else if c.is_ascii_alphabetic() || c == '_' {
            let mut token = String::from(c);
            while chars
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
            {
                token.push(chars.next()?);
            }
            out.push_str(match token.as_str() {
                "True" => "true",
                "False" => "false",
                "None" => "null",
                _ => &token,
            });
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// Strict JSON wins; only legacy syntax outside strings is normalized.
fn extra_json(raw: &str) -> Option<Value> {
    let parse = |s: &str| {
        serde_json::from_str::<Value>(s)
            .ok()
            .filter(Value::is_object)
    };
    parse(raw).or_else(|| parse(&python_repr_json(raw)?))
}

fn query_extra(source: &str, key: &str, decoded: &str) -> Option<Value> {
    extra_json(decoded).or_else(|| {
        // Decode forms from the original URL: replacing '+' after percent decoding
        // would also destroy encoded literal %2B characters inside values.
        let url = url::Url::parse(source).ok()?;
        let (_, value) = url
            .query_pairs()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))?;
        extra_json(&value)
    })
}

pub(crate) fn xhttp_profile(
    name: &str,
    o: &ParsedOutbound,
    query: &[(String, String)],
    source: &str,
) -> Option<ImportedProfile> {
    let mut xhttp = json!({
        "path": o.path.as_deref().filter(|path| !path.is_empty()).unwrap_or("/"),
        "mode": get(query, "mode").filter(|mode| !mode.is_empty()).unwrap_or("auto"),
    });
    if let Some(host) = o.host.as_deref().filter(|host| !host.is_empty()) {
        xhttp["host"] = json!(host);
    }
    if let Some(extra) = get(query, "extra") {
        xhttp["extra"] = query_extra(source, "extra", extra)?;
    }
    let security = o.security.as_deref().unwrap_or("none").to_ascii_lowercase();
    let mut stream = json!({"network":"xhttp","security":security,"xhttpSettings":xhttp});
    if security == "tls" {
        // xrayTLS::ExportToJson always writes serverName, even when it is empty.
        let mut tls = json!({"serverName": o.sni.as_deref().unwrap_or("")});
        for (query_key, key) in [
            ("fp", "fingerprint"),
            ("ech", "echConfigList"),
            ("pcs", "pinnedPeerCertSha256"),
            ("vcn", "verifyPeerCertByName"),
        ] {
            if let Some(value) = get(query, query_key).filter(|value| !value.is_empty()) {
                tls[key] = json!(value);
            }
        }
        if let Some(alpn) = o.alpn.as_deref().filter(|alpn| !alpn.is_empty()) {
            tls["alpn"] = json!(alpn
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>());
        }
        if let Some(cs) = get(query, "cs").or_else(|| get(query, "tls_cipher_suites")) {
            let suites = cs
                .split([':', ','])
                .map(str::trim)
                .filter(|suite| !suite.is_empty())
                .collect::<Vec<_>>()
                .join(":");
            if !suites.is_empty() {
                tls["cipherSuites"] = json!(suites);
            }
        }
        if get(query, "allowInsecure")
            .or_else(|| get(query, "insecure"))
            .is_some_and(|value| matches!(value, "1" | "true"))
        {
            // Not in ExportToJson. Qt ignores the extra key; our reload still honors it.
            tls["allowInsecure"] = json!(true);
        }
        stream["tlsSettings"] = tls;
    } else if security == "reality" {
        let mut reality = json!({"serverName": o.sni.as_deref().unwrap_or("")});
        if let Some(fp) =
            o.fp.as_deref()
                .filter(|fp| !fp.is_empty() && !fp.eq_ignore_ascii_case("unsafe"))
        {
            reality["fingerprint"] = json!(fp);
        }
        if let Some(pbk) = o.pbk.as_deref().filter(|pbk| !pbk.is_empty()) {
            reality["password"] = json!(pbk);
        }
        if let Some(sid) = o.sid.as_deref().filter(|sid| !sid.is_empty()) {
            reality["shortId"] = json!(sid);
        }
        if let Some(spx) = o.spx.as_deref().filter(|spx| !spx.is_empty()) {
            reality["spiderX"] = json!(spx);
        }
        stream["realitySettings"] = reality;
    }
    if let Some(key) = ["fm", "finalmask"]
        .into_iter()
        .find(|key| get(query, key).is_some())
    {
        stream["finalmask"] = query_extra(source, key, get(query, key)?)?;
    }
    let encryption = get(query, "encryption")
        .filter(|value| !value.is_empty())
        .unwrap_or("none");
    let mut settings = json!({
        "address": o.server.as_deref().unwrap_or(""),
        "port": o.server_port.unwrap_or(443),
        "id": o.uuid.as_deref().unwrap_or(""),
        "encryption": encryption,
    });
    if let Some(flow) = o
        .flow
        .as_deref()
        .filter(|flow| !flow.is_empty() && !flow.eq_ignore_ascii_case("none"))
    {
        settings["flow"] = json!(flow);
    }
    // Upstream xrayVless::ExportToJson. Original Throne ParseFromJson requires
    // protocol=vless and settings.address/id, not a custom xrayfullconfig wrapper.
    let mut bean = json!({
        "protocol": "vless",
        "settings": settings,
        "streamSettings": stream,
    });
    if !name.is_empty() {
        bean["tag"] = json!(name);
    }
    let mut outbound = o.clone();
    if !name.is_empty() {
        outbound.tag = Some(name.to_string());
    }
    outbound.transport = Some("xhttp".into());
    outbound.security = Some(security.clone());
    outbound.tls = Some(matches!(security.as_str(), "tls" | "reality"));
    outbound.raw_json = Some(bean.to_string());
    Some(ImportedProfile {
        name: name.to_string(),
        profile_type: ProfileType::XrayVless,
        outbound,
        source: source.to_string(),
    })
}

pub(crate) fn import_notes(profile: &ImportedProfile) -> Vec<String> {
    if matches!(
        profile.profile_type,
        ProfileType::Custom | ProfileType::XrayVless
    ) || profile
        .outbound
        .raw_json
        .as_deref()
        .is_some_and(|raw| raw.contains("\"subtype\":\"xrayfullconfig\""))
    {
        return Vec::new();
    }
    let Ok(url) = url::Url::parse(&profile.source) else {
        return Vec::new();
    };
    let query: Vec<_> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    get(&query, "cs")
        .or_else(|| get(&query, "tls_cipher_suites"))
        .into_iter()
        .flat_map(|value| value.split([':', ',']))
        .map(str::trim)
        .filter(|suite| !suite.is_empty() && !NATIVE_CIPHER_SUITES.contains(suite))
        .map(|suite| {
            format!(
                "{}: ignored unsupported native TLS cipher suite {suite}",
                profile.name
            )
        })
        .collect()
}
