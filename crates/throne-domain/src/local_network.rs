//! LAN inbound helpers (upstream `LocalNetwork`, 1.3.0-beta.2).
//!
//! Used to show a dialable address when mixed inbound is a wildcard bind, and
//! to hide this machine's own addresses in the Connections Source column.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const LAN_TTL: Duration = Duration::from_secs(30);

struct LanCache {
    at: Option<Instant>,
    addr: Option<String>,
}

static LAN: Mutex<LanCache> = Mutex::new(LanCache {
    at: None,
    addr: None,
});

/// True while mixed inbound is bound somewhere other than loopback.
pub fn lan_inbound_enabled(inbound_address: &str) -> bool {
    match parse_inbound_ip(inbound_address) {
        Some(ip) => !ip.is_loopback(),
        None => false,
    }
}

/// True only for a wildcard bind (`0.0.0.0` / `::`).
pub fn lan_inbound_is_wildcard(inbound_address: &str) -> bool {
    matches!(
        parse_inbound_ip(inbound_address),
        Some(IpAddr::V4(a)) if a.is_unspecified()
    ) || matches!(
        parse_inbound_ip(inbound_address),
        Some(IpAddr::V6(a)) if a.is_unspecified()
    )
}

/// Address of the default route's interface; empty when none can be determined.
pub fn lan_address() -> Option<String> {
    let mut g = LAN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(at) = g.at {
        if at.elapsed() < LAN_TTL {
            return g.addr.clone();
        }
    }
    let addr = probe_lan_address();
    g.at = Some(Instant::now());
    g.addr = addr.clone();
    addr
}

/// True for loopback and for this machine's current LAN address.
pub fn is_own_address(host: &str) -> bool {
    let bare = strip_host(&endpoint_host(host));
    if bare.is_empty() {
        return false;
    }
    let Ok(ip) = bare.parse::<IpAddr>() else {
        return false;
    };
    if ip.is_loopback() {
        return true;
    }
    lan_address()
        .as_deref()
        .is_some_and(|lan| lan == ip.to_string())
}

/// Host part of `"ip:port"` / `"[ip]:port"`. Does not split bare IPv6.
pub fn endpoint_host(endpoint: &str) -> String {
    let s = endpoint.trim();
    if let Some(rest) = s.strip_prefix('[') {
        return rest
            .split_once(']')
            .map(|(h, _)| h.to_string())
            .unwrap_or_else(|| rest.to_string());
    }
    if s.parse::<IpAddr>().is_ok() {
        return s.to_string();
    }
    match s.rsplit_once(':') {
        Some((h, p))
            if !h.is_empty() && !h.contains(':') && p.chars().all(|c| c.is_ascii_digit()) =>
        {
            h.to_string()
        }
        _ => s.to_string(),
    }
}

fn strip_host(host: &str) -> String {
    host.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string()
}

fn parse_inbound_ip(addr: &str) -> Option<IpAddr> {
    let raw = addr.trim();
    if raw.is_empty() {
        return Some(IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
    raw.parse().ok()
}

fn probe_lan_address() -> Option<String> {
    let sock = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))).ok()?;
    sock.connect(SocketAddr::from((Ipv4Addr::new(8, 8, 8, 8), 80)))
        .ok()?;
    let ip = sock.local_addr().ok()?.ip();
    if ip.is_loopback() || ip.is_unspecified() {
        // IPv6-only fallback
        let sock6 = UdpSocket::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))).ok()?;
        sock6
            .connect(SocketAddr::from((
                Ipv6Addr::new(0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888),
                80,
            )))
            .ok()?;
        let ip6 = sock6.local_addr().ok()?.ip();
        if ip6.is_loopback() || ip6.is_unspecified() {
            return None;
        }
        return Some(ip6.to_string());
    }
    Some(ip.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_and_loopback_classification() {
        assert!(lan_inbound_is_wildcard("::"));
        assert!(lan_inbound_is_wildcard("0.0.0.0"));
        assert!(!lan_inbound_is_wildcard("127.0.0.1"));
        assert!(!lan_inbound_is_wildcard("192.168.1.8"));

        assert!(!lan_inbound_enabled("127.0.0.1"));
        assert!(!lan_inbound_enabled("::1"));
        assert!(lan_inbound_enabled("::"));
        assert!(lan_inbound_enabled("0.0.0.0"));
        assert!(lan_inbound_enabled("192.168.1.8"));
    }

    #[test]
    fn endpoint_host_strips_port_and_brackets() {
        assert_eq!(endpoint_host("10.0.0.2:443"), "10.0.0.2");
        assert_eq!(endpoint_host("[2001:db8::1]:443"), "2001:db8::1");
        assert_eq!(endpoint_host("10.0.0.2"), "10.0.0.2");
        assert_eq!(endpoint_host("::1"), "::1");
        assert_eq!(endpoint_host("2001:db8::1"), "2001:db8::1");
    }

    #[test]
    fn own_address_treats_loopback_as_local() {
        assert!(is_own_address("127.0.0.1"));
        assert!(is_own_address("127.0.0.1:1234"));
        assert!(is_own_address("[::1]:9"));
        assert!(is_own_address("::1"));
        assert!(!is_own_address("8.8.8.8"));
    }
}
