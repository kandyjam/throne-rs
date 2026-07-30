use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Stable numeric id used across UI and persistence (matches legacy SQLite ids).
pub type ProfileId = i64;
pub type GroupId = i64;

/// Outbound protocol kinds supported by the original Throne client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProfileType {
    #[default]
    Socks,
    Http,
    Shadowsocks,
    Vmess,
    Vless,
    Trojan,
    Hysteria,
    Hysteria2,
    Tuic,
    Wireguard,
    AnyTls,
    Mieru,
    Naive,
    Juicity,
    TrustTunnel,
    ShadowTls,
    Ssh,
    XrayVless,
    Chain,
    Custom,
    Direct,
    Tailscale,
    ExtraCore,
}

impl ProfileType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Socks => "socks",
            Self::Http => "http",
            Self::Shadowsocks => "shadowsocks",
            Self::Vmess => "vmess",
            Self::Vless => "vless",
            Self::Trojan => "trojan",
            Self::Hysteria => "hysteria",
            Self::Hysteria2 => "hysteria2",
            Self::Tuic => "tuic",
            Self::Wireguard => "wireguard",
            Self::AnyTls => "anytls",
            Self::Mieru => "mieru",
            Self::Naive => "naive",
            Self::Juicity => "juicity",
            Self::TrustTunnel => "trusttunnel",
            Self::ShadowTls => "shadowtls",
            Self::Ssh => "ssh",
            Self::XrayVless => "xray_vless",
            Self::Chain => "chain",
            Self::Custom => "custom",
            Self::Direct => "direct",
            Self::Tailscale => "tailscale",
            Self::ExtraCore => "extra_core",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Socks => "SOCKS",
            Self::Http => "HTTP",
            Self::Shadowsocks => "Shadowsocks",
            Self::Vmess => "VMess",
            Self::Vless => "VLESS",
            Self::Trojan => "Trojan",
            Self::Hysteria => "Hysteria",
            Self::Hysteria2 => "Hysteria2",
            Self::Tuic => "TUIC",
            Self::Wireguard => "WireGuard",
            Self::AnyTls => "AnyTLS",
            Self::Mieru => "Mieru",
            Self::Naive => "Naïve",
            Self::Juicity => "Juicity",
            Self::TrustTunnel => "TrustTunnel",
            Self::ShadowTls => "ShadowTLS",
            Self::Ssh => "SSH",
            Self::XrayVless => "Xray VLESS",
            Self::Chain => "Chain",
            Self::Custom => "Custom",
            Self::Direct => "Direct",
            Self::Tailscale => "Tailscale",
            Self::ExtraCore => "Extra Core",
        }
    }
}

/// A proxy profile (node) belonging to a group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: ProfileId,
    pub group_id: GroupId,
    pub name: String,
    pub profile_type: ProfileType,
    /// Latency in milliseconds; 0 = untested, negative = failed.
    pub latency_ms: i32,
    pub download_speed: String,
    pub upload_speed: String,
    pub test_country: String,
    pub traffic_downlink: i64,
    pub traffic_uplink: i64,
    pub ip_out: String,
    /// Opaque outbound JSON / sing-box fragment (filled by importers later).
    pub outbound_json: String,
}

impl Profile {
    pub fn new(id: ProfileId, group_id: GroupId, name: impl Into<String>, ty: ProfileType) -> Self {
        Self {
            id,
            group_id,
            name: name.into(),
            profile_type: ty,
            latency_ms: 0,
            download_speed: String::new(),
            upload_speed: String::new(),
            test_country: String::new(),
            traffic_downlink: 0,
            traffic_uplink: 0,
            ip_out: String::new(),
            outbound_json: String::new(),
        }
    }

    pub fn display_latency(&self) -> String {
        match self.latency_ms {
            0 => "—".into(),
            n if n < 0 => "fail".into(),
            n => format!("{n} ms"),
        }
    }

    pub fn display_traffic(&self) -> String {
        format!(
            "↓ {}  ↑ {}",
            human_bytes(self.traffic_downlink),
            human_bytes(self.traffic_uplink)
        )
    }
}

/// Subscription / manual group of profiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    pub url: String,
    pub info: String,
    pub archive: bool,
    pub skip_auto_update: bool,
    pub sub_last_update: Option<DateTime<Utc>>,
    /// Ordered profile ids in this group.
    pub profile_ids: Vec<ProfileId>,
}

impl Group {
    pub fn new(id: GroupId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            url: String::new(),
            info: String::new(),
            archive: false,
            skip_auto_update: false,
            sub_last_update: None,
            profile_ids: Vec::new(),
        }
    }
}

/// High-level proxy / VPN modes (system proxy vs TUN).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SystemMode {
    #[default]
    Off,
    SystemProxy,
    VpnTun,
}

/// Runtime connection state of the Go core.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CoreStatus {
    #[default]
    Stopped,
    Starting,
    Running {
        profile_id: ProfileId,
        profile_name: String,
    },
    Stopping,
    Error(String),
}

impl CoreStatus {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    pub fn label(&self) -> String {
        match self {
            Self::Stopped => "Stopped".into(),
            Self::Starting => "Starting…".into(),
            Self::Running { profile_name, .. } => format!("Running · {profile_name}"),
            Self::Stopping => "Stopping…".into(),
            Self::Error(e) => format!("Error · {e}"),
        }
    }
}

/// Live traffic counters from the core.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrafficSnapshot {
    pub proxy_up: i64,
    pub proxy_down: i64,
    pub direct_up: i64,
    pub direct_down: i64,
}

fn human_bytes(n: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n.max(0) as f64;
    let mut i = 0usize;
    while v >= 1024.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_display() {
        let mut p = Profile::new(1, 1, "a", ProfileType::Vless);
        assert_eq!(p.display_latency(), "—");
        p.latency_ms = 42;
        assert_eq!(p.display_latency(), "42 ms");
        p.latency_ms = -1;
        assert_eq!(p.display_latency(), "fail");
    }

    #[test]
    fn human_bytes_units() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KB");
    }
}
