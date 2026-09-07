//! Share-link / subscription / route importers.
//!
//! Pipeline mirrors upstream `Subscription::RawUpdater::update`
//! (`GroupUpdater.cpp`) and route share helpers on `RouteProfile`.

mod clash;
mod decode;
mod deeplink;
mod fetch;
mod json_sub;
mod links;
mod route_share;

use throne_domain::{ParsedOutbound, ProfileType, RouteProfile};

pub use deeplink::{parse_deeplink, Deeplink};
pub use fetch::{
    device_details, fetch_url, fetch_url_with_options, fetch_url_with_timeout, hwid_headers,
    DeviceDetails, FetchOptions, FetchResponse,
};
pub use links::parse_share_link;
pub use route_share::{
    import_route_payload, to_share_link, to_share_object, try_import_routes, RouteImportReport,
};

#[derive(Debug, Clone)]
pub struct SubscriptionImport {
    pub report: ImportReport,
    pub user_info: Option<String>,
}

pub fn import_subscription_response(response: FetchResponse) -> Result<SubscriptionImport, String> {
    let user_info = response.header("Subscription-UserInfo").map(str::to_string);
    let report = import_text(&response.body);
    if !report.errors.is_empty() {
        return Err(format!(
            "subscription was only partially parsed: {}",
            report.errors.join("; ")
        ));
    }
    if report.profiles.is_empty() {
        let detail = if report.errors.is_empty() {
            "no profiles recognized".into()
        } else {
            report.errors.join("; ")
        };
        return Err(format!(
            "subscription contained no usable profiles: {detail}"
        ));
    }
    Ok(SubscriptionImport { report, user_info })
}

/// Fetch `url` and run [`import_text`] on the body.
pub fn import_from_url(url: &str) -> ImportReport {
    match fetch_url(url) {
        Ok(body) => {
            let mut report = import_text(&body);
            if report.profiles.is_empty() && report.routes.is_empty() && report.errors.is_empty() {
                report
                    .errors
                    .push("fetched body but no profiles/routes recognized".into());
            }
            report.notes.push(format!("fetched {}", url.trim()));
            report
        }
        Err(e) => ImportReport {
            errors: vec![e],
            ..Default::default()
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedProfile {
    pub name: String,
    pub profile_type: ProfileType,
    pub outbound: ParsedOutbound,
    /// Original share link or source snippet (for debugging / re-export).
    pub source: String,
}

#[derive(Debug, Clone, Default)]
pub struct ImportReport {
    pub profiles: Vec<ImportedProfile>,
    pub routes: Vec<RouteProfile>,
    pub skipped: usize,
    pub errors: Vec<String>,
    pub pending_sub_url: Option<String>,
    pub notes: Vec<String>,
}

impl ImportReport {
    pub fn ok_count(&self) -> usize {
        self.profiles.len()
    }

    pub fn route_count(&self) -> usize {
        self.routes.len()
    }
}

/// Import a blob the same way upstream `RawUpdater::update` starts.
pub fn import_text(input: &str) -> ImportReport {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return ImportReport {
            errors: vec!["empty input".into()],
            ..Default::default()
        };
    }

    // Deep links first
    if let Some(dl) = parse_deeplink(trimmed) {
        return import_deeplink(dl);
    }

    // Route share object (tagged only — not bare JSON subs)
    if let Some(route_report) = try_import_routes(trimmed) {
        return ImportReport {
            routes: route_report.routes,
            errors: route_report.errors,
            notes: route_report.notes,
            ..Default::default()
        };
    }

    // Whole-body base64 (subscription endpoint style)
    if !trimmed.contains('\n')
        && !trimmed.contains("://")
        && !trimmed.starts_with('{')
        && !trimmed.starts_with('[')
        && !trimmed.contains("proxies:")
    {
        if let Some(decoded) = decode::decode_b64_flexible(trimmed) {
            let mut report = import_text_inner(&decoded);
            report.notes.push("decoded base64 body".into());
            return report;
        }
    }

    import_text_inner(trimmed)
}

fn import_text_inner(trimmed: &str) -> ImportReport {
    // JSON (sing-box / xray / SIP008 / custom)
    if let Some(list) = json_sub::try_import_json(trimmed) {
        return ImportReport {
            profiles: list,
            notes: vec!["json subscription".into()],
            ..Default::default()
        };
    }

    // Clash YAML
    if let Some(list) = clash::try_import_clash(trimmed) {
        return ImportReport {
            profiles: list,
            notes: vec!["clash proxies".into()],
            ..Default::default()
        };
    }

    // Amnezia vpn:// container (1.3.0-beta.2)
    if trimmed.to_ascii_lowercase().starts_with("vpn://") {
        return import_vpn_scheme(trimmed);
    }

    // WireGuard conf file
    if trimmed.contains("[Interface]") && trimmed.contains("[Peer]") {
        if let Some(p) = parse_wireguard_file(trimmed) {
            return ImportReport {
                profiles: vec![p],
                notes: vec!["wireguard conf".into()],
                ..Default::default()
            };
        }
    }

    // Multi-line share links
    if trimmed.lines().count() > 1 {
        return import_lines(trimmed);
    }

    // Single share link
    if let Some(p) = parse_share_link(trimmed) {
        return ImportReport {
            profiles: vec![p],
            ..Default::default()
        };
    }

    ImportReport {
        errors: vec!["unrecognized share link or subscription format".into()],
        skipped: 1,
        ..Default::default()
    }
}

fn import_deeplink(dl: Deeplink) -> ImportReport {
    match dl {
        Deeplink::Add { payload } => {
            let mut report = import_text(&payload);
            report.notes.push("throne://add/ deep link".into());
            report
        }
        Deeplink::AddSub { url } => ImportReport {
            pending_sub_url: Some(url),
            notes: vec!["throne://addsub/ — call import_from_url with the pending URL".into()],
            ..Default::default()
        },
        Deeplink::Route { payload } => {
            let r = route_share::import_route_payload(&payload);
            ImportReport {
                routes: r.routes,
                errors: r.errors,
                notes: {
                    let mut n = r.notes;
                    n.push("throne://route/ deep link".into());
                    n
                },
                ..Default::default()
            }
        }
        Deeplink::RemoteRoute { payload } => {
            let r = route_share::import_route_payload_as_remote(&payload);
            ImportReport {
                routes: r.routes,
                errors: r.errors,
                notes: {
                    let mut n = r.notes;
                    n.push("throne://remoteRoute/ deep link".into());
                    n
                },
                ..Default::default()
            }
        }
    }
}

fn import_lines(text: &str) -> ImportReport {
    let mut report = ImportReport::default();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        // Nested JSON object line
        if line.to_ascii_lowercase().starts_with("vpn://") {
            let inner = import_vpn_scheme(line);
            report.profiles.extend(inner.profiles);
            report.errors.extend(inner.errors);
            report.skipped += inner.skipped;
            continue;
        }
        if line.starts_with('{') {
            if let Some(list) = json_sub::try_import_json(line) {
                report.profiles.extend(list);
                continue;
            }
        }
        match parse_share_link(line) {
            Some(p) => report.profiles.push(p),
            None => {
                report.skipped += 1;
                report.errors.push(format!("skip: {line}"));
            }
        }
    }
    if report.profiles.is_empty() && report.errors.is_empty() {
        report.errors.push("no profiles found".into());
    }
    report
}

pub(crate) fn parse_wireguard_file(text: &str) -> Option<ImportedProfile> {
    let mut private_key = None;
    let mut address = None;
    let mut public_key = None;
    let mut endpoint = None;
    let mut preshared = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim().to_ascii_lowercase().replace('_', "");
        let v = v.trim().to_string();
        match k.as_str() {
            "privatekey" => private_key = Some(v),
            "address" => address = Some(v.replace(' ', "")),
            "publickey" => public_key = Some(v),
            "presharedkey" | "psk" => preshared = Some(v),
            "endpoint" => endpoint = Some(v),
            _ => {}
        }
    }
    let endpoint = endpoint?;
    let (host, port) = match endpoint.rsplit_once(':') {
        Some((h, p)) => (
            h.trim().trim_matches('[').trim_matches(']').to_string(),
            p.trim().parse().ok().filter(|n| *n > 0).unwrap_or(51820),
        ),
        None => return None,
    };
    let private_key = private_key.filter(|s| !s.is_empty())?;
    let public_key = public_key.filter(|s| !s.is_empty())?;
    let name = "WireGuard".to_string();
    let outbound = ParsedOutbound {
        tag: Some(name.clone()),
        server: Some(host),
        server_port: Some(port),
        password: Some(private_key),
        username: Some(public_key),
        method: preshared,
        path: address,
        raw_json: Some(text.to_string()),
        ..Default::default()
    };
    Some(ImportedProfile {
        name,
        profile_type: ProfileType::Wireguard,
        outbound,
        source: "wg-conf".into(),
    })
}

/// Amnezia `vpn://` payload: percent-decode + base64, then JSON containers or a conf body.
fn import_vpn_scheme(link: &str) -> ImportReport {
    let after = link
        .trim()
        .get(6..)
        .unwrap_or("")
        .split('#')
        .next()
        .unwrap_or("");
    let decoded = percent_decode_bytes(after);
    let bytes = decode::decode_b64_bytes(&decoded).or_else(|| decode::decode_b64_bytes(after));
    let Some(bytes) = bytes else {
        return ImportReport {
            errors: vec!["vpn:// payload is not valid base64".into()],
            skipped: 1,
            ..Default::default()
        };
    };
    let bytes = maybe_quncompress(&bytes);
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
        let mut report = ImportReport::default();
        report.notes.push("vpn://".into());
        collect_vpn_containers(&v, &mut report);
        if report.profiles.is_empty() && report.errors.is_empty() {
            report.errors.push("vpn:// JSON had no configs".into());
        }
        return report;
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let mut report = import_text_inner(text.trim());
    report.notes.insert(0, "vpn://".into());
    report
}

/// Qt `qUncompress`: 4-byte big-endian size prefix + zlib. Also try raw zlib.
fn maybe_quncompress(bytes: &[u8]) -> Vec<u8> {
    use std::io::Read;
    if bytes.len() > 4 {
        let mut dec = flate2::read::ZlibDecoder::new(&bytes[4..]);
        let mut out = Vec::new();
        if dec.read_to_end(&mut out).is_ok() && !out.is_empty() {
            return out;
        }
    }
    let mut dec = flate2::read::ZlibDecoder::new(bytes);
    let mut out = Vec::new();
    if dec.read_to_end(&mut out).is_ok() && !out.is_empty() {
        return out;
    }
    bytes.to_vec()
}

fn percent_decode_bytes(s: &str) -> String {
    urlencoding::decode(s)
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| s.to_string())
}

fn collect_vpn_containers(v: &serde_json::Value, report: &mut ImportReport) {
    let Some(containers) = v.get("containers").and_then(|c| c.as_array()) else {
        return;
    };
    for c in containers {
        let Some(obj) = c.as_object() else { continue };
        for proto in obj.values() {
            let conf = proto
                .get("last_config")
                .and_then(vpn_last_config_text)
                .unwrap_or_default();
            let conf = conf.trim();
            if conf.is_empty() {
                continue;
            }
            let inner = import_text_inner(conf);
            report.profiles.extend(inner.profiles);
            report.errors.extend(inner.errors);
            report.skipped += inner.skipped;
        }
    }
}

fn vpn_last_config_text(v: &serde_json::Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        if let Ok(inner) = serde_json::from_str::<serde_json::Value>(s) {
            if let Some(cfg) = inner.get("config").and_then(|c| c.as_str()) {
                return Some(cfg.to_string());
            }
        }
        return Some(s.to_string());
    }
    v.as_object()
        .and_then(|o| o.get("config"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_response_carries_provider_info() {
        let response = FetchResponse {
            body: "vless://11111111-1111-1111-1111-111111111111@example.com:443#Demo".into(),
            headers: vec![("Subscription-UserInfo".into(), "total=100".into())],
        };
        let imported = import_subscription_response(response).unwrap();
        assert_eq!(imported.user_info.as_deref(), Some("total=100"));
        assert_eq!(imported.report.profiles.len(), 1);
    }

    #[test]
    fn subscription_response_rejects_unrecognized_body() {
        let response = FetchResponse {
            body: "not a subscription".into(),
            headers: Vec::new(),
        };
        assert!(import_subscription_response(response).is_err());
    }

    #[test]
    fn subscription_response_rejects_partial_parse_to_protect_existing_snapshot() {
        let response = FetchResponse {
            body: "vless://11111111-1111-1111-1111-111111111111@example.com:443#Demo\nnot-a-link"
                .into(),
            headers: Vec::new(),
        };

        assert!(import_subscription_response(response).is_err());
    }

    #[test]
    fn import_vless_line() {
        let link = "vless://11111111-1111-1111-1111-111111111111@example.com:443?encryption=none&security=tls&sni=example.com&type=ws&path=%2Fws#Demo-VLESS";
        let r = import_text(link);
        assert_eq!(r.ok_count(), 1);
        assert_eq!(r.profiles[0].profile_type, ProfileType::Vless);
        assert_eq!(r.profiles[0].name, "Demo-VLESS");
        assert_eq!(
            r.profiles[0].outbound.server.as_deref(),
            Some("example.com")
        );
        assert_eq!(r.profiles[0].outbound.server_port, Some(443));
    }

    #[test]
    fn import_multiline_mixed() {
        let body = r#"
# comment
trojan://secret@host.example:443?security=tls&sni=host.example#T1
vless://22222222-2222-2222-2222-222222222222@a.com:8443#T2
not-a-link
"#;
        let r = import_text(body);
        assert_eq!(r.ok_count(), 2);
        assert_eq!(r.skipped, 1);
    }

    #[test]
    fn import_ss_2022_style() {
        let link = "ss://aes-256-gcm:p%40ss@1.2.3.4:8388#MySS";
        let r = import_text(link);
        assert_eq!(r.ok_count(), 1);
        assert_eq!(r.profiles[0].profile_type, ProfileType::Shadowsocks);
        assert_eq!(r.profiles[0].outbound.password.as_deref(), Some("p@ss"));
        assert_eq!(
            r.profiles[0].outbound.method.as_deref(),
            Some("aes-256-gcm")
        );
    }

    #[test]
    fn import_wireguard_link_and_ini() {
        let link = "wireguard://pk@10.1.1.1:51820?public_key=pubk&address=10.0.0.2/32#WG1";
        let r = import_text(link);
        assert_eq!(r.ok_count(), 1);
        assert_eq!(r.profiles[0].profile_type, ProfileType::Wireguard);
        assert_eq!(r.profiles[0].outbound.server.as_deref(), Some("10.1.1.1"));
        assert_eq!(r.profiles[0].outbound.server_port, Some(51820));
        assert_eq!(r.profiles[0].outbound.password.as_deref(), Some("pk"));
        assert_eq!(r.profiles[0].outbound.username.as_deref(), Some("pubk"));
        let json: serde_json::Value =
            serde_json::from_str(&r.profiles[0].outbound.to_db_json(ProfileType::Wireguard))
                .unwrap();
        assert_eq!(json["private_key"], "pk");
        assert_eq!(json["peer_public_key"], "pubk");
        assert_eq!(json["local_address"][0], "10.0.0.2/32");

        let ini = "[Interface]\nPrivateKey = aaa\nAddress = 10.0.0.2/32\n[Peer]\nPublicKey = bbb\nPresharedKey = psk\nEndpoint = example.com:51820\n";
        let r = import_text(ini);
        assert_eq!(r.ok_count(), 1);
        assert_eq!(r.profiles[0].outbound.username.as_deref(), Some("bbb"));
        assert_eq!(r.profiles[0].outbound.method.as_deref(), Some("psk"));
        let json: serde_json::Value =
            serde_json::from_str(&r.profiles[0].outbound.to_db_json(ProfileType::Wireguard))
                .unwrap();
        assert_eq!(json["private_key"], "aaa");
        assert_eq!(json["peer_public_key"], "bbb");
        assert_eq!(json["pre_shared_key"], "psk");
    }

    #[test]
    fn import_vpn_scheme_json_container() {
        let inner = serde_json::json!({
            "containers": [{
                "awg": {
                    "last_config": "[Interface]\nPrivateKey = aaa\nAddress = 10.0.0.2/32\n[Peer]\nPublicKey = bbb\nEndpoint = 1.2.3.4:51820\n"
                }
            }]
        });
        let b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            inner.to_string().as_bytes(),
        );
        let r = import_text(&format!("vpn://{b64}"));
        assert_eq!(r.ok_count(), 1, "{:?}", r.errors);
        assert_eq!(r.profiles[0].profile_type, ProfileType::Wireguard);
    }

    #[test]
    fn import_vless_finalmask_query() {
        let link = "vless://11111111-1111-1111-1111-111111111111@example.com:443?encryption=none&security=tls&fm=%7B%22k%22%3A1%7D#FM";
        let r = import_text(link);
        assert_eq!(r.ok_count(), 1);
        assert_eq!(
            r.profiles[0].outbound.finalmask.as_deref(),
            Some(r#"{"k":1}"#)
        );
        let json: serde_json::Value =
            serde_json::from_str(&r.profiles[0].outbound.to_db_json(ProfileType::Vless)).unwrap();
        assert_eq!(json["finalmask"]["k"], 1);
    }

    #[test]
    fn import_snell_link() {
        let link = "snell://psk@example.com:440?version=4#Node";
        let r = import_text(link);
        assert_eq!(r.ok_count(), 1);
        assert_eq!(r.profiles[0].profile_type, ProfileType::Snell);
        assert_eq!(r.profiles[0].outbound.password.as_deref(), Some("psk"));
        assert_eq!(r.profiles[0].outbound.method.as_deref(), Some("4"));
    }

    #[test]
    fn deeplink_add_wraps_payload() {
        let inner = "vless://33333333-3333-3333-3333-333333333333@b.com:443#ViaAdd";
        let b64 = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            inner.as_bytes(),
        );
        let r = import_text(&format!("throne://add/{b64}"));
        assert_eq!(r.ok_count(), 1);
        assert!(r.notes.iter().any(|n| n.contains("throne://add")));
    }

    #[test]
    fn import_singbox_outbound_array() {
        let json = r#"[
          {"type":"vless","tag":"a","server":"1.1.1.1","server_port":443,"uuid":"u"},
          {"type":"direct","tag":"direct"}
        ]"#;
        let r = import_text(json);
        assert_eq!(r.ok_count(), 1);
        assert_eq!(r.profiles[0].name, "a");
    }

    #[test]
    fn import_clash_yaml() {
        let yaml = r#"
proxies:
  - { name: "c1", type: ss, server: 9.9.9.9, port: 1234, cipher: aes-128-gcm, password: x }
"#;
        let r = import_text(yaml);
        assert_eq!(r.ok_count(), 1);
        assert!(r.notes.iter().any(|n| n.contains("clash")));
    }

    #[test]
    fn import_route_share() {
        let json = r#"{"kind":"throne-route-profile","v":1,"name":"R1","default_outbound":"proxy","rules":[]}"#;
        let r = import_text(json);
        assert_eq!(r.route_count(), 1);
        assert_eq!(r.routes[0].name, "R1");
    }
}
