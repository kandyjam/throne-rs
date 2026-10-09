//! Scanner IP-list entities and network-only import parsing.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpListEntry {
    /// Masked CIDR, or a bare host for /32 and /128.
    pub cidr: String,
    pub port: u16,
    pub latency_ms: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum IpListRole {
    #[default]
    User = 0,
    ScanResult = 1,
    ScanSnapshot = 2,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum IpListSourceKind {
    #[default]
    Manual = 0,
    Url = 1,
    RuleSet = 2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpList {
    pub id: i64,
    pub name: String,
    pub related_test_id: i64,
    pub role: IpListRole,
    pub source_kind: IpListSourceKind,
    pub source: String,
    pub auto_update: bool,
    pub update_interval: i32,
    pub last_update: i64,
    pub last_error: String,
    pub entries: Vec<IpListEntry>,
    pub entry_count: usize,
    pub entries_loaded: bool,
}

impl Default for IpList {
    fn default() -> Self {
        Self::new("")
    }
}

impl IpList {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: -1,
            name: name.into(),
            related_test_id: -1,
            role: IpListRole::User,
            source_kind: IpListSourceKind::Manual,
            source: String::new(),
            auto_update: false,
            update_interval: 1440,
            last_update: 0,
            last_error: String::new(),
            entries: Vec::new(),
            entry_count: 0,
            entries_loaded: true,
        }
    }

    pub fn is_remote(&self) -> bool {
        self.source_kind != IpListSourceKind::Manual && !self.source.is_empty()
    }
    pub fn is_hidden(&self) -> bool {
        self.role == IpListRole::ScanSnapshot
    }
    pub fn update_due(&self, now: i64) -> bool {
        self.is_remote()
            && self.auto_update
            && now.saturating_sub(self.last_update) >= i64::from(self.update_interval.max(30)) * 60
    }
}

/// Normalize without expanding a subnet into its hosts, including IPv6 /0.
pub fn normalize_ip_cidr(text: &str) -> Option<String> {
    let text = text.trim();
    let (host, prefix) = match text.split_once('/') {
        Some((host, prefix)) => (host, Some(prefix.parse::<u32>().ok()?)),
        None => (text, None),
    };
    let host = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    match host.parse::<IpAddr>().ok()? {
        IpAddr::V4(ip) => {
            let bits = prefix.unwrap_or(32);
            if bits > 32 {
                return None;
            }
            let value = u32::from(ip)
                & if bits == 0 {
                    0
                } else {
                    u32::MAX << (32 - bits)
                };
            let ip = Ipv4Addr::from(value);
            Some(if bits == 32 {
                ip.to_string()
            } else {
                format!("{ip}/{bits}")
            })
        }
        IpAddr::V6(ip) => {
            let bits = prefix.unwrap_or(128);
            if bits > 128 {
                return None;
            }
            let value = u128::from(ip)
                & if bits == 0 {
                    0
                } else {
                    u128::MAX << (128 - bits)
                };
            let ip = Ipv6Addr::from(value);
            Some(if bits == 128 {
                ip.to_string()
            } else {
                format!("{ip}/{bits}")
            })
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IpListParseResult {
    pub entries: Vec<IpListEntry>,
    pub rejected: usize,
    pub duplicates: usize,
    pub samples: Vec<String>,
}

fn address_range(text: &str) -> Option<Vec<String>> {
    let (start, end) = text.split_once(['-', '–'])?;
    let start: IpAddr = start.trim().parse().ok()?;
    let end: IpAddr = end.trim().parse().ok()?;
    let (mut low, high, bits) = match (start, end) {
        (IpAddr::V4(a), IpAddr::V4(b)) => (u32::from(a) as u128, u32::from(b) as u128, 32_u32),
        (IpAddr::V6(a), IpAddr::V6(b)) => (u128::from(a), u128::from(b), 128_u32),
        _ => return None,
    };
    if low > high {
        return None;
    }
    let mut out = Vec::new();
    loop {
        let align = low.trailing_zeros().min(bits);
        let remaining = high - low;
        let capacity = if remaining == u128::MAX {
            128
        } else {
            127 - (remaining + 1).leading_zeros()
        };
        let block = align.min(capacity);
        let prefix = bits - block;
        let host = if bits == 32 {
            Ipv4Addr::from(low as u32).to_string()
        } else {
            Ipv6Addr::from(low).to_string()
        };
        out.push(if prefix == bits {
            host
        } else {
            format!("{host}/{prefix}")
        });
        if block == 128 {
            break;
        }
        let Some(next) = low.checked_add(1_u128 << block) else {
            break;
        };
        if next > high {
            break;
        }
        low = next;
    }
    Some(out)
}

fn address_token(text: &str) -> Option<(Vec<String>, Option<u16>)> {
    if let Some(cidr) = normalize_ip_cidr(text) {
        return Some((vec![cidr], None));
    }
    if let Some(range) = address_range(text) {
        return Some((range, None));
    }
    let (host, port) = if let Some(rest) = text.strip_prefix('[') {
        let (host, port) = rest.split_once("]:")?;
        (host, port)
    } else {
        let (host, port) = text.rsplit_once(':')?;
        // Unbracketed IPv6 is always an address, never an address + port.
        if host.contains(':') {
            return None;
        }
        (host, port)
    };
    let port = port.parse::<u16>().ok()?;
    Some((vec![normalize_ip_cidr(host)?], Some(port)))
}

fn looks_like_address(text: &str) -> bool {
    text.contains(':')
        || text.chars().next().is_some_and(|c| c.is_ascii_digit()) && text.contains('.')
}

fn reject(result: &mut IpListParseResult, text: &str) {
    result.rejected += 1;
    if result.samples.len() < 5 {
        result.samples.push(text.chars().take(64).collect());
    }
}

/// Import manual/URL text: IPs, CIDRs, inclusive ranges, host:port and
/// whitespace/CSV/TSV columns. Reject hostnames; preserve order and deduplicate.
pub fn parse_ip_list_text(text: &str, default_port: u16) -> IpListParseResult {
    let mut result = IpListParseResult::default();
    let mut seen = HashSet::new();
    let mut first = true;
    let mut port_column = None;
    let mut tabular_file = false;
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line
            .split('#')
            .next()
            .unwrap_or_default()
            .split("//")
            .next()
            .unwrap_or_default()
            .trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let joined = line.replace(" - ", "-").replace(" – ", "–");
        let separated = joined.contains([',', ';', '\t']);
        let fields: Vec<&str> = if separated {
            joined.split([',', ';', '\t']).collect()
        } else {
            joined.split_whitespace().collect()
        };
        let fields: Vec<&str> = fields
            .into_iter()
            .map(|s| s.trim().trim_matches(['"', '\'']))
            .collect();
        if first
            && fields.iter().any(|s| {
                matches!(
                    s.to_ascii_lowercase().as_str(),
                    "ip" | "address" | "cidr" | "endpoint"
                )
            })
            && !fields.iter().any(|s| address_token(s).is_some())
        {
            port_column = fields
                .iter()
                .position(|s| s.to_ascii_lowercase().contains("port"));
            first = false;
            tabular_file = true;
            continue;
        }
        first = false;
        let tabular = separated || tabular_file;
        let line_port = port_column
            .and_then(|i| fields.get(i))
            .and_then(|s| s.parse::<u16>().ok());
        let tokens: Vec<&str> = fields.iter().flat_map(|s| s.split_whitespace()).collect();
        let has_address = tokens.iter().any(|s| address_token(s).is_some());
        if !has_address {
            let address_shaped = tokens.iter().any(|s| looks_like_address(s));
            if tabular && !address_shaped {
                reject(&mut result, line);
            } else {
                for token in tokens {
                    if !tabular || looks_like_address(token) {
                        reject(&mut result, token);
                    }
                }
            }
            continue;
        }
        let mut consumed_port = None;
        for (index, token) in tokens.iter().enumerate() {
            if consumed_port == Some(index) {
                continue;
            }
            if let Some((cidrs, own_port)) = address_token(token) {
                let following_port = if tabular_file {
                    None
                } else {
                    tokens.get(index + 1).and_then(|s| s.parse::<u16>().ok())
                };
                if own_port.is_none() && following_port.is_some() {
                    consumed_port = Some(index + 1);
                }
                let port = own_port
                    .or(line_port)
                    .or(following_port)
                    .filter(|p| *p > 0)
                    .unwrap_or(default_port);
                for cidr in cidrs {
                    if seen.insert((cidr.clone(), port)) {
                        result.entries.push(IpListEntry {
                            cidr,
                            port,
                            latency_ms: 0,
                        });
                    } else {
                        result.duplicates += 1;
                    }
                }
            } else if !tabular || looks_like_address(token) {
                reject(&mut result, token);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_normalization_masks_hosts_without_expanding_ipv6() {
        assert_eq!(
            normalize_ip_cidr("192.0.2.19/24"),
            Some("192.0.2.0/24".into())
        );
        assert_eq!(
            normalize_ip_cidr("2001:db8:1234::1/32"),
            Some("2001:db8::/32".into())
        );
        assert_eq!(normalize_ip_cidr("::1/0"), Some("::/0".into()));
        for invalid in ["010.0.0.1", "example.org", "1.2.3", "1.2.3.4/33", "::1/129"] {
            assert_eq!(normalize_ip_cidr(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn text_import_preserves_ports_order_and_canonical_deduplication() {
        let parsed = parse_ip_list_text(
            "\u{feff}IP,Port,Country\n192.0.2.19/24,443,US\n192.0.2.0/24,443,US\n[2001:db8::1]:8443\ninvalid\n# comment",
            80,
        );
        assert_eq!(
            parsed.entries,
            vec![
                IpListEntry {
                    cidr: "192.0.2.0/24".into(),
                    port: 443,
                    latency_ms: 0
                },
                IpListEntry {
                    cidr: "2001:db8::1".into(),
                    port: 8443,
                    latency_ms: 0
                }
            ]
        );
        assert_eq!((parsed.rejected, parsed.duplicates), (1, 1));
    }

    #[test]
    fn ranges_are_compact_and_do_not_overflow_full_address_space() {
        let parsed = parse_ip_list_text(
            "192.0.2.1 - 192.0.2.6\n:: - ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff",
            0,
        );
        assert_eq!(
            parsed
                .entries
                .iter()
                .map(|e| e.cidr.as_str())
                .collect::<Vec<_>>(),
            [
                "192.0.2.1",
                "192.0.2.2/31",
                "192.0.2.4/31",
                "192.0.2.6",
                "::/0"
            ]
        );
        assert!(parse_ip_list_text("192.0.2.10-192.0.2.1", 0)
            .entries
            .is_empty());
    }

    #[test]
    fn tabular_latency_is_not_a_port_and_mixed_invalid_addresses_are_reported() {
        let result = parse_ip_list_text("ip,latency\n192.0.2.1,25\n192.0.2.2 999.1.2.3,30", 443);
        assert_eq!(
            result.entries.iter().map(|e| e.port).collect::<Vec<_>>(),
            [443, 443]
        );
        assert_eq!(result.rejected, 1);
        let result = parse_ip_list_text("192.0.2.1 8443 invalid 999.1.2.3", 443);
        assert_eq!(result.entries[0].port, 8443);
        assert_eq!(result.rejected, 2);
    }
}
