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

pub use deeplink::{Deeplink, parse_deeplink};
pub use fetch::{fetch_url, fetch_url_with_timeout};
pub use links::parse_share_link;
pub use route_share::{RouteImportReport, to_share_object, try_import_routes};

/// Fetch `url` and run [`import_text`] on the body.
pub fn import_from_url(url: &str) -> ImportReport {
    match fetch_url(url) {
        Ok(body) => {
            let mut report = import_text(&body);
            if report.profiles.is_empty()
                && report.routes.is_empty()
                && report.errors.is_empty()
            {
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

fn parse_wireguard_file(text: &str) -> Option<ImportedProfile> {
    let mut private_key = None;
    let mut address = None;
    let mut public_key = None;
    let mut endpoint = None;
    let mut section = "";
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = line;
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim().to_string();
            match (section, k.as_str()) {
                ("[Interface]", "privatekey") => private_key = Some(v),
                ("[Interface]", "address") => address = Some(v),
                ("[Peer]", "publickey") => public_key = Some(v),
                ("[Peer]", "endpoint") => endpoint = Some(v),
                _ => {}
            }
        }
    }
    let endpoint = endpoint?;
    let (host, port) = match endpoint.rsplit_once(':') {
        Some((h, p)) => (h.trim_matches('[').trim_matches(']').to_string(), p.parse().ok()?),
        None => return None,
    };
    let name = "WireGuard".to_string();
    let mut outbound = ParsedOutbound {
        tag: Some(name.clone()),
        server: Some(host),
        server_port: Some(port),
        password: private_key,
        username: public_key,
        path: address,
        raw_json: Some(text.to_string()),
        ..Default::default()
    };
    outbound.raw_json = Some(text.to_string());
    Some(ImportedProfile {
        name,
        profile_type: ProfileType::Wireguard,
        outbound,
        source: "wg-conf".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_vless_line() {
        let link = "vless://11111111-1111-1111-1111-111111111111@example.com:443?encryption=none&security=tls&sni=example.com&type=ws&path=%2Fws#Demo-VLESS";
        let r = import_text(link);
        assert_eq!(r.ok_count(), 1);
        assert_eq!(r.profiles[0].profile_type, ProfileType::Vless);
        assert_eq!(r.profiles[0].name, "Demo-VLESS");
        assert_eq!(r.profiles[0].outbound.server.as_deref(), Some("example.com"));
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
        assert_eq!(r.profiles[0].outbound.method.as_deref(), Some("aes-256-gcm"));
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
