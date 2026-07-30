//! Share-link / subscription importers.
//!
//! Behaviour is intentionally aligned with upstream
//! `Subscription::RawUpdater` in `throneproj/Throne` (`GroupUpdater.cpp`):
//! scheme dispatch, multi-line bodies, optional whole-body base64, and
//! `throne://add|route|remoteRoute|addsub/` deep links (`e7eb0438`).

mod decode;
mod deeplink;
mod links;

use throne_domain::{ProfileType, ParsedOutbound};

pub use deeplink::{Deeplink, parse_deeplink};
pub use links::parse_share_link;

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
    pub skipped: usize,
    pub errors: Vec<String>,
    /// When the input was a subscription URL deep link, the URL to fetch later.
    pub pending_sub_url: Option<String>,
    pub notes: Vec<String>,
}

impl ImportReport {
    pub fn ok_count(&self) -> usize {
        self.profiles.len()
    }
}

/// Import a blob the same way upstream `RawUpdater::update` starts:
/// try JSON later; for now handle deeplinks, multi-line links, and base64 bodies.
pub fn import_text(input: &str) -> ImportReport {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return ImportReport {
            errors: vec!["empty input".into()],
            ..Default::default()
        };
    }

    // Deep links first (throne://…)
    if let Some(dl) = parse_deeplink(trimmed) {
        return import_deeplink(dl);
    }

    // Multi-line subscription / clipboard dump
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

    // Whole-body base64 (common for subscription endpoints)
    if let Some(decoded) = decode::decode_b64_flexible(trimmed) {
        if decoded.contains('\n') || looks_like_link_list(&decoded) {
            let mut report = import_lines(&decoded);
            report.notes.push("decoded base64 subscription body".into());
            return report;
        }
        if let Some(p) = parse_share_link(decoded.trim()) {
            return ImportReport {
                profiles: vec![p],
                notes: vec!["decoded base64 single link".into()],
                ..Default::default()
            };
        }
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
            notes: vec!["throne://addsub/ — fetch URL not implemented yet".into()],
            ..Default::default()
        },
        Deeplink::Route { payload } => ImportReport {
            notes: vec![format!(
                "throne://route/ payload received ({} bytes) — route import WIP",
                payload.len()
            )],
            ..Default::default()
        },
        Deeplink::RemoteRoute { payload } => ImportReport {
            notes: vec![format!(
                "throne://remoteRoute/ payload received ({} bytes) — remote route WIP",
                payload.len()
            )],
            ..Default::default()
        },
    }
}

fn import_lines(text: &str) -> ImportReport {
    let mut report = ImportReport::default();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
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

fn looks_like_link_list(s: &str) -> bool {
    s.lines().any(|l| {
        let t = l.trim();
        t.contains("://")
            || t.starts_with("ss://")
            || t.starts_with("vmess://")
            || t.starts_with("vless://")
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
}
