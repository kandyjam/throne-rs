//! DNSTT, SlipNet, and MasterDNS share links from upstream 1.4.0-beta.6.
//!
//! The stored JSON is the bean `ExportToJson` shape: resolver lines stay
//! strings until config build turns them into the sing-box objects or
//! MasterDNS strings. Encrypted SlipNet and MasterDNS TOML links fail with
//! the upstream message instead of disappearing.

use base64::engine::general_purpose::URL_SAFE;
use base64::Engine;
use serde_json::{json, Map, Value};

use throne_domain::{ParsedOutbound, ProfileType};

use crate::decode::percent_decode;
use crate::ImportedProfile;

const MAX_LINK_SIZE: usize = 64 * 1024;

const DNST_PARAMS: &[&str] = &[
    "pubkey",
    "cert",
    "record-type",
    "dnstt-compat",
    "clientid-size",
    "idle-timeout",
    "keepalive",
    "mkey",
    "encryption",
    "user",
    "password",
    "pk",
    "method",
];

const TRANSPORT_PARAMS: &[&str] = &[
    "pubkey",
    "cert",
    "record-type",
    "dnstt-compat",
    "clientid-size",
    "idle-timeout",
    "keepalive",
    "mkey",
    "encryption",
];

const ENCRYPTION_METHODS: &[&str] = &[
    "none",
    "xor",
    "chacha20",
    "aes-128-gcm",
    "aes-192-gcm",
    "aes-256-gcm",
];

const BALANCING: &[&str] = &[
    "round_robin",
    "random",
    "round_robin",
    "least_loss",
    "lowest_latency",
    "hybrid",
    "loss_then_latency",
    "least_loss_top_random",
    "least_loss_top_round_robin",
];

#[derive(Debug)]
pub struct ParsedDnsTunnel {
    pub profile: ImportedProfile,
    pub notes: Vec<String>,
}

pub fn parse_dns_tunnel_link(link: &str) -> Result<ParsedDnsTunnel, String> {
    let link = link.trim();
    if link.len() > MAX_LINK_SIZE {
        return Err("The link is too long.".into());
    }
    let lower = link.to_ascii_lowercase();
    if lower.starts_with("slipnet://") {
        return parse_slipnet(link);
    }
    if lower.starts_with("slipnet-") {
        return Err(
            "Encrypted SlipNet links cannot be read: export the profile from SlipNet without encryption."
                .into(),
        );
    }
    if lower.starts_with("dnst://") {
        return parse_dnst(link);
    }
    if lower.starts_with("masterdns://") || lower.starts_with("stormdns://") {
        let rest = link.split_once("://").map(|(_, rest)| rest).unwrap_or("");
        return parse_masterdns_rest(rest);
    }
    Err("Not a DNS tunnel link.".into())
}

struct DnstLink {
    tag: String,
    domain: String,
    transport: String,
    backend: String,
    params: Vec<(String, String)>,
    resolvers: Vec<String>,
}

fn parse_dnst(link: &str) -> Result<ParsedDnsTunnel, String> {
    let parsed = decode_dnst(link)?;
    if parsed.transport == "masterdns" {
        return masterdns_from_dnst(parsed);
    }
    if parsed.transport != "dnstt" && parsed.transport != "vaydns" {
        return Err("This DNS tunnel type is not supported.".into());
    }
    if parsed.backend.is_empty() {
        return Err("The link does not name the service behind the tunnel.".into());
    }
    if parsed.domain.is_empty() {
        return Err("The link does not name a domain.".into());
    }
    let mut notes = Vec::new();
    let mut object = Map::new();
    object.insert("type".into(), json!("dnstt"));
    object.insert("mode".into(), json!(parsed.transport));
    object.insert("domain".into(), json!(normalize_domain(&parsed.domain)));
    let pubkey = param(&parsed.params, "pubkey").trim().to_ascii_lowercase();
    if !pubkey.is_empty() {
        object.insert("public_key".into(), json!(pubkey));
    }
    if parsed.transport == "vaydns" {
        read_vay_knobs(&mut object, &parsed.params);
    }
    match parsed.backend.as_str() {
        "socks" => {
            object.insert(
                "socks".into(),
                json!({
                    "username": param(&parsed.params, "user"),
                    "password": param(&parsed.params, "password"),
                }),
            );
        }
        "ssh" => {
            let mut ssh = Map::new();
            ssh.insert("user".into(), json!(param(&parsed.params, "user")));
            ssh.insert("password".into(), json!(param(&parsed.params, "password")));
            if let Some(pem) = private_key_pem(param(&parsed.params, "pk")) {
                ssh.insert("private_key".into(), json!(pem));
            } else if !param(&parsed.params, "pk").trim().is_empty() {
                notes.push(
                    "The SSH private key in the link could not be read; it was ignored.".into(),
                );
            }
            object.insert("ssh".into(), Value::Object(ssh));
        }
        "shadowsocks" => {
            object.insert(
                "shadowsocks".into(),
                json!({
                    "method": param(&parsed.params, "method").trim().to_ascii_lowercase(),
                    "password": param(&parsed.params, "password"),
                }),
            );
        }
        "custom" => {}
        _ => return Err("This DNS tunnel backend is not supported.".into()),
    }
    if !parsed.resolvers.is_empty() {
        object.insert("resolvers".into(), json!(parsed.resolvers));
    }
    Ok(finish(ProfileType::Dnstt, parsed.tag, object, notes, link))
}

fn masterdns_from_dnst(parsed: DnstLink) -> Result<ParsedDnsTunnel, String> {
    if parsed.domain.is_empty() {
        return Err("The link does not name a domain.".into());
    }
    // A SOCKS user name and password guard only a local listener, which this profile does not open.
    let mode = match parsed.backend.as_str() {
        "socks" => "socks5",
        "custom" => "tcp",
        "" => {
            return Err("The link does not name the service behind the tunnel.".into());
        }
        _ => {
            return Err("MasterDNS only carries a SOCKS or a custom service.".into());
        }
    };
    let method = encryption_method(param(&parsed.params, "encryption"))?;
    let mut object = masterdns_object(
        &[normalize_domain(&parsed.domain)],
        mode,
        &method,
        param(&parsed.params, "mkey"),
    );
    if !parsed.resolvers.is_empty() {
        object.insert("resolvers".into(), json!(parsed.resolvers));
    }
    Ok(finish(
        ProfileType::MasterDns,
        parsed.tag,
        object,
        Vec::new(),
        "",
    ))
}

fn parse_masterdns_rest(rest: &str) -> Result<ParsedDnsTunnel, String> {
    let rest = rest.trim();
    let lower = rest.to_ascii_lowercase();
    if lower.starts_with("v1/") {
        return Err("This MasterDNS link format is not supported.".into());
    }
    if lower == "v1" || lower.starts_with("v1?") || lower.starts_with("v1#") {
        return parse_yptun(&rest[2..]);
    }
    parse_profile(rest)
}

fn parse_yptun(rest: &str) -> Result<ParsedDnsTunnel, String> {
    let (rest, name) = match rest.split_once('#') {
        Some((body, fragment)) => (body, percent_decode(fragment).trim().to_string()),
        None => (rest, String::new()),
    };
    let rest = rest.strip_prefix('?').unwrap_or(rest);
    let mut query: Vec<(String, String)> = Vec::new();
    for pair in rest.split('&').filter(|part| !part.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = percent_decode(key).trim().to_ascii_lowercase();
        if query.iter().any(|(existing, _)| existing == &key) {
            continue;
        }
        query.push((key, percent_decode(value)));
    }
    let mut notes = Vec::new();
    let domains = split_list(&q(&query, "domains"))
        .into_iter()
        .map(|item| normalize_domain(&item))
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    let method = encryption_method(&q(&query, "enc"))?;
    let mut object = masterdns_object(&domains, "socks5", &method, &q(&query, "key"));
    if query.iter().any(|(key, _)| key == "resolvers") {
        object.insert(
            "resolvers".into(),
            json!(split_list(&q(&query, "resolvers"))),
        );
    }
    if let Some(strategy) = balancing_strategy(&q(&query, "bal")) {
        object.insert("balancing_strategy".into(), json!(strategy));
    }
    if let Some(duplication) = parse_count(&q(&query, "dup"), 0, 10) {
        if duplication > 0 {
            object.insert("packet_duplication".into(), json!(duplication));
        }
    }
    if !q(&query, "proxy").trim().is_empty() {
        notes.push(
            "The link also carries a proxy to run over the tunnel; import that proxy separately and chain it over this profile."
                .into(),
        );
    }
    Ok(finish(ProfileType::MasterDns, name, object, notes, ""))
}

fn parse_profile(payload: &str) -> Result<ParsedDnsTunnel, String> {
    let payload = payload
        .split_once('#')
        .map(|(body, _)| body)
        .unwrap_or(payload);
    let payload = payload
        .split_once('?')
        .map(|(body, _)| body)
        .unwrap_or(payload);
    let bytes = dns_tunnel_base64(payload.trim())?;
    let root: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "The link is damaged: its payload is not a JSON object.".to_string())?;
    let Some(root) = root.as_object() else {
        return Err("The link is damaged: its payload is not a JSON object.".into());
    };
    let schema = root
        .get("schema")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !matches!(
        schema.as_str(),
        "whitedns.profile" | "masterdns.profile" | "stormdns.profile"
    ) {
        return Err("This MasterDNS link format is not supported.".into());
    }
    let version = root.get("version").map(value_text).unwrap_or_default();
    if version.trim() != "1" {
        return Err("This MasterDNS link version is not supported.".into());
    }
    let profile = root.get("profile").and_then(Value::as_object);
    let server = profile
        .and_then(|item| item.get("server"))
        .and_then(Value::as_object);
    let name = profile
        .and_then(|item| item.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let mut domains = Vec::new();
    if let Some(server) = server {
        if let Some(domain) = server.get("domain").and_then(Value::as_str) {
            domains.push(normalize_domain(domain));
        }
        if let Some(list) = server.get("domains").and_then(Value::as_array) {
            domains.extend(list.iter().filter_map(Value::as_str).map(normalize_domain));
        }
    }
    domains.retain(|domain| !domain.is_empty());
    domains.dedup();
    let method_value = server.and_then(|item| {
        item.get("encryption_method")
            .or_else(|| item.get("encryptionMethod"))
    });
    let method = match method_value {
        Some(Value::Number(number)) => encryption_method(&number.to_string())?,
        Some(value) => encryption_method(&value_text(value))?,
        None => encryption_method("")?,
    };
    let key = server
        .and_then(|item| {
            item.get("encryption_key")
                .or_else(|| item.get("encryptionKey"))
        })
        .map(value_text)
        .unwrap_or_default();
    let object = masterdns_object(&domains, "socks5", &method, &key);
    Ok(finish(ProfileType::MasterDns, name, object, Vec::new(), ""))
}

fn parse_slipnet(link: &str) -> Result<ParsedDnsTunnel, String> {
    let mut payload = link.get(10..).unwrap_or("");
    if let Some((body, _)) = payload.split_once('#') {
        payload = body;
    }
    let bytes = dns_tunnel_base64(payload.trim())?;
    let text = String::from_utf8(bytes)
        .map_err(|_| "The link is damaged: its payload is not valid base64.".to_string())?;
    let fields: Vec<&str> = text.split('|').collect();
    let field = |index: usize| fields.get(index).copied().unwrap_or("").trim();
    let version: i32 = field(0)
        .parse()
        .map_err(|_| "This SlipNet link version is not supported.".to_string())?;
    if version < 16 {
        return Err("This SlipNet link version is not supported.".into());
    }
    let needed = if version >= 28 {
        79
    } else if version >= 22 {
        60
    } else if version >= 20 {
        50
    } else {
        36
    };
    if fields.len() < needed {
        return Err("The link is damaged: it is shorter than its version allows.".into());
    }
    let mut notes = Vec::new();
    if version > 28 {
        notes.push(
            "This link comes from a newer SlipNet version; check the imported settings.".into(),
        );
    }
    let mut kind = field(1).to_ascii_lowercase();
    let ssh_type = kind.ends_with("_ssh");
    if ssh_type {
        kind.truncate(kind.len().saturating_sub(4));
    }
    let mode = if kind == "dnstt" || kind == "vaydns" {
        kind
    } else if kind == "sayedns" {
        notes.push(
            "NoizDNS was imported as plain DNSTT: its server accepts that, but without NoizDNS's DPI evasion."
                .into(),
        );
        "dnstt".into()
    } else {
        return Err("This SlipNet profile type is not supported.".into());
    };
    let mut object = Map::new();
    object.insert("type".into(), json!("dnstt"));
    object.insert("mode".into(), json!(mode));
    object.insert("domain".into(), json!(normalize_domain(field(3))));
    let key = field(11).trim().to_ascii_lowercase();
    if !key.is_empty() {
        object.insert("public_key".into(), json!(key));
    }
    if let Some(resolver) = slipnet_resolver(field(4), field(22)) {
        object.insert("resolvers".into(), json!([resolver]));
    }
    if ssh_type || field(14) == "1" {
        let mut ssh = Map::new();
        ssh.insert("user".into(), json!(field(15)));
        ssh.insert(
            "password".into(),
            json!(fields.get(16).copied().unwrap_or("")),
        );
        if let Some(pem) = private_key_pem(field(24)) {
            ssh.insert("private_key".into(), json!(pem));
        } else if !field(24).is_empty() {
            notes.push("The SSH private key in the link could not be read; it was ignored.".into());
        }
        object.insert("ssh".into(), Value::Object(ssh));
    } else {
        object.insert(
            "socks".into(),
            json!({
                "username": field(12),
                "password": fields.get(13).copied().unwrap_or(""),
            }),
        );
    }
    if mode == "vaydns" {
        let record_type = field(42).to_ascii_lowercase();
        if !record_type.is_empty() {
            object.insert("record_type".into(), json!(record_type));
        }
        if let Some(max_qname) = parse_count(field(43), 1, 253) {
            object.insert("max_qname_len".into(), json!(max_qname));
        }
        if let Ok(rps) = field(44).parse::<f64>() {
            if field(44) != "0" && rps.is_finite() && rps >= 0.0 {
                object.insert("rps".into(), json!(rps));
            }
        }
        if let Some(client_id) = parse_count(field(49), 1, 253) {
            object.insert("client_id_size".into(), json!(client_id));
        }
    }
    Ok(finish(
        ProfileType::Dnstt,
        field(2).to_string(),
        object,
        notes,
        link,
    ))
}

fn decode_dnst(link: &str) -> Result<DnstLink, String> {
    let Some(rest) = link.get(7..) else {
        return Err("Not a dnst:// link.".into());
    };
    let mut rest = rest.trim().to_string();
    let mut fragment = String::new();
    if let Some(hash) = rest.find('#') {
        fragment = percent_decode(&rest[hash + 1..]).trim().to_string();
        rest.truncate(hash);
    }
    let mut query = String::new();
    if let Some(mark) = rest.find('?') {
        query = rest[mark + 1..].to_string();
        rest.truncate(mark);
    }
    if rest.contains('/') {
        let parts: Vec<&str> = rest.split('/').collect();
        let mut params = Vec::new();
        for pair in query.split('&').filter(|part| !part.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let key = form_decode(key).trim().to_ascii_lowercase();
            if DNST_PARAMS.contains(&key.as_str()) {
                params.push((key, form_decode(value)));
            }
        }
        return Ok(DnstLink {
            tag: fragment,
            domain: percent_decode(parts.first().copied().unwrap_or(""))
                .trim()
                .to_string(),
            transport: percent_decode(parts.get(1).copied().unwrap_or(""))
                .trim()
                .to_ascii_lowercase(),
            backend: percent_decode(parts.get(2).copied().unwrap_or(""))
                .trim()
                .to_ascii_lowercase(),
            params,
            resolvers: Vec::new(),
        });
    }
    let bytes = dns_tunnel_base64(rest.trim())?;
    let root: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "The link is damaged: its payload is not a JSON object.".to_string())?;
    let Some(root) = root.as_object() else {
        return Err("The link is damaged: its payload is not a JSON object.".into());
    };
    let transport = root
        .get("transport")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let backend = root
        .get("backend")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut tag = root
        .get("tag")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if tag.is_empty() {
        tag = fragment;
    }
    let mut params = Vec::new();
    for name in DNST_PARAMS {
        let key = name.replace('-', "_");
        let layer = if TRANSPORT_PARAMS.contains(name) {
            &transport
        } else {
            &backend
        };
        if let Some(value) = layer.get(&key) {
            params.push(((*name).to_string(), value_text(value)));
        }
    }
    let resolvers = root
        .get("resolvers")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let kind = transport
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if kind.is_empty() {
        return Err("The link does not name the tunnel type.".into());
    }
    Ok(DnstLink {
        tag,
        domain: transport
            .get("domain")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        transport: kind,
        backend: backend
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase(),
        params,
        resolvers,
    })
}

fn read_vay_knobs(object: &mut Map<String, Value>, params: &[(String, String)]) {
    let compat = param(params, "dnstt-compat").trim().to_ascii_lowercase();
    if matches!(compat.as_str(), "true" | "1" | "yes") {
        object.insert("dnstt_compat".into(), json!(true));
    }
    let record_type = param(params, "record-type").trim().to_ascii_lowercase();
    if !record_type.is_empty() {
        object.insert("record_type".into(), json!(record_type));
    }
    if let Some(size) = parse_count(param(params, "clientid-size").trim(), 0, 253) {
        if size > 0 {
            object.insert("client_id_size".into(), json!(size));
        }
    }
    let idle = param(params, "idle-timeout").trim();
    if !idle.is_empty() {
        object.insert("idle_timeout".into(), json!(idle));
    }
    let keepalive = param(params, "keepalive").trim();
    if !keepalive.is_empty() {
        object.insert("keepalive_interval".into(), json!(keepalive));
    }
}

fn masterdns_object(domains: &[String], mode: &str, method: &str, key: &str) -> Map<String, Value> {
    let mut object = Map::new();
    object.insert("type".into(), json!("masterdns"));
    object.insert("domain".into(), json!(domains));
    object.insert(
        "mode".into(),
        json!(if mode.is_empty() { "socks5" } else { mode }),
    );
    object.insert(
        "encryption_method".into(),
        json!(if method.is_empty() { "xor" } else { method }),
    );
    if !key.is_empty() {
        object.insert("encryption_key".into(), json!(key));
    }
    object
}

fn finish(
    profile_type: ProfileType,
    name: String,
    object: Map<String, Value>,
    notes: Vec<String>,
    source: &str,
) -> ParsedDnsTunnel {
    let name = if name.trim().is_empty() {
        profile_type.display_name().to_string()
    } else {
        name.trim().to_string()
    };
    let raw = Value::Object(object).to_string();
    let mut outbound = ParsedOutbound {
        tag: Some(name.clone()),
        raw_json: Some(raw),
        ..Default::default()
    };
    outbound.server = None;
    outbound.server_port = None;
    ParsedDnsTunnel {
        profile: ImportedProfile {
            name,
            profile_type,
            outbound,
            source: if source.is_empty() {
                profile_type.as_str().to_string()
            } else {
                source.to_string()
            },
        },
        notes,
    }
}

fn encryption_method(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok("xor".into());
    }
    if let Some(name) = ENCRYPTION_METHODS
        .iter()
        .find(|name| name.eq_ignore_ascii_case(text))
    {
        return Ok((*name).to_string());
    }
    let id: i64 = text
        .parse()
        .map_err(|_| "Unknown encryption method: use a number from 0 to 5.".to_string())?;
    ENCRYPTION_METHODS
        .get(usize::try_from(id).unwrap_or(usize::MAX))
        .map(|name| (*name).to_string())
        .ok_or_else(|| "Unknown encryption method: use a number from 0 to 5.".into())
}

fn balancing_strategy(text: &str) -> Option<&str> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let id: usize = text.parse().ok()?;
    if id == 0 {
        return None;
    }
    BALANCING.get(id).copied()
}

fn parse_count(text: &str, min: i64, max: i64) -> Option<i64> {
    let text = text.trim();
    if text.is_empty() || text == "0" && min > 0 {
        return None;
    }
    let value: i64 = text.parse().ok()?;
    (min..=max).contains(&value).then_some(value)
}

fn split_list(text: &str) -> Vec<String> {
    text.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn slipnet_resolver(address: &str, transport: &str) -> Option<String> {
    let address = address.trim();
    if address.is_empty() {
        return None;
    }
    let kind = transport.trim().to_ascii_lowercase();
    Some(match kind.as_str() {
        "" | "udp" => address.to_string(),
        "tcp" => format!("tcp://{address}"),
        "dot" | "tls" => format!("tls://{address}"),
        _ => address.to_string(),
    })
}

fn param<'a>(params: &'a [(String, String)], key: &str) -> &'a str {
    params
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
        .unwrap_or("")
}

fn q(query: &[(String, String)], key: &str) -> String {
    query
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.clone())
        .unwrap_or_default()
}

fn form_decode(text: &str) -> String {
    percent_decode(&text.replace('+', " "))
}

fn normalize_domain(domain: &str) -> String {
    domain.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

fn dns_tunnel_base64(text: &str) -> Result<Vec<u8>, String> {
    let mut data = String::new();
    for ch in text.chars() {
        if matches!(ch, '\r' | '\n' | '\t') {
            continue;
        }
        if !ch.is_ascii() {
            return Err("The link is damaged: its payload is not valid base64.".into());
        }
        match ch {
            ' ' | '+' => data.push('-'),
            '/' => data.push('_'),
            _ => data.push(ch),
        }
    }
    while data.ends_with('=') {
        data.pop();
    }
    if data.is_empty() {
        return Err("The link is damaged: its payload is not valid base64.".into());
    }
    while data.len() % 4 != 0 {
        data.push('=');
    }
    URL_SAFE
        .decode(data)
        .map_err(|_| "The link is damaged: its payload is not valid base64.".to_string())
}

fn private_key_pem(param: &str) -> Option<String> {
    let param = param.trim();
    if param.is_empty() {
        return None;
    }
    if param.contains("-----BEGIN ") {
        return Some(param.to_string());
    }
    let der = dns_tunnel_base64(param).ok()?;
    let kind = pem_type(&der)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&der);
    let mut lines = vec![format!("-----BEGIN {kind}-----")];
    for chunk in encoded.as_bytes().chunks(64) {
        lines.push(String::from_utf8_lossy(chunk).into_owned());
    }
    lines.push(format!("-----END {kind}-----"));
    Some(lines.join("\n"))
}

fn pem_type(der: &[u8]) -> Option<&'static str> {
    if der.starts_with(b"openssh-key-v1\0") {
        return Some("OPENSSH PRIVATE KEY");
    }
    let (tag, start, end) = der_element(der, 0)?;
    if tag != 0x30 || end != der.len() {
        return None;
    }
    let mut tags = Vec::new();
    let mut first_integer = Vec::new();
    let mut pos = start;
    while pos < end && tags.len() <= 16 {
        let (child_tag, child_start, child_end) = der_element(der, pos)?;
        if child_end > end {
            return None;
        }
        if tags.is_empty() && child_tag == 0x02 {
            first_integer = der[child_start..child_end].to_vec();
        }
        tags.push(child_tag);
        pos = child_end;
    }
    let leading_integers = tags.iter().take_while(|tag| **tag == 0x02).count();
    if tags.len() >= 3 && tags[0] == 0x02 && tags[1] == 0x30 && tags[2] == 0x04 {
        return Some("PRIVATE KEY");
    }
    if tags.len() == 2 && tags[0] == 0x30 && tags[1] == 0x04 {
        return Some("ENCRYPTED PRIVATE KEY");
    }
    if tags.len() >= 2 && tags[0] == 0x02 && tags[1] == 0x04 && first_integer == [0x01] {
        return Some("EC PRIVATE KEY");
    }
    if tags.len() == 6 && leading_integers == 6 {
        return Some("DSA PRIVATE KEY");
    }
    if leading_integers >= 9 {
        return Some("RSA PRIVATE KEY");
    }
    None
}

fn der_element(der: &[u8], pos: usize) -> Option<(u8, usize, usize)> {
    if pos + 2 > der.len() {
        return None;
    }
    let tag = der[pos];
    let mut length = u64::from(der[pos + 1]);
    let mut start = pos + 2;
    if length & 0x80 != 0 {
        let bytes = (length & 0x7f) as usize;
        if bytes == 0 || bytes > 4 || start + bytes > der.len() {
            return None;
        }
        length = 0;
        for byte in &der[start..start + bytes] {
            length = (length << 8) | u64::from(*byte);
        }
        start += bytes;
    }
    let end = start.checked_add(usize::try_from(length).ok()?)?;
    if end > der.len() {
        return None;
    }
    Some((tag, start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    fn pubkey() -> String {
        "ab".repeat(32)
    }

    #[test]
    fn dnst_form_socks_link_keeps_export_shape() {
        let link = format!(
            "dnst://Tunnel.Example/dnstt/socks?pubkey={}&user=alice&password=secret#Home",
            pubkey()
        );
        let parsed = parse_dns_tunnel_link(&link).unwrap();
        assert_eq!(parsed.profile.profile_type, ProfileType::Dnstt);
        assert_eq!(parsed.profile.name, "Home");
        assert!(parsed.profile.outbound.server.is_none());
        let value: Value =
            serde_json::from_str(parsed.profile.outbound.raw_json.as_deref().unwrap()).unwrap();
        assert_eq!(value["type"], "dnstt");
        assert_eq!(value["domain"], "tunnel.example");
        assert_eq!(value["public_key"], pubkey());
        assert_eq!(value["socks"]["username"], "alice");
        assert!(value.get("server").is_none());
    }

    #[test]
    fn dnst_masterdns_transport_becomes_masterdns_and_ignores_socks_login() {
        let link =
            format!("dnst://dns.example/masterdns/socks?mkey=secret&encryption=3&user=nope#Storm");
        let parsed = parse_dns_tunnel_link(&link).unwrap();
        assert_eq!(parsed.profile.profile_type, ProfileType::MasterDns);
        assert!(
            parsed
                .notes
                .iter()
                .all(|note| !note.contains("local listener")),
            "SOCKS listener credentials are ignored without a note"
        );
        let value: Value =
            serde_json::from_str(parsed.profile.outbound.raw_json.as_deref().unwrap()).unwrap();
        assert_eq!(value["mode"], "socks5");
        assert!(!value.to_string().contains("nope"));
        assert_eq!(value["encryption_method"], "aes-128-gcm");
        assert_eq!(value["encryption_key"], "secret");
        assert_eq!(value["domain"][0], "dns.example");
    }

    #[test]
    fn encrypted_slipnet_is_a_visible_error() {
        let error = parse_dns_tunnel_link("slipnet-enc://aabb").unwrap_err();
        assert!(error.contains("without encryption"));
    }

    #[test]
    fn yptun_and_profile_json_round_trip_fields() {
        let yptun = parse_dns_tunnel_link(
            "masterdns://v1?domains=One.Example,two.example&key=k&enc=1&resolvers=1.1.1.1,system&bal=1&proxy=ss://x#Node",
        )
        .unwrap();
        assert_eq!(yptun.profile.name, "Node");
        assert!(yptun.notes.iter().any(|note| note.contains("separately")));
        let value: Value =
            serde_json::from_str(yptun.profile.outbound.raw_json.as_deref().unwrap()).unwrap();
        assert_eq!(value["domain"][0], "one.example");
        assert_eq!(value["balancing_strategy"], "random");
        assert_eq!(value["resolvers"][0], "1.1.1.1");

        let document = json!({
            "schema": "whitedns.profile",
            "version": "1",
            "profile": {
                "name": "White",
                "server": {"domains": ["a.example", "a.example"], "encryptionKey": "kk", "encryptionMethod": 0}
            }
        });
        let payload = URL_SAFE_NO_PAD.encode(document.to_string());
        let profile = parse_dns_tunnel_link(&format!("stormdns://{payload}")).unwrap();
        assert_eq!(profile.profile.name, "White");
        let stored: Value =
            serde_json::from_str(profile.profile.outbound.raw_json.as_deref().unwrap()).unwrap();
        assert_eq!(stored["encryption_method"], "none");
        assert_eq!(stored["domain"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn toml_masterdns_links_are_rejected_instead_of_dropped() {
        let error = parse_dns_tunnel_link("masterdns://v1/YWJj").unwrap_err();
        assert!(error.contains("not supported"));
    }

    #[test]
    fn slipnet_v16_socks_fields_map_onto_dnstt() {
        let key = pubkey();
        let mut fields = vec![""; 36];
        fields[0] = "16";
        fields[1] = "vaydns";
        fields[2] = "Slip";
        fields[3] = "Tunnel.Example";
        fields[4] = "9.9.9.9";
        fields[11] = &key;
        fields[12] = "alice";
        fields[13] = "secret";
        fields[22] = "tls";
        let payload = URL_SAFE_NO_PAD.encode(fields.join("|"));
        let parsed = parse_dns_tunnel_link(&format!("slipnet://{payload}")).unwrap();
        let value: Value =
            serde_json::from_str(parsed.profile.outbound.raw_json.as_deref().unwrap()).unwrap();
        assert_eq!(value["mode"], "vaydns");
        assert_eq!(value["domain"], "tunnel.example");
        assert_eq!(value["resolvers"][0], "tls://9.9.9.9");
        assert_eq!(value["socks"]["username"], "alice");
    }
}
