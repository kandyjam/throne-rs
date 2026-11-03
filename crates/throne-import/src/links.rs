use url::Url;

use throne_domain::{ParsedOutbound, ProfileType};

use crate::decode::{decode_b64_flexible, percent_decode};
use crate::ImportedProfile;

/// Parse one share link into a profile draft. Returns `None` if scheme unknown
/// or required fields missing (mirrors upstream `ParseFromLink` failure).
pub fn parse_share_link(raw: &str) -> Option<ImportedProfile> {
    let link = raw.trim();
    if link.is_empty() {
        return None;
    }

    let lower = link.to_ascii_lowercase();
    if lower.starts_with("ss://") {
        return parse_ss(link);
    }
    if lower.starts_with("vmess://") {
        return parse_vmess(link);
    }
    if lower.starts_with("vless://") {
        return parse_vless(link);
    }
    if lower.starts_with("trojan://") {
        return parse_trojan(link);
    }
    if lower.starts_with("hysteria2://")
        || lower.starts_with("hy2://")
        || lower.starts_with("hysteria://")
    {
        return parse_hysteria(link);
    }
    if lower.starts_with("tuic://") {
        return parse_tuic(link);
    }
    if lower.starts_with("socks://") || lower.starts_with("socks5://") {
        return parse_socks(link);
    }
    if lower.starts_with("snell://") {
        return parse_snell(link);
    }
    if lower.starts_with("wg://")
        || lower.starts_with("wireguard://")
        || lower.starts_with("awg://")
    {
        return parse_wireguard_link(link);
    }
    if lower.starts_with("http://") || lower.starts_with("https://") {
        // Only treat as HTTP proxy share link when userinfo is present.
        if let Ok(u) = Url::parse(link) {
            if !u.username().is_empty() {
                return parse_http_proxy(link);
            }
        }
    }
    None
}

fn base_from_url(url: &Url) -> (String, Option<u16>, String) {
    let host = url.host_str().unwrap_or("").to_string();
    let port = url.port();
    let name = url
        .fragment()
        .map(percent_decode)
        .filter(|s| !s.is_empty())
        .unwrap_or_default();
    (host, port, name)
}

fn query_map(url: &Url) -> Vec<(String, String)> {
    // Share links use URI query semantics, not form encoding: '+' is literal.
    url.query()
        .unwrap_or_default()
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (percent_decode(key), percent_decode(value))
        })
        .collect()
}

fn qget<'a>(q: &'a [(String, String)], key: &str) -> Option<&'a str> {
    q.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

fn parse_ss(link: &str) -> Option<ImportedProfile> {
    // SIP002 / 2022: ss://method:pass@host:port#name
    // v2rayN: ss://base64(method:pass@host:port)#name
    let after = link
        .strip_prefix("ss://")
        .or_else(|| link.strip_prefix("SS://"))?;
    let (main, frag) = match after.split_once('#') {
        Some((m, f)) => (m, Some(percent_decode(f))),
        None => (after, None),
    };

    let reconstructed = if main.contains('@') {
        format!("ss://{main}")
    } else {
        let decoded = decode_b64_flexible(main)?;
        // decoded is method:pass@host:port or similar without scheme
        format!("ss://{decoded}")
    };

    let url = Url::parse(&reconstructed).ok()?;
    let (host, port, frag_name) = base_from_url(&url);
    let name = frag.unwrap_or(frag_name);

    let (method, password) = if url.password().is_none() || url.password() == Some("") {
        // method:password in username (often base64)
        let user = percent_decode(url.username());
        let decoded = decode_b64_flexible(&user).unwrap_or(user);
        let (m, p) = decoded.split_once(':')?;
        (m.to_string(), p.to_string())
    } else {
        (
            percent_decode(url.username()),
            percent_decode(url.password().unwrap_or("")),
        )
    };

    if host.is_empty() || method.is_empty() || password.is_empty() {
        return None;
    }

    let q = query_map(&url);
    let mut outbound = ParsedOutbound {
        server: Some(host),
        server_port: port.or(Some(8388)),
        method: Some(method),
        password: Some(password),
        plugin: qget(&q, "plugin").map(|s| s.replace("simple-obfs;", "obfs-local;")),
        ..Default::default()
    };
    if let Some(plugin) = outbound.plugin.clone() {
        if let Some((p, opts)) = plugin.split_once(';') {
            outbound.plugin = Some(p.to_string());
            outbound.plugin_opts = Some(opts.to_string());
        }
    }
    if let Some(opts) = qget(&q, "plugin-opts") {
        outbound.plugin_opts = Some(opts.to_string());
    }

    Some(finish(
        name_or("Shadowsocks", &name),
        ProfileType::Shadowsocks,
        outbound,
        link,
    ))
}

fn parse_vmess(link: &str) -> Option<ImportedProfile> {
    let payload = link
        .strip_prefix("vmess://")
        .or_else(|| link.strip_prefix("VMESS://"))?;

    // V2RayN JSON base64
    if let Some(json_text) = decode_b64_flexible(payload.split('#').next().unwrap_or(payload)) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_text) {
            let uuid = v.get("id")?.as_str()?.to_string();
            let server = v.get("add")?.as_str()?.to_string();
            let port = v
                .get("port")
                .and_then(|p| {
                    p.as_u64()
                        .map(|n| n as u16)
                        .or_else(|| p.as_str().and_then(|s| s.parse().ok()))
                })
                .unwrap_or(443);
            let name = v
                .get("ps")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let alter_id = v
                .get("aid")
                .and_then(|a| a.as_i64().or_else(|| a.as_str()?.parse().ok()))
                .unwrap_or(0) as i32;
            let mut net = v
                .get("net")
                .and_then(|x| x.as_str())
                .unwrap_or("tcp")
                .to_string();
            if net == "h2" {
                net = "http".into();
            }
            let tls_on = v
                .get("tls")
                .and_then(|x| x.as_str())
                .is_some_and(|t| t.eq_ignore_ascii_case("tls"));
            let outbound = ParsedOutbound {
                server: Some(server),
                server_port: Some(port),
                uuid: Some(uuid),
                security: v.get("scy").and_then(|x| x.as_str()).map(|s| s.to_string()),
                alter_id: Some(alter_id),
                transport: Some(net),
                host: v
                    .get("host")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string()),
                path: v
                    .get("path")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string()),
                tls: Some(tls_on),
                sni: v.get("sni").and_then(|x| x.as_str()).map(|s| s.to_string()),
                ..Default::default()
            };
            return Some(finish(
                name_or("VMess", &name),
                ProfileType::Vmess,
                outbound,
                link,
            ));
        }
    }

    // Standard URL form
    let url = Url::parse(link).ok()?;
    let (host, port, name) = base_from_url(&url);
    let q = query_map(&url);
    let uuid = percent_decode(url.username());
    if host.is_empty() || uuid.is_empty() {
        return None;
    }
    let outbound = ParsedOutbound {
        server: Some(host),
        server_port: port.or(Some(443)),
        uuid: Some(uuid),
        security: Some(qget(&q, "encryption").unwrap_or("auto").to_string()),
        transport: qget(&q, "type").map(|s| s.to_string()),
        host: qget(&q, "host").map(|s| s.to_string()),
        path: qget(&q, "path").map(|s| s.to_string()),
        tls: Some(qget(&q, "security").is_some_and(|s| s.eq_ignore_ascii_case("tls"))),
        sni: qget(&q, "sni").map(|s| s.to_string()),
        alter_id: qget(&q, "alterId").and_then(|s| s.parse().ok()),
        ..Default::default()
    };
    Some(finish(
        name_or("VMess", &name),
        ProfileType::Vmess,
        outbound,
        link,
    ))
}

fn parse_vless(link: &str) -> Option<ImportedProfile> {
    let url = Url::parse(link).ok()?;
    let (host, port, name) = base_from_url(&url);
    let q = query_map(&url);
    let uuid = percent_decode(url.username());
    if host.is_empty() || uuid.is_empty() {
        return None;
    }
    let security = qget(&q, "security").unwrap_or("");
    let outbound = ParsedOutbound {
        server: Some(host),
        server_port: port.or(Some(443)),
        uuid: Some(uuid),
        flow: qget(&q, "flow").map(|s| s.to_string()),
        packet_encoding: Some(qget(&q, "packetEncoding").unwrap_or("xudp").to_string()),
        transport: qget(&q, "type").map(|s| s.to_string()),
        host: qget(&q, "host").map(|s| s.to_string()),
        path: qget(&q, "path").map(|s| s.to_string()),
        service_name: qget(&q, "serviceName").map(|s| s.to_string()),
        tls: Some(security.eq_ignore_ascii_case("tls") || security.eq_ignore_ascii_case("reality")),
        sni: qget(&q, "sni").map(|s| s.to_string()),
        alpn: qget(&q, "alpn").map(|s| s.to_string()),
        fp: qget(&q, "fp").map(|s| s.to_string()),
        pbk: qget(&q, "pbk").map(|s| s.to_string()),
        sid: qget(&q, "sid").map(|s| s.to_string()),
        spx: qget(&q, "spx").map(|s| s.to_string()),
        security: Some(security.to_string()),
        finalmask: qget(&q, "fm")
            .or_else(|| qget(&q, "finalmask"))
            .map(|s| s.to_string()),
        ..Default::default()
    };
    // XDrive has no share-link format; importing it as TCP loses storage credentials.
    if outbound
        .transport
        .as_deref()
        .is_some_and(|t| t.eq_ignore_ascii_case("xdrive"))
    {
        return None;
    }
    if outbound
        .transport
        .as_deref()
        .is_some_and(|t| matches!(t, "xhttp" | "splithttp"))
    {
        return crate::tls::xhttp_profile(&name_or("VLESS", &name), &outbound, &q, link);
    }
    Some(finish(
        name_or("VLESS", &name),
        ProfileType::Vless,
        outbound,
        link,
    ))
}

fn parse_trojan(link: &str) -> Option<ImportedProfile> {
    let url = Url::parse(link).ok()?;
    let (host, port, name) = base_from_url(&url);
    let q = query_map(&url);
    let password = if url.password().is_some() {
        // trojan://password@host — password may be in username when no user:pass form
        percent_decode(url.username())
    } else {
        percent_decode(url.username())
    };
    if host.is_empty() || password.is_empty() {
        return None;
    }
    let security = qget(&q, "security").unwrap_or("tls");
    let outbound = ParsedOutbound {
        server: Some(host),
        server_port: port.or(Some(443)),
        password: Some(password),
        transport: qget(&q, "type").map(|s| s.to_string()),
        host: qget(&q, "host").map(|s| s.to_string()),
        path: qget(&q, "path").map(|s| s.to_string()),
        tls: Some(!security.eq_ignore_ascii_case("none")),
        sni: qget(&q, "sni").map(|s| s.to_string()),
        alpn: qget(&q, "alpn").map(|s| s.to_string()),
        fp: qget(&q, "fp").map(|s| s.to_string()),
        security: Some(security.to_string()),
        ..Default::default()
    };
    Some(finish(
        name_or("Trojan", &name),
        ProfileType::Trojan,
        outbound,
        link,
    ))
}

fn parse_hysteria(link: &str) -> Option<ImportedProfile> {
    let url = Url::parse(link).ok()?;
    let (host, port, name) = base_from_url(&url);
    let q = query_map(&url);
    if host.is_empty() {
        return None;
    }
    let is_v1 = link.to_ascii_lowercase().starts_with("hysteria://")
        && !link.to_ascii_lowercase().starts_with("hysteria2://");
    let auth = url
        .password()
        .map(percent_decode)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let u = percent_decode(url.username());
            if u.is_empty() {
                None
            } else {
                Some(u)
            }
        })
        .or_else(|| qget(&q, "auth").map(|s| s.to_string()))
        .or_else(|| qget(&q, "password").map(|s| s.to_string()));

    let obfs = qget(&q, "obfs-password")
        .or_else(|| qget(&q, "obfsParam"))
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let (obfs_type, obfs_min_packet_size, obfs_max_packet_size) = if is_v1 {
        (None, None, None)
    } else {
        let obfs_type = if let Some(raw) = qget(&q, "obfs") {
            Some(hysteria_obfs_type_token(raw))
        } else if qget(&q, "min_packet_size").is_some() || qget(&q, "max_packet_size").is_some() {
            Some("gecko".into())
        } else {
            None
        };
        (
            obfs_type,
            query_packet_size(&q, "min_packet_size"),
            query_packet_size(&q, "max_packet_size"),
        )
    };
    let outbound = ParsedOutbound {
        server: Some(host),
        server_port: port.or(Some(443)),
        password: auth,
        obfs,
        obfs_type,
        obfs_min_packet_size,
        obfs_max_packet_size,
        up_mbps: qget(&q, "upmbps").and_then(positive_bandwidth),
        down_mbps: qget(&q, "downmbps").and_then(positive_bandwidth),
        tls: Some(true),
        sni: qget(&q, "sni").map(|s| s.to_string()),
        insecure: qget(&q, "insecure").map(|s| s == "1" || s.eq_ignore_ascii_case("true")),
        ..Default::default()
    };
    let ty = if is_v1 {
        ProfileType::Hysteria
    } else {
        ProfileType::Hysteria2
    };
    Some(finish(name_or("Hysteria", &name), ty, outbound, link))
}

fn parse_tuic(link: &str) -> Option<ImportedProfile> {
    let url = Url::parse(link).ok()?;
    let (host, port, name) = base_from_url(&url);
    let q = query_map(&url);
    let uuid = percent_decode(url.username());
    let password = url.password().map(percent_decode).unwrap_or_default();
    if host.is_empty() || uuid.is_empty() {
        return None;
    }
    let outbound = ParsedOutbound {
        server: Some(host),
        server_port: port.or(Some(443)),
        uuid: Some(uuid),
        password: if password.is_empty() {
            None
        } else {
            Some(password)
        },
        congestion_control: qget(&q, "congestion_control").map(|s| s.to_string()),
        udp_relay_mode: qget(&q, "udp_relay_mode").map(|s| s.to_string()),
        tls: Some(true),
        sni: qget(&q, "sni").map(|s| s.to_string()),
        alpn: qget(&q, "alpn").map(|s| s.to_string()),
        ..Default::default()
    };
    Some(finish(
        name_or("TUIC", &name),
        ProfileType::Tuic,
        outbound,
        link,
    ))
}

fn parse_socks(link: &str) -> Option<ImportedProfile> {
    let url = Url::parse(link).ok()?;
    let (host, port, name) = base_from_url(&url);
    if host.is_empty() {
        return None;
    }
    let outbound = ParsedOutbound {
        server: Some(host),
        server_port: port.or(Some(1080)),
        username: if url.username().is_empty() {
            None
        } else {
            Some(percent_decode(url.username()))
        },
        password: url.password().map(percent_decode),
        ..Default::default()
    };
    Some(finish(
        name_or("SOCKS", &name),
        ProfileType::Socks,
        outbound,
        link,
    ))
}

fn parse_http_proxy(link: &str) -> Option<ImportedProfile> {
    let url = Url::parse(link).ok()?;
    let (host, port, name) = base_from_url(&url);
    if host.is_empty() {
        return None;
    }
    let outbound = ParsedOutbound {
        server: Some(host),
        server_port: port.or(Some(8080)),
        username: Some(percent_decode(url.username())),
        password: url.password().map(percent_decode),
        tls: Some(url.scheme() == "https"),
        ..Default::default()
    };
    Some(finish(
        name_or("HTTP", &name),
        ProfileType::Http,
        outbound,
        link,
    ))
}

fn parse_wireguard_link(link: &str) -> Option<ImportedProfile> {
    if link.contains("[Interface]") && link.contains("[Peer]") {
        return crate::parse_wireguard_file(link);
    }
    let url = Url::parse(link).ok()?;
    let q = query_map(&url);
    let (host, port, name) = base_from_url(&url);
    let private_key = qget(&q, "private_key")
        .or_else(|| qget(&q, "privatekey"))
        .map(|s| s.to_string())
        .or_else(|| {
            let u = percent_decode(url.username());
            if u.is_empty() {
                None
            } else {
                Some(u)
            }
        })?;
    let public_key = qget(&q, "public_key")
        .or_else(|| qget(&q, "publickey"))
        .or_else(|| qget(&q, "peer_public_key"))
        .map(|s| s.to_string())?;
    let local_addr = qget(&q, "local_address")
        .or_else(|| qget(&q, "address"))
        .or_else(|| qget(&q, "ip"))
        .map(|s| s.replace('-', ",").replace(' ', ""));
    let server = if host.is_empty() { None } else { Some(host) };
    let server_port = port.or(Some(51820));
    if server.as_ref().is_none_or(|s| s.is_empty()) {
        return None;
    }
    let outbound = ParsedOutbound {
        server,
        server_port,
        password: Some(private_key),
        username: Some(public_key),
        path: local_addr,
        ..Default::default()
    };
    let mut profile = finish(
        name_or("WireGuard", &name),
        ProfileType::Wireguard,
        outbound,
        link,
    );
    crate::wireguard::apply_options(&mut profile, &q);
    Some(profile)
}

fn parse_snell(link: &str) -> Option<ImportedProfile> {
    let url = Url::parse(link).ok()?;
    let (host, port, name) = base_from_url(&url);
    if host.is_empty() {
        return None;
    }
    let q = query_map(&url);
    let psk = percent_decode(url.username());
    let psk = if psk.is_empty() {
        qget(&q, "psk").unwrap_or("").to_string()
    } else {
        psk
    };
    let version = qget(&q, "version")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(4);
    if version != 4 && version != 6 {
        return None;
    }
    let outbound = ParsedOutbound {
        server: Some(host),
        server_port: port.or(Some(440)),
        password: Some(psk),
        method: Some(version.to_string()),
        ..Default::default()
    };
    Some(finish(
        name_or("Snell", &name),
        ProfileType::Snell,
        outbound,
        link,
    ))
}

fn name_or(fallback: &str, name: &str) -> String {
    if name.is_empty() {
        fallback.to_string()
    } else {
        name.to_string()
    }
}

fn finish(
    name: String,
    profile_type: ProfileType,
    mut outbound: ParsedOutbound,
    source: &str,
) -> ImportedProfile {
    outbound.tag = Some(name.clone());
    // Upstream ExportToJson shape (must include `"type"` for Qt ParseFromJson).
    let mut raw: serde_json::Value = serde_json::from_str(&outbound.to_db_json(profile_type))
        .expect("serialized outbound is valid JSON");
    if let Ok(url) = Url::parse(source) {
        crate::tls::apply_link_tls(&mut raw, &query_map(&url));
    }
    outbound.raw_json = Some(raw.to_string());
    ImportedProfile {
        name,
        profile_type,
        outbound,
        source: source.to_string(),
    }
}

fn positive_bandwidth(text: &str) -> Option<u32> {
    let mbps = clash_bandwidth_to_mbps(text);
    (mbps > 0).then_some(mbps)
}

/// Upstream `hysteriaObfsType`: `none`, `plain`, and blank turn obfuscation off.
fn hysteria_obfs_type_token(raw: &str) -> String {
    let kind = raw.trim().to_ascii_lowercase();
    if kind.is_empty() || kind == "none" || kind == "plain" {
        String::new()
    } else {
        kind
    }
}

fn query_packet_size(q: &[(String, String)], key: &str) -> Option<u32> {
    let text = qget(q, key)?.trim();
    if text.is_empty() {
        return None;
    }
    text.parse().ok()
}

/// Upstream `clashBandwidthToMbps`.
///
/// A bare number is already Mbps. A prefix or a `b`/`B` unit scales it:
/// an empty prefix is 1e-6, `K` 1e-3, `M` 1, `G` 1e3, `T` 1e6, and `B` multiplies
/// by 8. The trailing `ps` alternative is not a unit, so `100ps` stays 100.
/// Positive results are rounded and at least 1. Anything else is 0.
fn clash_bandwidth_to_mbps(text: &str) -> u32 {
    let bytes = text.trim().as_bytes();
    let mut index = 0;
    if index >= bytes.len() || !bytes[index].is_ascii_digit() {
        return 0;
    }
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        let fraction = index + 1;
        if fraction < bytes.len() && bytes[fraction].is_ascii_digit() {
            index = fraction + 1;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
        }
    }
    let Ok(number_text) = std::str::from_utf8(&bytes[..index]) else {
        return 0;
    };
    let Ok(number) = number_text.parse::<f64>() else {
        return 0;
    };
    if !number.is_finite() {
        return 0;
    }
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    let mut prefix = None;
    if index < bytes.len() && matches!(bytes[index].to_ascii_lowercase(), b'k' | b'm' | b'g' | b't')
    {
        prefix = Some(bytes[index].to_ascii_lowercase());
        index += 1;
    }
    let mut unit_is_byte = false;
    let mut saw_unit = false;
    if bytes.get(index) == Some(&b'b') || bytes.get(index) == Some(&b'B') {
        saw_unit = true;
        unit_is_byte = bytes[index] == b'B';
        index += 1;
        if bytes.get(index..index + 2) == Some(b"ps") || bytes.get(index..index + 2) == Some(b"/s")
        {
            index += 2;
        }
    } else if bytes.get(index..index + 2) == Some(b"ps") {
        index += 2;
    }
    if index != bytes.len() {
        return 0;
    }
    let mut mbps = number;
    if prefix.is_some() || saw_unit {
        let scale = match prefix {
            None => 1e-6,
            Some(b'k') => 1e-3,
            Some(b'm') => 1.0,
            Some(b'g') => 1e3,
            Some(b't') => 1e6,
            Some(_) => return 0,
        };
        let scale = if unit_is_byte { scale * 8.0 } else { scale };
        mbps = number * scale;
    }
    if !(mbps > 0.0) || !mbps.is_finite() {
        return 0;
    }
    let rounded = mbps.min(i32::MAX as f64).round();
    if rounded < 1.0 {
        1
    } else {
        rounded as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clash_bandwidth_matches_upstream_units() {
        assert_eq!(clash_bandwidth_to_mbps("100"), 100);
        assert_eq!(clash_bandwidth_to_mbps("100 Mbps"), 100);
        assert_eq!(clash_bandwidth_to_mbps("50 MB/s"), 400);
        assert_eq!(clash_bandwidth_to_mbps("1 Gbps"), 1000);
        assert_eq!(clash_bandwidth_to_mbps("nope"), 0);
        assert_eq!(clash_bandwidth_to_mbps("100kbps"), 1);
        assert_eq!(clash_bandwidth_to_mbps("100ps"), 100);
    }

    #[test]
    fn hysteria2_none_keeps_password_and_disables_obfs() {
        let profile =
            parse_share_link("hysteria2://pw@h.example:443?obfs=none&obfs-password=keep#n")
                .expect("hysteria2 link");
        assert_eq!(profile.profile_type, ProfileType::Hysteria2);
        assert_eq!(profile.outbound.obfs.as_deref(), Some("keep"));
        assert_eq!(profile.outbound.obfs_type.as_deref(), Some(""));
        assert!(profile.outbound.hysteria2_obfs_value().is_none());
    }

    #[test]
    fn hysteria_v1_parses_bandwidth_strings() {
        let profile = parse_share_link(
            "hysteria://h.example:443?auth=tok&upmbps=100%20Mbps&downmbps=50%20MB%2Fs",
        )
        .expect("hysteria link");
        assert_eq!(profile.profile_type, ProfileType::Hysteria);
        assert_eq!(profile.outbound.password.as_deref(), Some("tok"));
        assert_eq!(profile.outbound.up_mbps, Some(100));
        assert_eq!(profile.outbound.down_mbps, Some(400));
    }
}
