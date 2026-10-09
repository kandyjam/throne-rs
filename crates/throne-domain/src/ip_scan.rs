//! Persisted scanner configuration, using upstream's camelCase config JSON.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanPortMode {
    Ignore,
    #[default]
    Merge,
    List,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScanIcmpOptions {
    pub enabled: bool,
    pub timeout_ms: i32,
    pub count: i32,
}
impl Default for ScanIcmpOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_ms: 1000,
            count: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScanTcpOptions {
    pub enabled: bool,
    pub timeout_ms: i32,
    pub attempts: i32,
}
impl Default for ScanTcpOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_ms: 2000,
            attempts: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScanHttpOptions {
    pub enabled: bool,
    pub tls: bool,
    pub server_name: String,
    pub host: String,
    pub path: String,
    pub method: String,
    pub http_version: String,
    pub alpn: Vec<String>,
    pub min_version: String,
    pub max_version: String,
    pub fingerprint: String,
    pub insecure: bool,
    pub disable_sni: bool,
    pub fragment: bool,
    pub fragment_fallback_delay_ms: i32,
    pub record_fragment: bool,
    pub mixed_case_sni: bool,
    pub timeout_ms: i32,
}
impl Default for ScanHttpOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            tls: true,
            server_name: String::new(),
            host: String::new(),
            path: "/".into(),
            method: "GET".into(),
            http_version: "1.1".into(),
            alpn: Vec::new(),
            min_version: String::new(),
            max_version: String::new(),
            fingerprint: String::new(),
            insecure: false,
            disable_sni: false,
            fragment: false,
            fragment_fallback_delay_ms: 0,
            record_fragment: false,
            mixed_case_sni: false,
            timeout_ms: 3000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScanConfigTestOptions {
    pub enabled: bool,
    pub profile_id: i64,
    pub url: String,
    pub timeout_ms: i32,
    pub warm_latency: bool,
}
impl Default for ScanConfigTestOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            profile_id: -1,
            url: String::new(),
            timeout_ms: 3000,
            warm_latency: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScanWarpOptions {
    pub mode: String,
    pub http_mode: i32,
    pub identity: String,
    pub profile_id: i64,
    pub generated_identity: Value,
    pub generated_mode: String,
    pub mtu: i32,
    pub sni: String,
    pub jc: i32,
    pub jmin: i32,
    pub jmax: i32,
    pub i1: String,
    pub i2: String,
    pub i3: String,
    pub i4: String,
    pub i5: String,
    pub url: String,
    pub timeout_ms: i32,
    pub warm_latency: bool,
}
impl Default for ScanWarpOptions {
    fn default() -> Self {
        Self {
            mode: "wireguard".into(),
            http_mode: 0,
            identity: "generated".into(),
            profile_id: -1,
            generated_identity: serde_json::json!({}),
            generated_mode: String::new(),
            mtu: 1280,
            sni: String::new(),
            jc: 0,
            jmin: 0,
            jmax: 0,
            i1: String::new(),
            i2: String::new(),
            i3: String::new(),
            i4: String::new(),
            i5: String::new(),
            url: String::new(),
            timeout_ms: 5000,
            warm_latency: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScanConfig {
    pub ports: Vec<u16>,
    pub port_mode: ScanPortMode,
    pub shuffle: bool,
    #[serde(rename = "ipv6")]
    pub scan_ipv6: bool,
    pub stop_after: i32,
    pub concurrency: i32,
    pub spawn_interval_ms: i32,
    pub icmp: ScanIcmpOptions,
    pub tcp: ScanTcpOptions,
    pub http: ScanHttpOptions,
    pub config: ScanConfigTestOptions,
    pub warp: ScanWarpOptions,
}
impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            ports: Vec::new(),
            port_mode: ScanPortMode::Merge,
            shuffle: true,
            scan_ipv6: false,
            stop_after: 0,
            concurrency: 64,
            spawn_interval_ms: 2,
            icmp: ScanIcmpOptions::default(),
            tcp: ScanTcpOptions::default(),
            http: ScanHttpOptions::default(),
            config: ScanConfigTestOptions::default(),
            warp: ScanWarpOptions::default(),
        }
    }
}

impl ScanConfig {
    /// Upstream's persisted resume fingerprint; changing port order changes
    /// the core target iteration even when the set of ports is identical.
    pub fn target_spec_hash(&self, base_list_id: i64) -> String {
        let mut seen = std::collections::HashSet::new();
        let ports = self
            .ports
            .iter()
            .copied()
            .filter(|p| *p > 0 && seen.insert(*p))
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let mode = match self.port_mode {
            ScanPortMode::Ignore => "ignore",
            ScanPortMode::Merge => "merge",
            ScanPortMode::List => "list",
        };
        let input = format!(
            "{base_list_id}|{ports}|{mode}|{}|{}",
            u8::from(self.shuffle),
            u8::from(self.scan_ipv6)
        );
        sha1_smol::Sha1::from(input).digest().to_string()
    }

    pub fn for_kind(kind: IpScanKind) -> Self {
        let mut config = Self::default();
        if kind == IpScanKind::Warp {
            config.ports = vec![2408, 500, 1701, 4500];
            config.concurrency = 16;
            config.stop_after = 10;
            config.tcp.enabled = false;
        }
        config
    }

    /// Missing or invalid JSON fields retain defaults, as Qt's FromJson does.
    pub fn from_json(kind: IpScanKind, value: &Value) -> Self {
        fn merge(target: &mut Value, source: &Value) {
            let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) else {
                return;
            };
            for (key, source) in source {
                let Some(target) = target.get_mut(key) else {
                    continue;
                };
                if source.is_object() && target.is_object() {
                    if key == "generatedIdentity" {
                        *target = source.clone();
                    } else {
                        merge(target, source);
                    }
                } else if source.is_number() && target.is_number() {
                    if let Some(number) = source.as_f64() {
                        // Qt reads every scalar numeric option as an int. Clamp
                        // before deserializing so one oversized value cannot
                        // discard the other valid options in the config.
                        *target = (number.clamp(i32::MIN as f64, i32::MAX as f64) as i32).into();
                    }
                } else if (source.is_boolean() && target.is_boolean())
                    || (source.is_string() && target.is_string())
                    || (source.is_array() && target.is_array())
                {
                    *target = source.clone();
                }
            }
        }
        let default = Self::for_kind(kind);
        let mut base = serde_json::to_value(&default).expect("scan defaults serialize");
        merge(&mut base, value);
        if let Some(ports) = base.get_mut("ports").and_then(Value::as_array_mut) {
            let mut seen = std::collections::HashSet::new();
            ports.retain(|v| {
                v.as_u64()
                    .is_some_and(|p| p > 0 && p <= 65535 && seen.insert(p))
            });
        }
        if !matches!(base["portMode"].as_str(), Some("ignore" | "merge" | "list")) {
            base["portMode"] = "merge".into();
        }
        if let Some(alpn) = base["http"]["alpn"].as_array_mut() {
            let mut seen = std::collections::HashSet::new();
            *alpn = alpn
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty() && seen.insert((*s).to_owned()))
                .map(Value::from)
                .collect();
        }
        for (section, key, allowed, fallback) in [
            ("http", "method", &["GET", "HEAD", "NONE"][..], "GET"),
            ("http", "httpVersion", &["1.1", "2", "3"][..], "1.1"),
            (
                "http",
                "minVersion",
                &["", "1.0", "1.1", "1.2", "1.3"][..],
                "",
            ),
            (
                "http",
                "maxVersion",
                &["", "1.0", "1.1", "1.2", "1.3"][..],
                "",
            ),
            (
                "warp",
                "mode",
                &["wireguard", "amneziawg", "masque"][..],
                "wireguard",
            ),
            (
                "warp",
                "identity",
                &["generated", "profile", "builtin"][..],
                "generated",
            ),
            (
                "warp",
                "generatedMode",
                &["", "wireguard", "masque"][..],
                "",
            ),
        ] {
            let text = base[section][key].as_str().unwrap_or_default().trim();
            base[section][key] = allowed
                .iter()
                .copied()
                .find(|choice| text.eq_ignore_ascii_case(choice))
                .unwrap_or(fallback)
                .into();
        }
        let mut config = serde_json::from_value(base).unwrap_or(default);
        config.normalize();
        config
    }

    pub fn normalize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.ports.retain(|p| *p > 0 && seen.insert(*p));
        self.stop_after = self.stop_after.clamp(0, 1_000_000_000);
        self.concurrency = self.concurrency.clamp(1, 1000);
        self.spawn_interval_ms = self.spawn_interval_ms.clamp(0, 1000);
        self.icmp.timeout_ms = self.icmp.timeout_ms.clamp(100, 60000);
        self.icmp.count = self.icmp.count.clamp(1, 10);
        self.tcp.timeout_ms = self.tcp.timeout_ms.clamp(100, 60000);
        self.tcp.attempts = self.tcp.attempts.clamp(1, 10);
        self.http.timeout_ms = self.http.timeout_ms.clamp(100, 60000);
        self.http.fragment_fallback_delay_ms = self.http.fragment_fallback_delay_ms.clamp(0, 60000);
        self.config.timeout_ms = self.config.timeout_ms.clamp(100, 60000);
        self.config.profile_id = self.config.profile_id.max(-1);
        self.warp.timeout_ms = self.warp.timeout_ms.clamp(100, 60000);
        self.warp.profile_id = self.warp.profile_id.max(-1);
        self.warp.http_mode = self.warp.http_mode.clamp(0, 2);
        self.warp.mtu = self.warp.mtu.clamp(576, 9000);
        self.warp.jc = self.warp.jc.clamp(0, 128);
        self.warp.jmin = self.warp.jmin.clamp(0, 1280);
        self.warp.jmax = self.warp.jmax.clamp(0, 1280);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum IpScanKind {
    #[default]
    Generic = 0,
    Warp = 1,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum IpScanStatus {
    #[default]
    Idle = 0,
    Running = 1,
    Paused = 2,
    Completed = 3,
    Failed = 4,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum IpScanMode {
    #[default]
    Initial = 0,
    RescanResult = 1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpScan {
    pub id: i64,
    pub name: String,
    pub kind: IpScanKind,
    pub base_list_id: i64,
    pub result_list_id: i64,
    pub config: ScanConfig,
    pub status: IpScanStatus,
    pub mode: IpScanMode,
    pub snapshot_list_id: i64,
    pub seed: u64,
    pub cursor: u64,
    pub total: u64,
    pub spec_hash: String,
    pub rescan_snapshot_list_id: i64,
    pub rescan_cursor: u64,
    pub rescan_total: u64,
    pub found: i32,
    pub removed: i32,
    pub last_error: String,
    pub started_at: i64,
    pub finished_at: i64,
}

impl Default for IpScan {
    fn default() -> Self {
        Self::new("", IpScanKind::Generic)
    }
}
impl IpScan {
    pub fn new(name: impl Into<String>, kind: IpScanKind) -> Self {
        Self {
            id: -1,
            name: name.into(),
            kind,
            base_list_id: -1,
            result_list_id: -1,
            config: ScanConfig::for_kind(kind),
            status: IpScanStatus::Idle,
            mode: IpScanMode::Initial,
            snapshot_list_id: -1,
            seed: 0,
            cursor: 0,
            total: 0,
            spec_hash: String::new(),
            rescan_snapshot_list_id: -1,
            rescan_cursor: 0,
            rescan_total: 0,
            found: 0,
            removed: 0,
            last_error: String::new(),
            started_at: 0,
            finished_at: 0,
        }
    }
    pub fn active_cursor(&self) -> u64 {
        if self.mode == IpScanMode::RescanResult {
            self.rescan_cursor
        } else {
            self.cursor
        }
    }
    pub fn active_total(&self) -> u64 {
        if self.mode == IpScanMode::RescanResult {
            self.rescan_total
        } else {
            self.total
        }
    }
    pub fn can_resume_initial(&self) -> bool {
        self.snapshot_list_id >= 0 && self.total > 0 && self.cursor < self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn upstream_config_json_preserves_defaults_ports_and_bounds() {
        let config = ScanConfig::from_json(
            IpScanKind::Warp,
            &json!({"concurrency":0,"ports":[443,0,443,65536,8443],"http":{"serverName":"example.org"}}),
        );
        assert_eq!(config.ports, [443, 8443]);
        assert_eq!(config.concurrency, 1);
        assert!(!config.tcp.enabled);
        assert!(config.http.tls);
        assert_eq!(config.stop_after, 10);
        let value = serde_json::to_value(config).unwrap();
        assert_eq!(value["portMode"], "merge");
        assert_eq!(value["http"]["serverName"], "example.org");
        assert_eq!(value["warp"]["generatedIdentity"], json!({}));
        assert!(value.get("ipv6").is_some());
    }

    #[test]
    fn resume_requires_unfinished_snapshot_and_uses_active_pass() {
        let mut scan = IpScan::default();
        scan.snapshot_list_id = 1;
        scan.total = 5;
        scan.cursor = 2;
        assert!(scan.can_resume_initial());
        scan.mode = IpScanMode::RescanResult;
        scan.rescan_cursor = 3;
        scan.rescan_total = 4;
        assert_eq!((scan.active_cursor(), scan.active_total()), (3, 4));
        scan.cursor = 5;
        assert!(!scan.can_resume_initial());
    }

    #[test]
    fn invalid_scanner_fields_do_not_discard_other_valid_options() {
        let config = ScanConfig::from_json(
            IpScanKind::Generic,
            &json!({
                "ports":[8443],"concurrency":1e30,"http":{"enabled":true,"method":" head ",
                "httpVersion":"invalid","timeoutMs":1500.9,"alpn":["h2",4," h2 "," ","http/1.1"]}
            }),
        );
        assert_eq!(config.ports, [8443]);
        assert_eq!(config.concurrency, 1000);
        assert_eq!(config.http.timeout_ms, 1500);
        assert!(config.http.enabled);
        assert_eq!(config.http.method, "HEAD");
        assert_eq!(config.http.http_version, "1.1");
        assert_eq!(config.http.alpn, ["h2", "http/1.1"]);
    }

    #[test]
    fn resume_fingerprint_changes_only_with_target_order_options() {
        let mut config = ScanConfig {
            ports: vec![443, 8443],
            ..ScanConfig::default()
        };
        let hash = config.target_spec_hash(7);
        config.concurrency = 1;
        config.http.server_name = "example.org".into();
        assert_eq!(config.target_spec_hash(7), hash);
        config.ports.push(443);
        assert_eq!(config.target_spec_hash(7), hash);
        config.ports.reverse();
        config.ports = vec![8443, 443];
        assert_ne!(config.target_spec_hash(7), hash);
        assert_ne!(config.target_spec_hash(8), config.target_spec_hash(7));
    }
}
