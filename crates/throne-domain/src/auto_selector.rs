//! Auto Selector profile support (upstream Throne 1.2.3+ / 1.2.4).
//!
//! An Auto Selector tracks a whole group instead of one server. Membership is
//! resolved at build/start time from `gid` + filters; ranking prefers measured
//! latency. The core then switches among the built pool (or we sticky-pick the
//! best member when the running ThroneCore lacks `auto-selector` outbound).
//!
//! 1.2.4: Xray full-config members are eligible (each gets its own gated Xray
//! instance + socks bridge). Sing-box full configs still skip as [`AutoSelectorSkip::FullConfig`].

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::models::{Group, GroupId, Profile, ProfileId, ProfileType};

/// Why a profile in the tracked group did not become a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoSelectorSkip {
    Missing,
    MetaType,
    ExtraCore,
    /// Sing-box full config wants the whole box (1.2.4: Xray full is no longer this).
    FullConfig,
    Malformed,
    Tailscale,
    NameFilter,
    CountryFilter,
    Unavailable,
    /// Xray full config cannot be combined with the group's landing/front proxies.
    XrayFullChained,
    /// OpenVPN/OpenConnect endpoints are started alongside a route, not as selector members.
    VpnEndpoint,
}

impl AutoSelectorSkip {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing profile",
            Self::MetaType => "chain or auto selector",
            Self::ExtraCore => "extra-core profile",
            Self::FullConfig => "full config profile",
            Self::Malformed => "config does not parse",
            Self::Tailscale => "Tailscale profile",
            Self::NameFilter => "filtered out by name",
            Self::CountryFilter => "filtered out by country",
            Self::Unavailable => "last test failed",
            Self::XrayFullChained => "Xray full config cannot be combined with the group's proxies",
            Self::VpnEndpoint => "OpenVPN/OpenConnect endpoint",
        }
    }
}

/// Membership decision for one auto-selector profile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoSelectorPlan {
    /// Eligible members, best first, capped at pool_cap.
    pub pool: Vec<ProfileId>,
    /// Prefix of `pool` that enters the config (build_limit).
    pub build: Vec<ProfileId>,
    pub members_in_group: usize,
    pub eligible: usize,
    pub ranked_by_test: usize,
    pub skipped: Vec<(AutoSelectorSkip, usize)>,
    pub truncated: bool,
    pub pool_cap_used: i32,
    pub build_limit_used: i32,
    /// True when ordering is untrusted and a URL test should run first.
    pub needs_ranking: bool,
    pub error: Option<String>,
}

impl AutoSelectorPlan {
    pub fn skipped_count(&self) -> usize {
        self.skipped.iter().map(|(_, n)| n).sum()
    }

    pub fn summary(&self) -> String {
        if let Some(err) = &self.error {
            return err.clone();
        }
        format!(
            "eligible {}/{} · build {} · ranked {} · skipped {}",
            self.eligible,
            self.members_in_group,
            self.build.len(),
            self.ranked_by_test,
            self.skipped_count()
        )
    }
}

/// Wire-compatible auto-selector outbound JSON (upstream `type: autoselector`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoSelectorConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_gid")]
    pub gid: GroupId,
    #[serde(default)]
    pub name_filter: String,
    #[serde(default)]
    pub country_filter: String,
    #[serde(default = "default_true")]
    pub exclude_unavailable: bool,
    #[serde(default = "default_pool_cap")]
    pub pool_cap: i32,
    #[serde(default = "default_build_limit")]
    pub build_limit: i32,
    #[serde(default = "default_result_validity")]
    pub result_validity_mins: i32,
    #[serde(default)]
    pub test_url: String,
    #[serde(default)]
    pub connectivity_url: String,
    #[serde(default = "default_interval")]
    pub interval_sec: i32,
    #[serde(default = "default_bench_interval")]
    pub bench_interval_sec: i32,
    #[serde(default = "default_watch_interval")]
    pub watch_interval_sec: i32,
    #[serde(default = "default_active_size")]
    pub active_size: i32,
    #[serde(default = "default_sampling")]
    pub sampling: i32,
    #[serde(default = "default_tolerance")]
    pub tolerance_ms: i32,
    #[serde(default)]
    pub max_rtt_ms: i32,
    #[serde(default = "default_expected")]
    pub expected: i32,
    #[serde(default = "default_dial_retries")]
    pub dial_retries: i32,
    #[serde(default = "default_true")]
    pub interrupt_on_switch: bool,
    #[serde(default)]
    pub balance: bool,
    #[serde(default = "default_balance_mode")]
    pub balance_mode: String,
    #[serde(default = "default_balance_interval")]
    pub balance_interval_sec: i32,
    #[serde(default)]
    pub pool: Vec<ProfileId>,
    #[serde(default)]
    pub pool_ranked_at: i64,
    #[serde(default = "default_pinned")]
    pub pinned_id: ProfileId,
    #[serde(default)]
    pub last_built: Vec<ProfileId>,
    #[serde(default)]
    pub last_built_at: i64,
    #[serde(default)]
    pub history: Value,
}

fn default_gid() -> GroupId {
    -1
}
fn default_true() -> bool {
    true
}
fn default_pool_cap() -> i32 {
    1000
}
fn default_build_limit() -> i32 {
    300
}
fn default_result_validity() -> i32 {
    1440
}
fn default_interval() -> i32 {
    300
}
fn default_bench_interval() -> i32 {
    600
}
fn default_watch_interval() -> i32 {
    15
}
fn default_active_size() -> i32 {
    8
}
fn default_sampling() -> i32 {
    10
}
fn default_tolerance() -> i32 {
    300
}
fn default_expected() -> i32 {
    3
}
fn default_dial_retries() -> i32 {
    2
}
fn default_balance_mode() -> String {
    "rotate".into()
}
fn default_balance_interval() -> i32 {
    30
}
fn default_pinned() -> ProfileId {
    -1
}

impl Default for AutoSelectorConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            gid: -1,
            name_filter: String::new(),
            country_filter: String::new(),
            exclude_unavailable: true,
            pool_cap: 1000,
            build_limit: 300,
            result_validity_mins: 1440,
            test_url: String::new(),
            connectivity_url: String::new(),
            interval_sec: 300,
            bench_interval_sec: 600,
            watch_interval_sec: 15,
            active_size: 8,
            sampling: 10,
            tolerance_ms: 300,
            max_rtt_ms: 0,
            expected: 3,
            dial_retries: 2,
            interrupt_on_switch: true,
            balance: false,
            balance_mode: "rotate".into(),
            balance_interval_sec: 30,
            pool: Vec::new(),
            pool_ranked_at: 0,
            pinned_id: -1,
            last_built: Vec::new(),
            last_built_at: 0,
            history: Value::Array(Vec::new()),
        }
    }
}

impl AutoSelectorConfig {
    pub const MAX_POOL_CAP: i32 = 3000;
    pub const MAX_BUILD_LIMIT: i32 = 500;

    pub fn new_for_group(gid: GroupId, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            gid,
            ..Self::default()
        }
    }

    pub fn normalize(&mut self) {
        if self.pool_cap < 1 {
            self.pool_cap = 1;
        }
        if self.pool_cap > Self::MAX_POOL_CAP {
            self.pool_cap = Self::MAX_POOL_CAP;
        }
        if self.build_limit < 1 {
            self.build_limit = 1;
        }
        if self.build_limit > Self::MAX_BUILD_LIMIT {
            self.build_limit = Self::MAX_BUILD_LIMIT;
        }
        if self.build_limit > self.pool_cap {
            self.build_limit = self.pool_cap;
        }
        if self.result_validity_mins < 0 {
            self.result_validity_mins = 0;
        }
        if self.interval_sec < 10 {
            self.interval_sec = 10;
        }
        if self.bench_interval_sec < self.interval_sec {
            self.bench_interval_sec = self.interval_sec;
        }
        if self.watch_interval_sec < 5 {
            self.watch_interval_sec = 5;
        }
        if self.watch_interval_sec > self.interval_sec {
            self.watch_interval_sec = self.interval_sec;
        }
        if self.active_size < 1 {
            self.active_size = 1;
        }
        if self.expected < 1 {
            self.expected = 1;
        }
        if self.active_size < self.expected {
            self.active_size = self.expected;
        }
        if self.active_size > self.build_limit {
            self.active_size = self.build_limit;
        }
        if self.sampling < 2 {
            self.sampling = 2;
        }
        if self.sampling > 60 {
            self.sampling = 60;
        }
        if self.tolerance_ms < 0 {
            self.tolerance_ms = 0;
        }
        if self.max_rtt_ms < 0 {
            self.max_rtt_ms = 0;
        }
        if self.expected > self.build_limit {
            self.expected = self.build_limit;
        }
        if self.dial_retries < 0 {
            self.dial_retries = 0;
        }
        if self.dial_retries > 5 {
            self.dial_retries = 5;
        }
        if self.balance_interval_sec < 5 {
            self.balance_interval_sec = 5;
        }
        if self.balance_mode != "connection" {
            self.balance_mode = "rotate".into();
        }
    }

    pub fn to_outbound_json(&self) -> String {
        let mut cfg = self.clone();
        cfg.normalize();
        let mut obj = json!({
            "type": "autoselector",
            "name": cfg.name,
            "gid": cfg.gid,
            "name_filter": cfg.name_filter,
            "country_filter": cfg.country_filter,
            "exclude_unavailable": cfg.exclude_unavailable,
            "pool_cap": cfg.pool_cap,
            "build_limit": cfg.build_limit,
            "result_validity_mins": cfg.result_validity_mins,
            "test_url": cfg.test_url,
            "connectivity_url": cfg.connectivity_url,
            "interval_sec": cfg.interval_sec,
            "bench_interval_sec": cfg.bench_interval_sec,
            "watch_interval_sec": cfg.watch_interval_sec,
            "active_size": cfg.active_size,
            "sampling": cfg.sampling,
            "tolerance_ms": cfg.tolerance_ms,
            "max_rtt_ms": cfg.max_rtt_ms,
            "expected": cfg.expected,
            "dial_retries": cfg.dial_retries,
            "interrupt_on_switch": cfg.interrupt_on_switch,
            "balance": cfg.balance,
            "balance_mode": cfg.balance_mode,
            "balance_interval_sec": cfg.balance_interval_sec,
            "pool": cfg.pool,
            "pool_ranked_at": cfg.pool_ranked_at,
            "pinned_id": cfg.pinned_id,
            "last_built": cfg.last_built,
            "last_built_at": cfg.last_built_at,
            "history": cfg.history,
        });
        if let Some(map) = obj.as_object_mut() {
            if !cfg.name.is_empty() {
                map.insert("tag".into(), json!(cfg.name));
            }
        }
        serde_json::to_string(&obj).unwrap_or_else(|_| "{}".into())
    }

    pub fn from_outbound_json(s: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(s).ok()?;
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if !matches!(ty, "autoselector" | "auto_selector" | "auto-selector" | "") {
            if v.get("gid").is_none() {
                return None;
            }
        } else if ty.is_empty() && v.get("gid").is_none() {
            return None;
        }
        let mut cfg: Self = serde_json::from_value(v).ok()?;
        cfg.normalize();
        Some(cfg)
    }
}

/// Parse Auto Selector config from a profile, if it is one.
pub fn profile_auto_selector(profile: &Profile) -> Option<AutoSelectorConfig> {
    if profile.profile_type == ProfileType::AutoSelector {
        return AutoSelectorConfig::from_outbound_json(&profile.outbound_json).or_else(|| {
            let mut cfg = AutoSelectorConfig::default();
            cfg.name = profile.name.clone();
            Some(cfg)
        });
    }
    AutoSelectorConfig::from_outbound_json(&profile.outbound_json).and_then(|cfg| {
        // Only accept non-typed JSON when it clearly looks like a selector.
        if cfg.gid >= 0 {
            Some(cfg)
        } else {
            None
        }
    })
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn has_fresh_result(member: &Profile, selector: &AutoSelectorConfig) -> bool {
    if member.latency_ms == 0 {
        return false;
    }
    // Without a dedicated latency_at column, any non-zero result is treated as
    // fresh when the validity window is enabled.
    selector.result_validity_mins > 0
}

fn effective_latency(member: &Profile, selector: &AutoSelectorConfig) -> i32 {
    if has_fresh_result(member, selector) {
        member.latency_ms
    } else {
        0
    }
}

fn latency_rank(latency: i32) -> i32 {
    if latency > 0 {
        0
    } else if latency == 0 {
        1
    } else {
        2
    }
}

fn country_set(filter: &str) -> Vec<String> {
    filter
        .split(',')
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Case-insensitive substring match; `*` wildcards expand to "contains segments".
fn name_filter_matches(filter: &str, name: &str) -> bool {
    let filter = filter.trim();
    if filter.is_empty() {
        return true;
    }
    let name_l = name.to_ascii_lowercase();
    let filter_l = filter.to_ascii_lowercase();
    if filter_l.contains('*') {
        let parts: Vec<_> = filter_l.split('*').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return true;
        }
        let mut rest = name_l.as_str();
        for (i, part) in parts.iter().enumerate() {
            if let Some(idx) = rest.find(part) {
                if i == 0 && !filter_l.starts_with('*') && idx != 0 {
                    return false;
                }
                rest = &rest[idx + part.len()..];
            } else {
                return false;
            }
        }
        if !filter_l.ends_with('*') && !rest.is_empty() && parts.last().is_some() {
            // last segment must be suffix when no trailing *
            return name_l.ends_with(parts.last().unwrap());
        }
        return true;
    }
    name_l.contains(&filter_l)
}

/// Classify a custom profile for Auto Selector membership (upstream 1.2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomMemberKind {
    /// Ordinary custom outbound JSON — fine as a leaf.
    Outbound,
    /// Xray full config — allowed; core runs a gated instance + socks bridge.
    XrayFull,
    /// Sing-box full config — still excluded (wants the whole box).
    SingBoxFull,
    Unknown,
}

/// Detect custom/full-config shape from stored outbound JSON / raw_json.
pub fn classify_custom_member(member: &Profile) -> CustomMemberKind {
    if member.profile_type != ProfileType::Custom {
        return CustomMemberKind::Unknown;
    }
    // Prefer structured outbound_json (upstream ExportToJson: type=custom, subtype=…).
    if let Ok(v) = serde_json::from_str::<Value>(&member.outbound_json) {
        let subtype = v
            .get("subtype")
            .or_else(|| v.get("custom_type"))
            .and_then(|t| t.as_str())
            .unwrap_or("");
        match subtype {
            "xrayfullconfig" | "xray_full" | "xray-full" => return CustomMemberKind::XrayFull,
            "fullconfig" | "full_config" | "full-config" => return CustomMemberKind::SingBoxFull,
            "outbound" | "xrayoutbound" | "xray_outbound" => return CustomMemberKind::Outbound,
            _ => {}
        }
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if matches!(ty, "custom" | "") {
            if let Some(cfg) = v.get("config").and_then(|c| c.as_str()) {
                return classify_raw_config_blob(cfg);
            }
            if v.get("config").is_some() || v.get("core").is_some() {
                // Embedded object without subtype — treat as sing-box full unless Xray-shaped.
                if let Some(cfg) = v.get("config") {
                    if looks_like_xray_full(cfg) {
                        return CustomMemberKind::XrayFull;
                    }
                }
                return CustomMemberKind::SingBoxFull;
            }
        }
    }
    // Import path often stores full configs in raw_json + security marker.
    if let Some(raw) = member.outbound.raw_json.as_deref() {
        let sec = member.outbound.security.as_deref().unwrap_or("");
        if sec == "xray-full-config" || sec == "xrayfullconfig" {
            return CustomMemberKind::XrayFull;
        }
        if sec == "full-config" || sec == "fullconfig" {
            return classify_raw_config_blob(raw);
        }
        if !raw.is_empty() {
            return classify_raw_config_blob(raw);
        }
    }
    CustomMemberKind::Outbound
}

fn classify_raw_config_blob(raw: &str) -> CustomMemberKind {
    let Ok(v) = serde_json::from_str::<Value>(raw) else {
        return CustomMemberKind::Unknown;
    };
    if looks_like_xray_full(&v) {
        return CustomMemberKind::XrayFull;
    }
    if looks_like_singbox_full(&v) {
        return CustomMemberKind::SingBoxFull;
    }
    CustomMemberKind::Outbound
}

fn looks_like_xray_full(v: &Value) -> bool {
    let Some(outs) = v.get("outbounds").and_then(|o| o.as_array()) else {
        return false;
    };
    // Xray outbounds use `protocol`; sing-box uses `type`.
    outs.iter().any(|o| o.get("protocol").is_some())
}

fn looks_like_singbox_full(v: &Value) -> bool {
    let has_inbounds = v
        .get("inbounds")
        .and_then(|a| a.as_array())
        .is_some_and(|a| !a.is_empty());
    let has_typed_outbounds = v
        .get("outbounds")
        .and_then(|a| a.as_array())
        .is_some_and(|a| a.iter().any(|o| o.get("type").is_some()));
    has_inbounds && has_typed_outbounds
}

/// True when this member should be started as an opaque Xray full-config instance.
pub fn is_xray_full_config_member(member: &Profile) -> bool {
    classify_custom_member(member) == CustomMemberKind::XrayFull
}

fn member_skip(
    member: Option<&Profile>,
    selector: &AutoSelectorConfig,
    group: Option<&Group>,
) -> Option<AutoSelectorSkip> {
    let Some(member) = member else {
        return Some(AutoSelectorSkip::Missing);
    };
    match member.profile_type {
        ProfileType::Chain | ProfileType::AutoSelector => {
            return Some(AutoSelectorSkip::MetaType);
        }
        ProfileType::Tailscale => return Some(AutoSelectorSkip::Tailscale),
        ProfileType::ExtraCore => return Some(AutoSelectorSkip::ExtraCore),
        ProfileType::OpenVpn | ProfileType::OpenConnect => {
            return Some(AutoSelectorSkip::VpnEndpoint);
        }
        ProfileType::Custom => match classify_custom_member(member) {
            // 1.2.4: only sing-box full config is excluded; Xray full is fine
            // unless the group chains landing/front proxies (XrayFullChained).
            CustomMemberKind::SingBoxFull => return Some(AutoSelectorSkip::FullConfig),
            CustomMemberKind::XrayFull => {
                if !xray_full_config_fits_group_chain(group) {
                    return Some(AutoSelectorSkip::XrayFullChained);
                }
            }
            CustomMemberKind::Outbound | CustomMemberKind::Unknown => {}
        },
        _ => {}
    }
    if !name_filter_matches(&selector.name_filter, &member.name) {
        return Some(AutoSelectorSkip::NameFilter);
    }
    let countries = country_set(&selector.country_filter);
    if !countries.is_empty()
        && !countries
            .iter()
            .any(|c| c.eq_ignore_ascii_case(member.test_country.trim()))
    {
        return Some(AutoSelectorSkip::CountryFilter);
    }
    if selector.exclude_unavailable && member.latency_ms < 0 && has_fresh_result(member, selector) {
        return Some(AutoSelectorSkip::Unavailable);
    }
    None
}

/// Upstream `xrayFullConfigFitsChain`: Xray full config only works with a bare
/// group (no landing / front proxy). Those would force a multi-hop chain the
/// full config cannot join.
fn xray_full_config_fits_group_chain(group: Option<&Group>) -> bool {
    let Some(group) = group else {
        return true;
    };
    group.landing_proxy_id < 0 && group.front_proxy_id < 0
}

/// Resolve membership for an auto-selector profile.
pub fn plan_auto_selector(
    selector_profile: &Profile,
    selector: &AutoSelectorConfig,
    group: Option<&Group>,
    lookup: impl Fn(ProfileId) -> Option<Profile>,
) -> AutoSelectorPlan {
    let mut plan = AutoSelectorPlan {
        pool_cap_used: selector.pool_cap,
        build_limit_used: selector.build_limit,
        ..Default::default()
    };

    let Some(group) = group else {
        plan.error = Some("Auto selector points at a group that no longer exists".into());
        return plan;
    };

    let mut skip_counts: Vec<(AutoSelectorSkip, usize)> = Vec::new();
    let mut bump = |skip: AutoSelectorSkip| {
        if let Some((_, n)) = skip_counts.iter_mut().find(|(s, _)| *s == skip) {
            *n += 1;
        } else {
            skip_counts.push((skip, 1));
        }
    };

    let mut members: Vec<ProfileId> = Vec::new();
    for &id in &group.profile_ids {
        if id == selector_profile.id {
            continue;
        }
        plan.members_in_group += 1;
        let member = lookup(id);
        if let Some(skip) = member_skip(member.as_ref(), selector, Some(group)) {
            bump(skip);
            continue;
        }
        if let Some(m) = member.as_ref() {
            if effective_latency(m, selector) > 0 {
                plan.ranked_by_test += 1;
            }
        }
        members.push(id);
    }
    plan.skipped = skip_counts;
    plan.eligible = members.len();
    if members.is_empty() {
        plan.error = Some("Auto selector has no usable members".into());
        return plan;
    }

    // Order: persisted pool first, then newcomers by latency.
    let mut ordered = Vec::new();
    let mut placed = std::collections::HashSet::new();
    let eligible: std::collections::HashSet<_> = members.iter().copied().collect();
    for id in &selector.pool {
        if eligible.contains(id) && placed.insert(*id) {
            ordered.push(*id);
        }
    }
    let mut newcomers: Vec<_> = members
        .into_iter()
        .filter(|id| !placed.contains(id))
        .collect();
    newcomers.sort_by(|&a, &b| {
        let la = lookup(a)
            .map(|m| effective_latency(&m, selector))
            .unwrap_or(0);
        let lb = lookup(b)
            .map(|m| effective_latency(&m, selector))
            .unwrap_or(0);
        let ra = latency_rank(la);
        let rb = latency_rank(lb);
        ra.cmp(&rb).then_with(|| {
            if ra == 0 {
                la.cmp(&lb)
            } else {
                std::cmp::Ordering::Equal
            }
        })
    });
    ordered.extend(newcomers);

    plan.truncated = ordered.len() as i32 > selector.pool_cap;
    if ordered.len() as i32 > selector.pool_cap {
        ordered.truncate(selector.pool_cap as usize);
    }
    plan.pool = ordered.clone();
    let build_n = (selector.build_limit as usize).min(ordered.len());
    plan.build = ordered[..build_n].to_vec();

    if ordered.len() as i32 > selector.build_limit {
        let unranked = plan
            .build
            .iter()
            .filter(|&&id| {
                lookup(id)
                    .map(|m| effective_latency(&m, selector) == 0)
                    .unwrap_or(true)
            })
            .count();
        plan.needs_ranking = unranked > 0;
    }

    plan
}

/// Re-order pool purely by current latencies and return ranked ids.
pub fn rerank_auto_selector_pool(
    selector: &mut AutoSelectorConfig,
    selector_profile_id: ProfileId,
    group: Option<&Group>,
    lookup: impl Fn(ProfileId) -> Option<Profile>,
) -> Vec<ProfileId> {
    let dummy = Profile::new(
        selector_profile_id,
        selector.gid,
        selector.name.clone(),
        ProfileType::AutoSelector,
    );
    let plan = plan_auto_selector(&dummy, selector, group, &lookup);
    if plan.error.is_some() {
        return Vec::new();
    }
    let mut members = plan.pool;
    members.sort_by(|&a, &b| {
        let la = lookup(a)
            .map(|m| effective_latency(&m, selector))
            .unwrap_or(0);
        let lb = lookup(b)
            .map(|m| effective_latency(&m, selector))
            .unwrap_or(0);
        let ra = latency_rank(la);
        let rb = latency_rank(lb);
        ra.cmp(&rb).then_with(|| {
            if ra == 0 {
                la.cmp(&lb)
            } else {
                std::cmp::Ordering::Equal
            }
        })
    });
    if members.len() as i32 > selector.pool_cap {
        members.truncate(selector.pool_cap as usize);
    }
    selector.pool = members.clone();
    selector.pool_ranked_at = now_secs();
    members
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_group(id: GroupId, profiles: &[ProfileId]) -> Group {
        let mut g = Group::new(id, "g");
        g.profile_ids = profiles.to_vec();
        g
    }

    fn make_node(id: ProfileId, gid: GroupId, name: &str, latency: i32) -> Profile {
        let mut p = Profile::new(id, gid, name, ProfileType::Vless);
        p.latency_ms = latency;
        p.outbound.server = Some(format!("{name}.example.com"));
        p.outbound.server_port = Some(443);
        p
    }

    #[test]
    fn plans_best_first_and_skips_meta() {
        let mut map = HashMap::new();
        map.insert(1, make_node(1, 10, "slow", 200));
        map.insert(2, make_node(2, 10, "fast", 40));
        map.insert(3, {
            let mut p = make_node(3, 10, "chain", 10);
            p.profile_type = ProfileType::Chain;
            p
        });
        map.insert(4, make_node(4, 10, "fail", -1));
        let selector_profile = Profile::new(99, 10, "auto", ProfileType::AutoSelector);
        let mut cfg = AutoSelectorConfig::new_for_group(10, "auto");
        cfg.exclude_unavailable = true;
        let group = make_group(10, &[1, 2, 3, 4, 99]);
        let plan = plan_auto_selector(&selector_profile, &cfg, Some(&group), |id| {
            map.get(&id).cloned()
        });
        assert!(plan.error.is_none(), "{:?}", plan.error);
        assert_eq!(plan.eligible, 2);
        assert_eq!(plan.build[0], 2);
        assert_eq!(plan.build[1], 1);
        assert!(plan
            .skipped
            .iter()
            .any(|(s, n)| *s == AutoSelectorSkip::MetaType && *n == 1));
        assert!(plan
            .skipped
            .iter()
            .any(|(s, n)| *s == AutoSelectorSkip::Unavailable && *n == 1));
    }

    #[test]
    fn roundtrips_outbound_json() {
        let mut cfg = AutoSelectorConfig::new_for_group(7, "Pick best");
        cfg.balance = true;
        cfg.build_limit = 50;
        let json = cfg.to_outbound_json();
        let back = AutoSelectorConfig::from_outbound_json(&json).expect("parse");
        assert_eq!(back.gid, 7);
        assert!(back.balance);
        assert_eq!(back.build_limit, 50);
        assert_eq!(back.name, "Pick best");
    }

    #[test]
    fn name_filter_substring_and_wildcard() {
        assert!(name_filter_matches("sing", "Singapore-01"));
        assert!(!name_filter_matches("jp", "Singapore-01"));
        assert!(name_filter_matches("*sg*", "xx-sg-01"));
    }

    #[test]
    fn xray_full_members_eligible_singbox_full_skipped() {
        let mut map = HashMap::new();
        map.insert(1, {
            let mut p = make_node(1, 10, "xray-full", 30);
            p.profile_type = ProfileType::Custom;
            p.outbound.security = Some("full-config".into());
            p.outbound.raw_json = Some(
                r#"{"outbounds":[{"protocol":"vless","settings":{"vnext":[{"address":"x.example.com","port":443}]}}],"inbounds":[]}"#
                    .into(),
            );
            p.outbound_json = r#"{"type":"custom","subtype":"xrayfullconfig","config":"{}"}"#.into();
            p
        });
        map.insert(2, {
            let mut p = make_node(2, 10, "sb-full", 20);
            p.profile_type = ProfileType::Custom;
            p.outbound.security = Some("full-config".into());
            p.outbound.raw_json = Some(
                r#"{"inbounds":[{"type":"mixed"}],"outbounds":[{"type":"direct","tag":"direct"}]}"#
                    .into(),
            );
            p.outbound_json = r#"{"type":"custom","subtype":"fullconfig","config":"{}"}"#.into();
            p
        });
        let selector_profile = Profile::new(99, 10, "auto", ProfileType::AutoSelector);
        let cfg = AutoSelectorConfig::new_for_group(10, "auto");
        let group = make_group(10, &[1, 2, 99]);
        let plan = plan_auto_selector(&selector_profile, &cfg, Some(&group), |id| {
            map.get(&id).cloned()
        });
        assert!(plan.error.is_none(), "{:?}", plan.error);
        assert_eq!(plan.eligible, 1);
        assert_eq!(plan.build, vec![1]);
        assert!(plan
            .skipped
            .iter()
            .any(|(s, n)| *s == AutoSelectorSkip::FullConfig && *n == 1));
    }

    #[test]
    fn xray_full_skipped_when_group_has_landing() {
        let mut map = HashMap::new();
        map.insert(1, {
            let mut p = make_node(1, 10, "xray-full", 30);
            p.profile_type = ProfileType::Custom;
            p.outbound_json =
                r#"{"type":"custom","subtype":"xrayfullconfig","config":"{}"}"#.into();
            p
        });
        let selector_profile = Profile::new(99, 10, "auto", ProfileType::AutoSelector);
        let cfg = AutoSelectorConfig::new_for_group(10, "auto");
        let mut group = make_group(10, &[1, 99]);
        group.landing_proxy_id = 5;
        let plan = plan_auto_selector(&selector_profile, &cfg, Some(&group), |id| {
            map.get(&id).cloned()
        });
        assert!(
            plan.skipped
                .iter()
                .any(|(s, _)| *s == AutoSelectorSkip::XrayFullChained),
            "{:?}",
            plan.skipped
        );
    }
}
