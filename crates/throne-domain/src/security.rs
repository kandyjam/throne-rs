//! Config security labels from upstream 1.4 (`Outbound::EffectiveSecurity`).
//! These describe the selected outbound configuration, never the stale persisted warning flag.

use std::net::IpAddr;

use serde_json::Value;

use crate::{Profile, ProfileType};

/// Ordered worst to best, matching upstream security-column sorting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecurityLevel {
    #[default]
    Unknown,
    None,
    Weak,
    Secure,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecurityInfo {
    pub label: &'static str,
    pub transport: String,
    pub level: SecurityLevel,
    pub ca_verified: bool,
    /// A global option weakened an otherwise verified profile.
    pub compromised: bool,
}

impl SecurityInfo {
    pub fn is_dangerous(&self) -> bool {
        matches!(self.level, SecurityLevel::None | SecurityLevel::Weak)
    }

    /// Global certificate skipping must not make a profile eligible for removal.
    pub fn is_insecure(&self) -> bool {
        self.is_dangerous() && !self.compromised
    }

    pub fn display(&self) -> String {
        if self.label.is_empty() {
            return String::new();
        }
        let text = if self.transport.is_empty() {
            self.label.to_string()
        } else {
            format!("{}+{}", self.transport, self.label)
        };
        if self.is_dangerous() {
            format!("⚠️ {text}")
        } else {
            text
        }
    }

    fn new(label: &'static str, level: SecurityLevel) -> Self {
        Self {
            label,
            level,
            ..Self::default()
        }
    }

    fn with_private_server(mut self, host: &str) -> Self {
        if self.is_dangerous() && is_private_host(host) {
            self.label = "Private";
            self.level = SecurityLevel::Secure;
            self.compromised = false;
        }
        self
    }
}

/// Private/special-use destinations where the upstream internet-security warning is moot.
/// This deliberately does not resolve DNS or classify ordinary public names by their current IP.
pub fn is_private_host(host: &str) -> bool {
    let host = host
        .trim()
        .trim_matches(['[', ']'])
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        let v4 = match ip {
            IpAddr::V4(ip) => Some(ip),
            IpAddr::V6(ip) => ip.to_ipv4_mapped(),
        };
        if let Some(ip) = v4 {
            let ip = u32::from(ip);
            const RANGES: &[(u32, u32)] = &[
                (0x00000000, 8),
                (0x0a000000, 8),
                (0x64400000, 10),
                (0x7f000000, 8),
                (0xa9fe0000, 16),
                (0xac100000, 12),
                (0xc0000000, 24),
                (0xc0000200, 24),
                (0xc0586300, 24),
                (0xc0a80000, 16),
                (0xc6120000, 15),
                (0xc6336400, 24),
                (0xcb007100, 24),
                (0xe0000000, 3),
            ];
            return RANGES
                .iter()
                .any(|&(network, prefix)| ip >> (32 - prefix) == network >> (32 - prefix));
        }
        if let IpAddr::V6(ip) = ip {
            let n = u128::from(ip);
            return n <= 1 || n >> 121 == 0x7e || n >> 118 == 0x3fa || n >> 120 == 0xff;
        }
    }
    const SUFFIXES: &[&str] = &[
        "lan",
        "localdomain",
        "example",
        "invalid",
        "localhost",
        "test",
        "local",
        "home.arpa",
        "internal",
    ];
    if SUFFIXES
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
    {
        return true;
    }
    let bytes = host.as_bytes();
    bytes.len() <= 63
        && bytes[0].is_ascii_alphabetic()
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'-')
}

/// Derive the warning shown for a profile, including global skip-cert and private endpoints.
pub fn profile_security(profile: &Profile, skip_cert: bool) -> SecurityInfo {
    if matches!(
        profile.profile_type,
        ProfileType::Chain
            | ProfileType::AutoSelector
            | ProfileType::Direct
            | ProfileType::ExtraCore
    ) {
        return SecurityInfo::default();
    }
    let value = [
        &profile.outbound_json,
        profile.outbound.raw_json.as_deref().unwrap_or(""),
    ]
    .into_iter()
    .filter_map(|raw| serde_json::from_str::<Value>(raw).ok())
    .find(|value| {
        ["type", "protocol", "subtype", "outbounds"]
            .iter()
            .any(|key| value.get(key).is_some())
    })
    .or_else(|| serde_json::from_str(&profile.export_outbound_json()).ok())
    .unwrap_or(Value::Null);
    if profile.profile_type == ProfileType::Custom {
        return custom_security(&value);
    }
    if value.get("protocol").and_then(Value::as_str).is_some() {
        return xray_security(&value);
    }
    singbox_security(profile.profile_type, &value, skip_cert)
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn nonempty(value: &Value) -> bool {
    match value {
        Value::String(s) => !s.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(items) => !items.is_empty(),
        _ => false,
    }
}

fn transport_name(raw: &str) -> String {
    match raw {
        "" | "tcp" | "raw" => String::new(),
        "ws" | "websocket" => "WebSocket".into(),
        "grpc" => "gRPC".into(),
        "http" | "h2" => "HTTP".into(),
        "httpupgrade" => "HTTPUpgrade".into(),
        "xhttp" => "XHTTP".into(),
        other => other.to_uppercase(),
    }
}

fn singbox_security(ty: ProfileType, value: &Value, skip_cert: bool) -> SecurityInfo {
    use ProfileType::*;
    use SecurityLevel::{None, Secure, Weak};
    let tls = &value["tls"];
    let mut info = match ty {
        Chain | AutoSelector | Custom | Direct | ExtraCore => return SecurityInfo::default(),
        Shadowsocks => {
            let method = text(value, "method");
            match method {
                "" | "none" => SecurityInfo::new("Raw", None),
                "aes-128-ctr" | "aes-192-ctr" | "aes-256-ctr" | "aes-128-cfb" | "aes-192-cfb"
                | "aes-256-cfb" | "rc4-md5" | "chacha20-ietf" | "xchacha20" => {
                    SecurityInfo::new("Weak Cipher", Weak)
                }
                _ => SecurityInfo::new("Encrypted", Secure),
            }
        }
        Ssh => {
            if nonempty(&value["host_key"]) {
                SecurityInfo::new("Encrypted", Secure)
            } else {
                SecurityInfo::new("Unverified Host Key", Weak)
            }
        }
        OpenVpn => {
            if text(value, "mode") == "static_key" {
                SecurityInfo::new("Static Key", Weak)
            } else if ["certificate", "certificate_path", "peer_fingerprint"]
                .iter()
                .any(|key| nonempty(&tls[key]))
            {
                SecurityInfo::new("TLS", Secure)
            } else {
                SecurityInfo::new("Unverified TLS", Weak)
            }
        }
        OpenConnect => {
            if tls["insecure"] == true && !nonempty(&tls["peer_fingerprint"]) {
                SecurityInfo::new("Insecure TLS", Weak)
            } else {
                SecurityInfo::new("TLS", Secure)
            }
        }
        Wireguard | Snell | Mieru | Tailscale => SecurityInfo::new("Encrypted", Secure),
        _ => {
            let must_tls = matches!(
                ty,
                AnyTls | Hysteria | Hysteria2 | Juicity | Naive | ShadowTls | TrustTunnel | Tuic
            );
            let has_tls = !matches!(ty, Socks | XrayVless);
            let mut info = if has_tls && (must_tls || tls["enabled"] == true) {
                if tls["reality"]["enabled"] == true {
                    SecurityInfo::new("Reality", Secure)
                } else if nonempty(&tls["certificate_public_key_sha256"]) {
                    SecurityInfo::new("TLS", Secure)
                } else if tls["insecure"] == true {
                    SecurityInfo::new("Insecure TLS", Weak)
                } else {
                    SecurityInfo {
                        ca_verified: true,
                        ..SecurityInfo::new("TLS", Secure)
                    }
                }
            } else {
                SecurityInfo::new("Raw", None)
            };
            if ty == Vmess
                && info.level == None
                && !matches!(text(value, "security"), "none" | "zero")
            {
                info = SecurityInfo::new("Insecure", Weak);
            }
            if ty == XrayVless {
                if tls["reality"]["enabled"] == true {
                    info = SecurityInfo::new("Reality", Secure);
                } else if tls["enabled"] == true {
                    info = SecurityInfo::new("TLS", Secure);
                } else if !matches!(text(value, "encryption"), "" | "none") {
                    info = SecurityInfo::new("Encrypted", Secure);
                }
            }
            info
        }
    };
    info.transport = match ty {
        Hysteria | Hysteria2 | Tuic | Juicity => "QUIC".into(),
        Naive | TrustTunnel if value["quic"] == true => "QUIC".into(),
        Tailscale => "WireGuard".into(),
        Wireguard if value["enable_amnezia"] == true => "AmneziaWG".into(),
        Shadowsocks | Ssh | OpenVpn | OpenConnect | Wireguard | Snell | Mieru => String::new(),
        _ => transport_name(text(&value["transport"], "type")),
    };
    if skip_cert && info.ca_verified && ty != Naive {
        info.label = "Compromised";
        info.level = Weak;
        info.compromised = true;
    }
    info.with_private_server(text(value, "server"))
}

fn custom_security(value: &Value) -> SecurityInfo {
    let subtype = text(value, "subtype");
    let config = match value.get("config") {
        Some(Value::String(raw)) => serde_json::from_str(raw).unwrap_or(Value::Null),
        Some(config) => config.clone(),
        _ => value.clone(),
    };
    if subtype == "xrayfullconfig" {
        return config["outbounds"]
            .as_array()
            .and_then(|outbounds| {
                outbounds
                    .iter()
                    .find(|out| !xray_infrastructure(text(out, "protocol")))
            })
            .map(xray_security)
            .unwrap_or_default();
    }
    if subtype == "xrayoutbound" || config.get("protocol").is_some() {
        return xray_security(&config);
    }
    let outbound = if subtype == "fullconfig" || config.get("outbounds").is_some() {
        resolve_singbox_egress(&config, text(&config["route"], "final"), 5)
    } else {
        Some(&config)
    };
    outbound
        .and_then(|outbound| {
            if text(outbound, "type") == "masque" {
                Some(masque_security(outbound))
            } else {
                ProfileType::from_upstream(text(outbound, "type"))
                    .map(|ty| singbox_security(ty, outbound, false))
            }
        })
        .unwrap_or_default()
}

/// MASQUE is currently retained as a custom raw outbound. Upstream always
/// enables its TLS, and its peer key pins the certificate even with insecure
/// TLS. Global bean options do not rewrite custom raw configurations.
fn masque_security(value: &Value) -> SecurityInfo {
    let tls = &value["tls"];
    let info = if !text(value, "peer_public_key").is_empty()
        || nonempty(&tls["certificate_public_key_sha256"])
    {
        SecurityInfo::new("TLS", SecurityLevel::Secure)
    } else if tls["insecure"] == true {
        SecurityInfo::new("Insecure TLS", SecurityLevel::Weak)
    } else {
        SecurityInfo {
            ca_verified: true,
            ..SecurityInfo::new("TLS", SecurityLevel::Secure)
        }
    };
    info.with_private_server(text(value, "server"))
}

fn resolve_singbox_egress<'a>(config: &'a Value, tag: &str, depth: usize) -> Option<&'a Value> {
    if depth == 0 {
        return None;
    }
    let outbounds = config["outbounds"].as_array()?;
    let target = if tag.is_empty() {
        outbounds.first()?
    } else {
        outbounds.iter().find(|out| text(out, "tag") == tag)?
    };
    match text(target, "type") {
        "selector" | "urltest" => {
            let mut next = text(target, "default");
            if next.is_empty() {
                next = target["outbounds"].as_array()?.first()?.as_str()?;
            }
            resolve_singbox_egress(config, next, depth - 1)
        }
        "" | "direct" | "block" | "dns" => None,
        _ => Some(target),
    }
}

fn xray_infrastructure(protocol: &str) -> bool {
    matches!(
        protocol,
        "freedom" | "direct" | "blackhole" | "block" | "dns" | "loopback"
    )
}

fn xray_security(value: &Value) -> SecurityInfo {
    use SecurityLevel::{None, Secure, Weak};
    let protocol = text(value, "protocol");
    if protocol.is_empty() || xray_infrastructure(protocol) {
        return SecurityInfo::default();
    }
    let settings = &value["settings"];
    let peer = if settings.get("address").is_some() {
        settings
    } else {
        let peers = if settings.get("vnext").is_some() {
            &settings["vnext"]
        } else {
            &settings["servers"]
        };
        peers
            .as_array()
            .and_then(|peers| peers.first())
            .unwrap_or(&Value::Null)
    };
    let user = peer["users"]
        .as_array()
        .and_then(|users| users.first())
        .unwrap_or(peer);
    let stream = &value["streamSettings"];
    let mut info = match text(stream, "security") {
        "reality" => SecurityInfo::new("Reality", Secure),
        "tls" if stream["tlsSettings"]["allowInsecure"] == true => {
            SecurityInfo::new("Insecure TLS", Weak)
        }
        "tls" => SecurityInfo::new("TLS", Secure),
        _ if matches!(protocol, "shadowsocks" | "wireguard")
            || (protocol == "vless" && !matches!(text(user, "encryption"), "" | "none")) =>
        {
            SecurityInfo::new("Encrypted", Secure)
        }
        _ if protocol == "vmess" => SecurityInfo::new("Insecure", Weak),
        _ => SecurityInfo::new("Raw", None),
    };
    info.transport = transport_name(text(stream, "network"));
    info.with_private_server(text(peer, "address"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn profile(ty: ProfileType, value: Value) -> Profile {
        let mut p = Profile::new(1, 1, "security", ty);
        p.outbound_json = value.to_string();
        p
    }

    #[test]
    fn private_hosts_include_mapped_special_use_and_local_names_only() {
        for host in [
            "127.0.0.1",
            "[::ffff:192.168.1.2]",
            "100.64.0.1",
            "192.0.2.1",
            "198.19.0.1",
            "224.0.0.1",
            "::",
            "::1",
            "fc00::1",
            "fe80::1",
            "ff02::1",
            "Printer",
            "host.home.arpa.",
            "a.local",
        ] {
            assert!(is_private_host(host), "{host}");
        }
        for host in [
            "",
            "8.8.8.8",
            "100.128.0.1",
            "2001:4860::1",
            "1.2.3.4",
            "example.com",
            "localhost.com",
            "1printer",
            "printer-",
            "bad name",
        ] {
            assert!(!is_private_host(host), "{host}");
        }
    }

    #[test]
    fn skip_cert_compromises_ca_tls_but_does_not_make_it_removable() {
        let p = profile(
            ProfileType::Vless,
            json!({"type":"vless", "server":"example.com", "tls":{"enabled":true}}),
        );
        assert_eq!(profile_security(&p, false).level, SecurityLevel::Secure);
        let info = profile_security(&p, true);
        assert!(info.is_dangerous());
        assert!(!info.is_insecure());
        assert_eq!(info.label, "Compromised");
        assert_eq!(info.display(), "⚠️ Compromised");
    }

    #[test]
    fn pinned_tls_reality_naive_and_custom_are_not_globally_compromised() {
        for tls in [
            json!({"enabled":true,"insecure":true,"certificate_public_key_sha256":["pin"]}),
            json!({"enabled":true,"reality":{"enabled":true}}),
        ] {
            let p = profile(
                ProfileType::Vless,
                json!({"type":"vless","server":"example.com","tls":tls}),
            );
            assert!(!profile_security(&p, true).is_dangerous());
        }
        let naive = profile(
            ProfileType::Naive,
            json!({"type":"naive","server":"example.com","tls":{"enabled":true}}),
        );
        assert!(!profile_security(&naive, true).is_dangerous());
        let custom = profile(
            ProfileType::Custom,
            json!({"subtype":"outbound","config":json!({"type":"vless","server":"example.com","tls":{"enabled":true}}).to_string()}),
        );
        assert!(!profile_security(&custom, true).is_dangerous());
    }

    #[test]
    fn private_destination_suppresses_raw_or_globally_compromised_warning() {
        let private = profile(
            ProfileType::Socks,
            json!({"type":"socks","server":"router.local"}),
        );
        assert_eq!(profile_security(&private, false).label, "Private");
        let tls = profile(
            ProfileType::Trojan,
            json!({"type":"trojan","server":"10.0.0.1","tls":{"enabled":true}}),
        );
        assert_eq!(profile_security(&tls, true).label, "Private");
        assert!(!profile_security(&tls, true).compromised);
    }

    #[test]
    fn masque_peer_pin_is_secure_even_with_insecure_tls_and_global_skip_cert() {
        let raw = json!({"type":"masque","server":"example.com","peer_public_key":"pin","tls":{"enabled":false,"insecure":true}});
        for stored in [
            raw.clone(),
            json!({"type":"custom","subtype":"outbound","config":raw.to_string()}),
            json!({"type":"custom","subtype":"outbound","config":raw}),
        ] {
            let pinned = profile(ProfileType::Custom, stored);
            for skip_cert in [false, true] {
                let info = profile_security(&pinned, skip_cert);
                assert_eq!(info.level, SecurityLevel::Secure);
                assert_eq!(info.label, "TLS");
                assert!(
                    !info.ca_verified,
                    "the peer pin verifies the server instead of a CA"
                );
                assert!(!info.compromised);
                assert!(
                    !info.is_insecure(),
                    "pinned MASQUE must survive insecure cleanup"
                );
            }
        }
    }

    #[test]
    fn custom_masque_requires_tls_even_when_disabled_and_rejects_unverified_public_peers() {
        for tls in [Value::Null, json!({"enabled":false})] {
            let ca_tls = profile(
                ProfileType::Custom,
                json!({"type":"masque","server":"example.com","tls":tls}),
            );
            for skip_cert in [false, true] {
                let info = profile_security(&ca_tls, skip_cert);
                assert_eq!(info.level, SecurityLevel::Secure);
                assert!(info.ca_verified);
                assert!(
                    !info.compromised,
                    "global TLS overrides do not rewrite custom configs"
                );
            }
        }
        let unverified = profile(
            ProfileType::Custom,
            json!({"type":"masque","server":"example.com","peer_public_key":"","tls":{"enabled":false,"insecure":true,"reality":{"enabled":true}}}),
        );
        assert!(
            profile_security(&unverified, false).is_insecure(),
            "MASQUE does not use Reality"
        );
        assert!(profile_security(&unverified, true).is_insecure());
        let full = profile(
            ProfileType::Custom,
            json!({"subtype":"fullconfig","config":{
                "route":{"final":"chooser"},"outbounds":[{"type":"selector","tag":"chooser","outbounds":["masque"]},
                {"type":"masque","tag":"masque","server":"example.com","peer_public_key":"pin","tls":{"insecure":true}}]
            }}),
        );
        assert_eq!(profile_security(&full, true).level, SecurityLevel::Secure);
    }

    #[test]
    fn vmess_without_tls_and_unverified_ssh_are_insecure() {
        let vmess = profile(
            ProfileType::Vmess,
            json!({"type":"vmess","server":"example.com","security":"auto","tls":{"enabled":false}}),
        );
        assert_eq!(profile_security(&vmess, false).label, "Insecure");
        assert!(profile_security(&vmess, false).is_insecure());
        let ssh = profile(
            ProfileType::Ssh,
            json!({"type":"ssh","server":"example.com"}),
        );
        assert_eq!(profile_security(&ssh, false).label, "Unverified Host Key");
    }

    #[test]
    fn xray_inline_peer_overrides_vnext_for_encryption_and_private_address() {
        let inline = profile(
            ProfileType::Custom,
            json!({"subtype":"xrayoutbound","config":json!({"protocol":"vless","settings":{"address":"example.com","encryption":"mlkem768x25519plus.native.600s","vnext":[{"address":"127.0.0.1","users":[{"encryption":"none"}]}]}}).to_string()}),
        );
        assert_eq!(profile_security(&inline, false).label, "Encrypted");
        let full = profile(
            ProfileType::Custom,
            json!({"subtype":"xrayfullconfig","config":json!({"outbounds":[{"protocol":"direct"},{"protocol":"block"},{"protocol":"vmess","settings":{"address":"example.com"}}]}).to_string()}),
        );
        assert_eq!(profile_security(&full, false).label, "Insecure");
    }

    #[test]
    fn custom_full_config_follows_selector_without_recursion_cycles() {
        let full = profile(
            ProfileType::Custom,
            json!({"subtype":"fullconfig","config":json!({"route":{"final":"choose"},"outbounds":[{"type":"selector","tag":"choose","outbounds":["leaf"]},{"type":"socks","tag":"leaf","server":"127.0.0.1"}]}).to_string()}),
        );
        assert_eq!(profile_security(&full, false).label, "Private");
        let cycle = profile(
            ProfileType::Custom,
            json!({"subtype":"fullconfig","config":json!({"outbounds":[{"type":"selector","tag":"loop","outbounds":["loop"]}]}).to_string()}),
        );
        assert_eq!(
            profile_security(&cycle, false).level,
            SecurityLevel::Unknown
        );
    }
}
