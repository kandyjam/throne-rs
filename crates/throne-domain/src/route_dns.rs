//! Routing-profile DNS overrides introduced in upstream 1.4.0-beta.4.

use serde::{Deserialize, Serialize};

use crate::{AppSettings, RouteProfile};

/// The eight DNS preferences a routing profile may override.
/// Hosts, local resolver overrides and advanced cache settings stay global.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RouteDns {
    pub remote_dns: String,
    pub remote_dns_disable_ipv6: bool,
    pub direct_dns: String,
    pub direct_dns_disable_ipv6: bool,
    pub dns_final_out: String,
    pub enable_dns_routing: bool,
    pub fake_dns: bool,
    pub fakeip_disable_ipv6: bool,
}

impl Default for RouteDns {
    fn default() -> Self {
        Self {
            remote_dns: "https://8.8.8.8/dns-query".into(),
            remote_dns_disable_ipv6: false,
            direct_dns: "localhost".into(),
            direct_dns_disable_ipv6: false,
            dns_final_out: "remote".into(),
            enable_dns_routing: true,
            fake_dns: false,
            fakeip_disable_ipv6: false,
        }
    }
}

impl RouteDns {
    pub fn from_settings(s: &AppSettings) -> Self {
        Self {
            remote_dns: s.remote_dns.clone(),
            remote_dns_disable_ipv6: s.remote_dns_disable_ipv6
                || s.remote_dns_strategy == "ipv4_only",
            direct_dns: s.direct_dns.clone(),
            direct_dns_disable_ipv6: s.direct_dns_disable_ipv6
                || s.direct_dns_strategy == "ipv4_only",
            dns_final_out: s.dns_final_out.clone(),
            enable_dns_routing: s.enable_dns_routing,
            fake_dns: s.fake_dns,
            fakeip_disable_ipv6: s.fakeip_disable_ipv6,
        }
    }

    /// Apply only the route-owned preferences to an effective settings clone.
    pub fn apply_to_settings(&self, s: &mut AppSettings) {
        s.remote_dns = self.remote_dns.trim().into();
        s.remote_dns_disable_ipv6 = self.remote_dns_disable_ipv6;
        s.direct_dns = self.direct_dns.trim().into();
        s.direct_dns_disable_ipv6 = self.direct_dns_disable_ipv6;
        s.dns_final_out.clone_from(&self.dns_final_out);
        s.enable_dns_routing = self.enable_dns_routing;
        s.fake_dns = self.fake_dns;
        s.fakeip_disable_ipv6 = self.fakeip_disable_ipv6;
        // A route-owned IPv6 policy must not inherit the global legacy strategy.
        s.remote_dns_strategy.clear();
        s.direct_dns_strategy.clear();
        s.use_dns_object = false;
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_address(&self.remote_dns).map_err(|e| format!("Remote DNS: {e}"))?;
        validate_address(&self.direct_dns).map_err(|e| format!("Direct DNS: {e}"))?;
        if !matches!(self.dns_final_out.as_str(), "remote" | "direct") {
            return Err("Default DNS must be remote or direct".into());
        }
        Ok(())
    }
}

impl RouteProfile {
    pub fn active_dns(&self) -> Option<&RouteDns> {
        self.dns.as_ref().filter(|_| self.custom_dns)
    }

    /// Update downloaded rule content while preserving local DNS, endpoint and
    /// disabled-rule choices. Raw remote profiles are rejected by upstream too.
    pub fn apply_remote_content(&mut self, incoming: RouteProfile) -> Result<(), String> {
        if incoming.is_raw {
            return Err("Remote content cannot be a raw routing profile".into());
        }
        let disabled: std::collections::HashSet<_> = self
            .rules
            .iter()
            .filter(|r| r.disabled && r.rule_type != 13)
            .map(|r| r.name.clone())
            .collect();
        let endpoints: Vec<_> = self
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| r.rule_type == 13)
            .map(|(i, r)| (i, r.clone()))
            .collect();
        self.rules = incoming
            .rules
            .into_iter()
            .filter(|r| r.rule_type != 13)
            .map(|mut r| {
                if disabled.contains(&r.name) {
                    r.disabled = true;
                }
                r
            })
            .collect();
        for (index, rule) in endpoints {
            self.rules.insert(index.min(self.rules.len()), rule);
        }
        self.is_raw = false;
        self.raw_route.clear();
        self.default_outbound = incoming.default_outbound;
        self.remote_last_update = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default();
        if self.name.trim().is_empty() {
            self.name = incoming.name;
        }
        Ok(())
    }
}

fn validate_address(value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("DNS address is empty".into());
    }
    if value.to_ascii_lowercase().starts_with("local") {
        return Ok(());
    }
    if value.chars().any(char::is_whitespace) {
        return Err("DNS address contains whitespace".into());
    }
    let (scheme, rest) = value.split_once("://").unwrap_or(("udp", value));
    let scheme = scheme.to_ascii_lowercase();
    if scheme == "dhcp" {
        return if rest.is_empty() || rest.contains('/') {
            Err("DHCP requires an interface or auto".into())
        } else {
            Ok(())
        };
    }
    if !matches!(
        scheme.as_str(),
        "udp" | "tcp" | "tls" | "quic" | "https" | "h3"
    ) {
        return Err("Unsupported DNS transport".into());
    }
    let authority = if let Some((host, _path)) = rest.split_once('/') {
        if !matches!(scheme.as_str(), "https" | "h3") {
            return Err("Only HTTPS and H3 DNS accept a path".into());
        }
        host
    } else {
        rest
    };
    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let (host, tail) = bracketed
            .split_once(']')
            .ok_or("IPv6 closing bracket missing")?;
        validate_ipv6(host)?;
        let port = if tail.is_empty() {
            None
        } else {
            Some(tail.strip_prefix(':').ok_or("Invalid IPv6 port")?)
        };
        (host, port)
    } else if authority.matches(':').count() > 1 {
        validate_ipv6(authority)?;
        (authority, None)
    } else {
        let (host, port) = authority
            .split_once(':')
            .map_or((authority, None), |(h, p)| (h, Some(p)));
        if host.is_empty() {
            return Err("DNS server is missing".into());
        }
        if host.chars().all(|c| c.is_ascii_digit() || c == '.') {
            host.parse::<std::net::Ipv4Addr>()
                .map_err(|_| "Invalid IPv4 address")?;
        } else {
            let host = host.strip_suffix('.').unwrap_or(host);
            if host.len() > 253
                || host.split('.').any(|label| {
                    label.is_empty()
                        || label.len() > 63
                        || label.starts_with('-')
                        || label.ends_with('-')
                        || !label
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                })
            {
                return Err("Invalid DNS host name".into());
            }
        }
        (host, port)
    };
    if host.is_empty() {
        return Err("DNS server is missing".into());
    }
    if let Some(port) = port {
        if port.len() > 5
            || !port.bytes().all(|c| c.is_ascii_digit())
            || port.parse::<u16>().ok().filter(|p| *p > 0).is_none()
        {
            return Err("DNS port must be 1–65535".into());
        }
    }
    Ok(())
}

fn validate_ipv6(host: &str) -> Result<(), String> {
    let (ip, zone) = host
        .split_once('%')
        .map_or((host, None), |(ip, zone)| (ip, Some(zone)));
    if zone.is_some_and(|z| z.is_empty() || z.contains('%')) {
        return Err("Invalid IPv6 scope".into());
    }
    ip.parse::<std::net::Ipv6Addr>()
        .map_err(|_| "Invalid IPv6 address".to_string())?;
    Ok(())
}
