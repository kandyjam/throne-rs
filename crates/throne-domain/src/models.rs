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
            // Upstream OutboundFactory uses "xrayvless" (no underscore).
            Self::XrayVless => "xrayvless",
            Self::Chain => "chain",
            Self::Custom => "custom",
            Self::Direct => "direct",
            Self::Tailscale => "tailscale",
            Self::ExtraCore => "extracore",
        }
    }

    /// Parse upstream type strings (`hysteria2` → Hysteria bean type hysteria).
    pub fn from_upstream(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "socks" | "socks5" => Some(Self::Socks),
            "http" | "https" => Some(Self::Http),
            "shadowsocks" | "ss" => Some(Self::Shadowsocks),
            "vmess" => Some(Self::Vmess),
            "vless" => Some(Self::Vless),
            "trojan" => Some(Self::Trojan),
            "hysteria" => Some(Self::Hysteria),
            "hysteria2" | "hy2" => Some(Self::Hysteria2),
            "tuic" => Some(Self::Tuic),
            "wireguard" | "wg" => Some(Self::Wireguard),
            "anytls" => Some(Self::AnyTls),
            "mieru" => Some(Self::Mieru),
            "naive" | "naiveproxy" => Some(Self::Naive),
            "juicity" => Some(Self::Juicity),
            "trusttunnel" => Some(Self::TrustTunnel),
            "shadowtls" => Some(Self::ShadowTls),
            "ssh" => Some(Self::Ssh),
            "xrayvless" | "xray_vless" => Some(Self::XrayVless),
            "chain" => Some(Self::Chain),
            "custom" => Some(Self::Custom),
            "direct" => Some(Self::Direct),
            "tailscale" => Some(Self::Tailscale),
            "extracore" | "extra_core" => Some(Self::ExtraCore),
            _ => None,
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

/// Parsed outbound fields shared by importers (maps toward sing-box JSON later).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedOutbound {
    pub tag: Option<String>,
    pub server: Option<String>,
    pub server_port: Option<u16>,
    pub uuid: Option<String>,
    pub password: Option<String>,
    pub username: Option<String>,
    pub method: Option<String>,
    pub flow: Option<String>,
    pub security: Option<String>,
    pub alter_id: Option<i32>,
    pub transport: Option<String>,
    pub host: Option<String>,
    pub path: Option<String>,
    pub service_name: Option<String>,
    pub tls: Option<bool>,
    pub sni: Option<String>,
    pub alpn: Option<String>,
    pub fp: Option<String>,
    pub pbk: Option<String>,
    pub sid: Option<String>,
    pub spx: Option<String>,
    pub insecure: Option<bool>,
    pub plugin: Option<String>,
    pub plugin_opts: Option<String>,
    pub obfs: Option<String>,
    pub up_mbps: Option<u32>,
    pub down_mbps: Option<u32>,
    pub congestion_control: Option<String>,
    pub udp_relay_mode: Option<String>,
    pub packet_encoding: Option<String>,
    /// Full serialized snapshot for DB `outbound_json`.
    pub raw_json: Option<String>,
}

impl ParsedOutbound {
    pub fn to_db_json(&self) -> String {
        self.raw_json
            .clone()
            .unwrap_or_else(|| serde_json::to_string(self).unwrap_or_else(|_| "{}".into()))
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
    /// Structured outbound when available (import path).
    #[serde(default)]
    pub outbound: ParsedOutbound,
    /// Upstream "config security" flag (`cd7cb259`) — true if link looks insecure.
    #[serde(default)]
    pub insecure: bool,
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
            outbound: ParsedOutbound::default(),
            insecure: false,
        }
    }

    pub fn display_latency(&self) -> String {
        match self.latency_ms {
            0 => String::new(),
            n if n < 0 => "fail".into(),
            n => format!("{n} ms"),
        }
    }

    /// Upstream ColTestResult: latency (+ country code). Speeds live in Traffic.
    pub fn display_test_result(&self) -> String {
        let lat = self.display_latency();
        if lat.is_empty() {
            if self.test_country.is_empty() {
                String::new()
            } else {
                self.test_country.clone()
            }
        } else if self.test_country.is_empty() {
            lat
        } else {
            format!("{lat} · {}", self.test_country)
        }
    }

    /// Upstream ColTraffic: cumulative ↓uplink / ↑ — or last speed-test rates if no bytes yet.
    pub fn display_traffic(&self) -> String {
        if self.traffic_downlink != 0 || self.traffic_uplink != 0 {
            return format!(
                "↓{} ↑{}",
                human_bytes(self.traffic_downlink),
                human_bytes(self.traffic_uplink)
            );
        }
        // Fall back to speed-test strings when cumulative counters are empty.
        match (
            self.download_speed.is_empty(),
            self.upload_speed.is_empty(),
        ) {
            (true, true) => String::new(),
            (false, true) => format!("↓{}", self.download_speed),
            (true, false) => format!("↑{}", self.upload_speed),
            (false, false) => format!("↓{} ↑{}", self.download_speed, self.upload_speed),
        }
    }

    /// Upstream ColAddress: `host:port` from outbound.
    pub fn display_address(&self) -> String {
        let server = self
            .outbound
            .server
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                serde_json::from_str::<serde_json::Value>(&self.outbound_json)
                    .ok()
                    .and_then(|v| {
                        v.get("server")
                            .and_then(|x| x.as_str())
                            .map(|s| s.to_string())
                    })
            })
            .unwrap_or_default();
        let port = self.outbound.server_port.or_else(|| {
            serde_json::from_str::<serde_json::Value>(&self.outbound_json)
                .ok()
                .and_then(|v| v.get("server_port").and_then(|x| x.as_u64().map(|n| n as u16)))
        });
        match (server.is_empty(), port) {
            (true, _) => String::new(),
            (false, Some(p)) if p > 0 => format!("{server}:{p}"),
            (false, _) => server,
        }
    }

    /// Upstream ColType via outbound DisplayType.
    pub fn display_type(&self) -> String {
        self.profile_type.display_name().to_string()
    }
}

/// Subscription / manual group of profiles (columns match upstream `groups` table).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    pub url: String,
    pub info: String,
    pub archive: bool,
    pub skip_auto_update: bool,
    pub auto_clear_unavailable: bool,
    /// Epoch seconds (`sub_last_update` column).
    pub sub_last_update: i64,
    pub front_proxy_id: i64,
    pub landing_proxy_id: i64,
    pub column_width_json: String,
    /// Ordered profile ids (`profiles_json` column).
    pub profile_ids: Vec<ProfileId>,
    pub scroll_last_profile: i64,
    pub test_sort_by: i32,
    pub traffic_sort_by: i32,
    pub test_items_to_show: i32,
    pub type_sort_by: i32,
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
            auto_clear_unavailable: false,
            sub_last_update: 0,
            front_proxy_id: -1,
            landing_proxy_id: -1,
            column_width_json: String::new(),
            profile_ids: Vec::new(),
            scroll_last_profile: -1,
            test_sort_by: 0,
            traffic_sort_by: 0,
            test_items_to_show: 0,
            type_sort_by: 0,
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrafficSnapshot {
    pub proxy_up: i64,
    pub proxy_down: i64,
    pub direct_down: i64,
    pub direct_up: i64,
}

/// Rule-set CDN mirror — matches upstream `Configs::Mirrors`.
///
/// Default is Cloudflare testing CF (`testingcf.jsdelivr.net`), same as Qt Throne.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RulesetMirror {
    Github = 0,
    #[default]
    Cloudflare = 1,
    Gcore = 2,
    Quantil = 3,
    Fastly = 4,
    Cdn = 5,
}

impl RulesetMirror {
    pub fn from_id(id: i32) -> Self {
        match id {
            0 => Self::Github,
            2 => Self::Gcore,
            3 => Self::Quantil,
            4 => Self::Fastly,
            5 => Self::Cdn,
            _ => Self::Cloudflare,
        }
    }

    pub fn as_id(self) -> i32 {
        self as i32
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Github => "GitHub raw",
            Self::Cloudflare => "jsDelivr (Cloudflare)",
            Self::Gcore => "jsDelivr (Gcore)",
            Self::Quantil => "jsDelivr (Quantil)",
            Self::Fastly => "jsDelivr (Fastly)",
            Self::Cdn => "jsDelivr (cdn)",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Self::Github => Self::Cloudflare,
            Self::Cloudflare => Self::Gcore,
            Self::Gcore => Self::Quantil,
            Self::Quantil => Self::Fastly,
            Self::Fastly => Self::Cdn,
            Self::Cdn => Self::Github,
        }
    }

    /// jsDelivr `/gh` base host for this mirror (empty when using GitHub raw).
    pub fn jsdelivr_gh_base(self) -> Option<&'static str> {
        match self {
            Self::Github => None,
            Self::Cloudflare => Some("https://testingcf.jsdelivr.net/gh"),
            Self::Gcore => Some("https://gcore.jsdelivr.net/gh"),
            Self::Quantil => Some("https://quantil.jsdelivr.net/gh"),
            Self::Fastly => Some("https://fastly.jsdelivr.net/gh"),
            Self::Cdn => Some("https://cdn.jsdelivr.net/gh"),
        }
    }
}

fn default_vpn_tun_ipv4_cidr() -> String {
    "172.19.0.1/24".into()
}

/// Subset of upstream `SettingsRepo` defaults used by the Rust client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub inbound_socks_port: i32,
    pub inbound_address: String,
    pub test_latency_url: String,
    pub remote_dns: String,
    pub direct_dns: String,
    pub vpn_strict_route: bool,
    pub vpn_mtu: i32,
    /// Tun IPv4 address/prefix (upstream `vpn_tun_ipv4_cidr`, default `172.19.0.1/24`).
    /// Passed to sing-box `inbounds[].address` and Start RPC `tun_ipv4_cidr` (macOS system DNS).
    #[serde(default = "default_vpn_tun_ipv4_cidr")]
    pub vpn_tun_ipv4_cidr: String,
    pub disable_private_range_bypass: bool,
    pub sub_show_change_popup: bool,
    pub allow_stopping_active_profile: bool,
    pub show_config_security: bool,
    pub current_route_id: i64,
    pub remember_id: i64,
    pub system_proxy_enabled: bool,
    pub tun_mode_enabled: bool,
    /// Upstream `system_dns_set` checkbox on the main toolbar.
    pub system_dns_set: bool,
    pub theme: String,
    pub log_level: String,
    /// CDN for remote `.srs` rule-sets (`ruleset_mirror` in SettingsRepo).
    #[serde(default)]
    pub ruleset_mirror: RulesetMirror,
    /// Inject throne-adblocksingbox rule-set on Start when true.
    #[serde(default)]
    pub adblock_enable: bool,
    /// Hotkey chords (display + future binding). Empty = use built-in defaults.
    #[serde(default)]
    pub hk_start_stop: String,
    #[serde(default)]
    pub hk_import: String,
    #[serde(default)]
    pub hk_save: String,
    #[serde(default)]
    pub hk_url_test: String,
    #[serde(default)]
    pub hk_copy_logs: String,

    // ── Routing dialog (DialogManageRoutes) settings ─────────────────────
    #[serde(default)]
    pub remote_dns_strategy: String,
    #[serde(default)]
    pub direct_dns_strategy: String,
    #[serde(default = "default_dns_cache_capacity")]
    pub dns_cache_capacity: i32,
    #[serde(default)]
    pub dns_disable_cache: bool,
    #[serde(default)]
    pub dns_disable_expire: bool,
    #[serde(default)]
    pub dns_reverse_mapping: bool,
    #[serde(default = "default_true")]
    pub enable_dns_routing: bool,
    #[serde(default)]
    pub use_dns_object: bool,
    #[serde(default)]
    pub dns_object: String,
    #[serde(default = "default_dns_final_out")]
    pub dns_final_out: String,
    #[serde(default)]
    pub resolve_domain_strategy: String,
    #[serde(default)]
    pub default_domain_strategy: String,
    #[serde(default)]
    pub core_box_underlying_dns: String,
    #[serde(default)]
    pub fake_dns: bool,
    /// DNS hijack / embedded DNS server.
    #[serde(default)]
    pub enable_dns_server: bool,
    #[serde(default = "default_dns_listen_port")]
    pub dns_server_listen_port: i32,
    #[serde(default = "default_dns_v4_resp")]
    pub dns_v4_resp: String,
    #[serde(default = "default_dns_v6_resp")]
    pub dns_v6_resp: String,
    #[serde(default)]
    pub dns_server_rules: Vec<String>,
    #[serde(default)]
    pub dns_server_listen_lan: bool,
    /// Transparent redirect inbound.
    #[serde(default)]
    pub enable_redirect: bool,
    #[serde(default = "default_redirect_addr")]
    pub redirect_listen_address: String,
    #[serde(default = "default_redirect_port")]
    pub redirect_listen_port: i32,
    /// Cloudflare WARP egress (warp-bypass outbound).
    #[serde(default)]
    pub enable_warp: bool,
    #[serde(default)]
    pub warp_ep: String,
    #[serde(default)]
    pub warp_private_key: String,
    #[serde(default)]
    pub warp_public_key: String,
    #[serde(default)]
    pub warp_ifc_addrs: Vec<String>,
    #[serde(default)]
    pub warp_reserved: Vec<String>,
}

fn default_true() -> bool {
    true
}
fn default_dns_cache_capacity() -> i32 {
    65536
}
fn default_dns_final_out() -> String {
    "remote".into()
}
fn default_dns_listen_port() -> i32 {
    53
}
fn default_dns_v4_resp() -> String {
    "198.18.0.2".into()
}
fn default_dns_v6_resp() -> String {
    "fc00::2".into()
}
fn default_redirect_addr() -> String {
    "127.0.0.1".into()
}
fn default_redirect_port() -> i32 {
    12345
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            inbound_socks_port: 2080,
            inbound_address: "127.0.0.1".into(),
            test_latency_url: "https://www.gstatic.com/generate_204".into(),
            // upstream 56d0d9fd — Google DoH as default remote DNS
            remote_dns: "https://dns.google/dns-query".into(),
            direct_dns: "localhost".into(),
            vpn_strict_route: false,
            // upstream SettingsRepo default is 1500; keep 9000 only if user already persisted it
            vpn_mtu: 1500,
            vpn_tun_ipv4_cidr: default_vpn_tun_ipv4_cidr(),
            disable_private_range_bypass: false,
            sub_show_change_popup: true,
            allow_stopping_active_profile: true,
            show_config_security: true,
            current_route_id: -1,
            remember_id: -1,
            system_proxy_enabled: false,
            tun_mode_enabled: false,
            system_dns_set: false,
            // Upstream default after migration: follow OS light/dark.
            theme: "System".into(),
            log_level: "info".into(),
            // upstream default: Mirrors::CLOUDFLARE
            ruleset_mirror: RulesetMirror::Cloudflare,
            adblock_enable: false,
            hk_start_stop: "Cmd/Ctrl+R".into(),
            hk_import: "Cmd/Ctrl+V".into(),
            hk_save: "Cmd/Ctrl+S".into(),
            hk_url_test: "Cmd/Ctrl+T".into(),
            hk_copy_logs: "Cmd/Ctrl+Shift+C".into(),
            remote_dns_strategy: String::new(),
            direct_dns_strategy: String::new(),
            dns_cache_capacity: default_dns_cache_capacity(),
            dns_disable_cache: false,
            dns_disable_expire: false,
            dns_reverse_mapping: false,
            enable_dns_routing: true,
            use_dns_object: false,
            dns_object: String::new(),
            dns_final_out: default_dns_final_out(),
            resolve_domain_strategy: String::new(),
            default_domain_strategy: String::new(),
            core_box_underlying_dns: String::new(),
            fake_dns: false,
            enable_dns_server: false,
            dns_server_listen_port: default_dns_listen_port(),
            dns_v4_resp: default_dns_v4_resp(),
            dns_v6_resp: default_dns_v6_resp(),
            dns_server_rules: Vec::new(),
            dns_server_listen_lan: false,
            enable_redirect: false,
            redirect_listen_address: default_redirect_addr(),
            redirect_listen_port: default_redirect_port(),
            enable_warp: false,
            warp_ep: String::new(),
            warp_private_key: String::new(),
            warp_public_key: String::new(),
            warp_ifc_addrs: Vec::new(),
            warp_reserved: Vec::new(),
        }
    }
}

/// Predefined outbound ids used by upstream route rules (`RouteRule.h`).
/// Note: warp-bypass is **-5** (−4 is reserved for DNS hijack internally).
pub mod outbound_ids {
    pub const PROXY: i64 = -1;
    pub const DIRECT: i64 = -2;
    pub const BLOCK: i64 = -3;
    pub const WARP_BYPASS: i64 = -5;
}

/// Default outbound token as stored in share JSON (`proxy` / `direct` / …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DefaultOutbound {
    #[default]
    Proxy,
    Direct,
    Block,
    WarpBypass,
    /// Positive profile id when resolved.
    Profile(i64),
}

impl DefaultOutbound {
    pub fn as_id(self) -> i64 {
        match self {
            Self::Proxy => outbound_ids::PROXY,
            Self::Direct => outbound_ids::DIRECT,
            Self::Block => outbound_ids::BLOCK,
            Self::WarpBypass => outbound_ids::WARP_BYPASS,
            Self::Profile(id) => id,
        }
    }

    pub fn from_id(id: i64) -> Self {
        match id {
            outbound_ids::PROXY => Self::Proxy,
            outbound_ids::DIRECT => Self::Direct,
            outbound_ids::BLOCK => Self::Block,
            outbound_ids::WARP_BYPASS => Self::WarpBypass,
            other if other >= 0 => Self::Profile(other),
            _ => Self::Proxy,
        }
    }

    pub fn from_share_token(s: &str) -> Self {
        match s {
            "proxy" | "" => Self::Proxy,
            "direct" => Self::Direct,
            "block" => Self::Block,
            "warp-bypass" => Self::WarpBypass,
            other => other
                .parse::<i64>()
                .map(Self::Profile)
                .unwrap_or(Self::Proxy),
        }
    }

    pub fn to_share_token(self) -> String {
        match self {
            Self::Proxy => "proxy".into(),
            Self::Direct => "direct".into(),
            Self::Block => "block".into(),
            Self::WarpBypass => "warp-bypass".into(),
            Self::Profile(id) => id.to_string(),
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Proxy => "proxy".into(),
            Self::Direct => "direct".into(),
            Self::Block => "block".into(),
            Self::WarpBypass => "warp-bypass".into(),
            Self::Profile(id) => format!("profile:{id}"),
        }
    }

    /// Cycle among built-in defaults (proxy → direct → block → proxy).
    pub fn cycle_builtin(self) -> Self {
        match self {
            Self::Proxy => Self::Direct,
            Self::Direct => Self::Block,
            Self::Block | Self::WarpBypass | Self::Profile(_) => Self::Proxy,
        }
    }
}

/// One route rule — fields map 1:1 to upstream `route_rules` columns.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RouteRule {
    pub name: String,
    /// Persisted as raw int enum (`ruleType` in RouteRule.h).
    pub rule_type: i32,
    /// Share-schema token when imported from JSON (not stored in SQLite).
    #[serde(default)]
    pub rule_type_token: String,
    pub outbound_id: i64,
    pub domain: Vec<String>,
    pub domain_suffix: Vec<String>,
    pub domain_keyword: Vec<String>,
    pub domain_regex: Vec<String>,
    pub ip_cidr: Vec<String>,
    pub ip_is_private: bool,
    pub source_ip_cidr: Vec<String>,
    pub source_ip_is_private: bool,
    pub process_name: Vec<String>,
    pub process_path: Vec<String>,
    pub process_path_regex: Vec<String>,
    pub network: String,
    pub protocol: String,
    pub ip_version: String,
    pub inbound: Vec<String>,
    pub source_port: Vec<String>,
    pub source_port_range: Vec<String>,
    pub port: Vec<String>,
    pub port_range: Vec<String>,
    pub rule_set: Vec<String>,
    pub invert: bool,
    pub action: String,
    pub reject_method: String,
    pub no_drop: bool,
    pub override_address: String,
    pub override_port: String,
    pub sniffers: Vec<String>,
    pub sniff_override_dest: bool,
    pub strategy: String,
    pub wifi_ssid: Vec<String>,
    pub wifi_bssid: Vec<String>,
}

impl RouteRule {
    pub fn outbound(&self) -> DefaultOutbound {
        DefaultOutbound::from_id(self.outbound_id)
    }

    /// Map share-schema type token → persisted int (`ruleType` enum order).
    pub fn type_from_token(token: &str) -> i32 {
        match token {
            "simple_address_proxy" => 1,
            "simple_address_bypass" => 2,
            "simple_address_block" => 3,
            "simple_process_name_proxy" => 4,
            "simple_process_name_bypass" => 5,
            "simple_process_name_block" => 6,
            "simple_process_path_proxy" => 7,
            "simple_process_path_bypass" => 8,
            "simple_process_path_block" => 9,
            "simple_address_warp_bypass" => 10,
            "simple_process_name_warp_bypass" => 11,
            "simple_process_path_warp_bypass" => 12,
            // legacy / loose tokens
            "simple" | "custom" | "" => 0,
            _ => 0,
        }
    }

    pub fn token_from_type(t: i32) -> &'static str {
        match t {
            1 => "simple_address_proxy",
            2 => "simple_address_bypass",
            3 => "simple_address_block",
            4 => "simple_process_name_proxy",
            5 => "simple_process_name_bypass",
            6 => "simple_process_name_block",
            7 => "simple_process_path_proxy",
            8 => "simple_process_path_bypass",
            9 => "simple_process_path_block",
            10 => "simple_address_warp_bypass",
            11 => "simple_process_name_warp_bypass",
            12 => "simple_process_path_warp_bypass",
            _ => "custom",
        }
    }
}

/// Route profile — structured or raw sing-box `route` object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteProfile {
    pub id: i64,
    pub name: String,
    pub default_outbound: DefaultOutbound,
    pub rules: Vec<RouteRule>,
    pub is_raw: bool,
    pub raw_route: String,
    pub prevent_modifications: bool,
    pub is_remote: bool,
    pub remote_url: String,
    pub auto_update: bool,
    pub remote_last_update: i64,
}

impl RouteProfile {
    pub fn new(id: i64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            default_outbound: DefaultOutbound::Proxy,
            rules: Vec::new(),
            is_raw: false,
            raw_route: String::new(),
            prevent_modifications: false,
            is_remote: false,
            remote_url: String::new(),
            auto_update: false,
            remote_last_update: 0,
        }
    }

    pub fn rule_count(&self) -> usize {
        if self.is_raw {
            if self.raw_route.trim().is_empty() {
                0
            } else {
                1
            }
        } else {
            self.rules.len()
        }
    }

    pub fn summary(&self) -> String {
        if self.is_raw {
            format!("{} · raw route", self.name)
        } else if self.is_remote {
            format!("{} · remote · {} rules", self.name, self.rules.len())
        } else {
            format!("{} · {} rules", self.name, self.rules.len())
        }
    }
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
        assert_eq!(p.display_latency(), "");
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
