//! Auto Selector profile support (upstream Throne 1.2.3+ / 1.2.4).
//!
//! An Auto Selector tracks ordered groups instead of one server. Membership is
//! resolved at build/start time from `gids` (legacy `gid`) + filters; ranking prefers measured
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
    #[serde(default)]
    pub missing_groups: usize,
    pub eligible: usize,
    pub ranked_by_test: usize,
    pub skipped: Vec<(AutoSelectorSkip, usize)>,
    pub truncated: bool,
    pub pool_cap_used: i32,
    pub build_limit_used: i32,
    /// Failed-last-test members kept because excluding them would have emptied the pool (1.3.0-beta.3).
    pub kept_unavailable: usize,
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
    /// Ordered tracked groups; `gid` remains the first group for older readers.
    #[serde(default)]
    pub gids: Vec<GroupId>,
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
            gids: Vec::new(),
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
            gids: vec![gid],
            ..Self::default()
        }
    }

    pub fn normalize(&mut self) {
        if self.gids.is_empty() && self.gid >= 0 {
            self.gids.push(self.gid);
        }
        let mut seen = std::collections::HashSet::new();
        self.gids.retain(|gid| *gid >= 0 && seen.insert(*gid));
        self.gid = self.gids.first().copied().unwrap_or(-1);
        self.pool_cap = self.pool_cap.clamp(1, Self::MAX_POOL_CAP);
        self.build_limit = self.build_limit.clamp(1, Self::MAX_BUILD_LIMIT);
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
        self.sampling = self.sampling.clamp(2, 60);
        if self.tolerance_ms < 0 {
            self.tolerance_ms = 0;
        }
        if self.max_rtt_ms < 0 {
            self.max_rtt_ms = 0;
        }
        if self.expected > self.build_limit {
            self.expected = self.build_limit;
        }
        self.dial_retries = self.dial_retries.clamp(0, 5);
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
            "gids": cfg.gids,
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
            if v.get("gid").is_none() && v.get("gids").is_none() {
                return None;
            }
        } else if ty.is_empty() && v.get("gid").is_none() && v.get("gids").is_none() {
            return None;
        }
        let has_gids = v.get("gids").is_some();
        let mut cfg: Self = serde_json::from_value(v).ok()?;
        if has_gids {
            // An explicit empty list overrides a stale legacy gid.
            cfg.gid = cfg.gids.first().copied().unwrap_or(-1);
        }
        cfg.normalize();
        Some(cfg)
    }
}

/// Parse Auto Selector config from a profile, if it is one.
pub fn profile_auto_selector(profile: &Profile) -> Option<AutoSelectorConfig> {
    if profile.profile_type == ProfileType::AutoSelector {
        return AutoSelectorConfig::from_outbound_json(&profile.outbound_json).or_else(|| {
            Some(AutoSelectorConfig {
                name: profile.name.clone(),
                ..AutoSelectorConfig::default()
            })
        });
    }
    // Only accept non-typed JSON when it clearly looks like a selector.
    AutoSelectorConfig::from_outbound_json(&profile.outbound_json)
        .filter(|cfg| !cfg.gids.is_empty())
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn has_fresh_result(member: &Profile, selector: &AutoSelectorConfig, now: i64) -> bool {
    if member.latency_ms == 0 || selector.result_validity_mins <= 0 {
        return false;
    }
    // Rows written before the timestamp column existed carry 0: expired, not fresh.
    if member.latency_at <= 0 {
        return false;
    }
    (now - member.latency_at) <= i64::from(selector.result_validity_mins) * 60
}

fn effective_latency(member: &Profile, selector: &AutoSelectorConfig, now: i64) -> i32 {
    if has_fresh_result(member, selector, now) {
        member.latency_ms
    } else {
        0
    }
}

/// Upstream `AutoSelectorStartQuorum` after `Normalize`.
///
/// The native Start path still builds from stored results; this value is the
/// count of working members upstream waits for before the first start.
#[cfg_attr(not(test), allow(dead_code))]
pub fn start_quorum(selector: &AutoSelectorConfig) -> i32 {
    let mut cfg = selector.clone();
    cfg.normalize();
    cfg.expected.max(cfg.active_size).min(cfg.build_limit)
}

/// A member works when its effective latency is a positive fresh measurement.
#[cfg_attr(not(test), allow(dead_code))]
pub fn member_works(selector: &AutoSelectorConfig, member: &Profile, now: i64) -> bool {
    effective_latency(member, selector, now) > 0
}

/// Upstream `AutoSelectorBuildChanged`.
///
/// An errored plan is unchanged. A missing previously built id changes the
/// build. A newly built id changes it only when that member is known to work;
/// an unmeasured addition does not.
#[cfg_attr(not(test), allow(dead_code))]
pub fn selector_build_changed(
    last_built: &[ProfileId],
    next_build: &[ProfileId],
    plan_error: bool,
    works: impl Fn(ProfileId) -> bool,
) -> bool {
    if plan_error {
        return false;
    }
    let next: std::collections::HashSet<_> = next_build.iter().copied().collect();
    if last_built.iter().any(|id| !next.contains(id)) {
        return true;
    }
    let built: std::collections::HashSet<_> = last_built.iter().copied().collect();
    next_build
        .iter()
        .any(|id| !built.contains(id) && works(*id))
}

/// Upstream `AutoSelectorUnmeasuredCandidates`.
///
/// Stale ids and zero effective latency are still needed. A positive latency
/// increments the working count. A fresh failure is neither.
#[cfg_attr(not(test), allow(dead_code))]
pub fn unmeasured_candidates(
    eligible: &[ProfileId],
    stale: &[ProfileId],
    effective_latency_of: impl Fn(ProfileId) -> i32,
) -> (Vec<ProfileId>, i32) {
    let stale_set: std::collections::HashSet<_> = stale.iter().copied().collect();
    let mut needed = Vec::new();
    let mut working = 0i32;
    for &id in eligible {
        if stale_set.contains(&id) {
            needed.push(id);
            continue;
        }
        let latency = effective_latency_of(id);
        if latency == 0 {
            needed.push(id);
        } else if latency > 0 {
            working += 1;
        }
    }
    (needed, working)
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

struct CountryFilter {
    includes: Vec<String>,
    excludes: Vec<String>,
}

/// Split `country_filter` the way upstream `buildFilters` does.
///
/// A `!` prefix is an exclusion. Tokens are trimmed, then uppercased. An empty
/// token, including a bare `!`, is skipped. `Qt::SkipEmptyParts` drops empty
/// split pieces before that trim.
fn country_filter(filter: &str) -> CountryFilter {
    let mut includes = Vec::new();
    let mut excludes = Vec::new();
    for item in filter.split(',') {
        if item.is_empty() {
            continue;
        }
        let mut code = item.trim().to_string();
        if code.is_empty() {
            continue;
        }
        let exclude = code.starts_with('!');
        if exclude {
            code = code[1..].trim().to_string();
        }
        if code.is_empty() {
            continue;
        }
        let code = code.to_ascii_uppercase();
        if exclude {
            excludes.push(code);
        } else {
            includes.push(code);
        }
    }
    CountryFilter { includes, excludes }
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
    now: i64,
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
    let countries = country_filter(&selector.country_filter);
    if !countries.includes.is_empty() || !countries.excludes.is_empty() {
        // An exclusion cannot vouch for a profile that was never IP-tested.
        let country = member.test_country.trim().to_ascii_uppercase();
        if country.is_empty()
            || countries.excludes.iter().any(|code| code == &country)
            || (!countries.includes.is_empty()
                && !countries.includes.iter().any(|code| code == &country))
        {
            return Some(AutoSelectorSkip::CountryFilter);
        }
    }
    if selector.exclude_unavailable
        && member.latency_ms < 0
        && has_fresh_result(member, selector, now)
    {
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
    let groups: Vec<_> = group.into_iter().collect();
    plan_auto_selector_groups(selector_profile, selector, &groups, lookup)
}

/// Plan one pooled selector across groups, preserving order and deduplicating members.
pub fn plan_auto_selector_groups(
    selector_profile: &Profile,
    selector: &AutoSelectorConfig,
    groups: &[&Group],
    lookup: impl Fn(ProfileId) -> Option<Profile>,
) -> AutoSelectorPlan {
    let mut normalized = selector.clone();
    normalized.normalize();
    let selector = &normalized;
    let groups: Vec<_> = selector
        .gids
        .iter()
        .filter_map(|id| groups.iter().copied().find(|g| g.id == *id))
        .collect();
    let now = now_secs();
    let mut plan = AutoSelectorPlan {
        missing_groups: selector.gids.len().saturating_sub(groups.len()),
        pool_cap_used: selector.pool_cap,
        build_limit_used: selector.build_limit,
        ..Default::default()
    };

    if groups.is_empty() {
        plan.error = Some(
            if selector.gids.is_empty() {
                "Auto selector tracks no group"
            } else {
                "Auto selector points at a group that no longer exists"
            }
            .into(),
        );
        return plan;
    }

    let mut skip_counts: Vec<(AutoSelectorSkip, usize)> = Vec::new();
    let mut bump = |skip: AutoSelectorSkip| {
        if let Some((_, n)) = skip_counts.iter_mut().find(|(s, _)| *s == skip) {
            *n += 1;
        } else {
            skip_counts.push((skip, 1));
        }
    };

    let mut members: Vec<ProfileId> = Vec::new();
    let mut unavailable: Vec<ProfileId> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for group in groups {
        for &id in &group.profile_ids {
            if id == selector_profile.id || !seen.insert(id) {
                continue;
            }
            plan.members_in_group += 1;
            let member = lookup(id);
            if let Some(skip) = member_skip(member.as_ref(), selector, Some(group), now) {
                if skip == AutoSelectorSkip::Unavailable {
                    unavailable.push(id);
                }
                bump(skip);
                continue;
            }
            if let Some(m) = member.as_ref() {
                if effective_latency(m, selector, now) > 0 {
                    plan.ranked_by_test += 1;
                }
            }
            members.push(id);
        }
    }
    // Network outage: every remaining member failed its last test. Keep them
    // rather than emitting an empty pool that can never recover.
    if members.is_empty() && !unavailable.is_empty() {
        plan.kept_unavailable = unavailable.len();
        members = unavailable;
        skip_counts.retain(|(s, _)| *s != AutoSelectorSkip::Unavailable);
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
            .map(|m| effective_latency(&m, selector, now))
            .unwrap_or(0);
        let lb = lookup(b)
            .map(|m| effective_latency(&m, selector, now))
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
                    .map(|m| effective_latency(&m, selector, now) == 0)
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
    let groups: Vec<_> = group.into_iter().collect();
    rerank_auto_selector_groups(selector, selector_profile_id, &groups, lookup)
}

/// Re-rank members from every tracked group together.
pub fn rerank_auto_selector_groups(
    selector: &mut AutoSelectorConfig,
    selector_profile_id: ProfileId,
    groups: &[&Group],
    lookup: impl Fn(ProfileId) -> Option<Profile>,
) -> Vec<ProfileId> {
    let now = now_secs();
    let dummy = Profile::new(
        selector_profile_id,
        selector.gid,
        selector.name.clone(),
        ProfileType::AutoSelector,
    );
    let plan = plan_auto_selector_groups(&dummy, selector, groups, &lookup);
    if plan.error.is_some() {
        return Vec::new();
    }
    let mut members = plan.pool;
    members.sort_by(|&a, &b| {
        let la = lookup(a)
            .map(|m| effective_latency(&m, selector, now))
            .unwrap_or(0);
        let lb = lookup(b)
            .map(|m| effective_latency(&m, selector, now))
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
        // A non-zero result is fresh only when latency_at falls inside the window.
        if latency != 0 {
            p.latency_at = now_secs();
        }
        p.outbound.server = Some(format!("{name}.example.com"));
        p.outbound.server_port = Some(443);
        p
    }

    #[test]
    fn normalization_preserves_bounds_and_limits_build_to_pool() {
        for (pool, build, sampling, retries, expected) in [
            (i32::MIN, i32::MIN, i32::MIN, i32::MIN, (1, 1, 2, 0)),
            (1, 1, 2, 0, (1, 1, 2, 0)),
            (20, 10, 15, 3, (20, 10, 15, 3)),
            (3000, 500, 60, 5, (3000, 500, 60, 5)),
            (i32::MAX, i32::MAX, i32::MAX, i32::MAX, (3000, 500, 60, 5)),
            (3, 500, 10, 1, (3, 3, 10, 1)),
        ] {
            let mut cfg = AutoSelectorConfig {
                pool_cap: pool,
                build_limit: build,
                sampling,
                dial_retries: retries,
                ..AutoSelectorConfig::default()
            };
            cfg.normalize();
            assert_eq!(
                (
                    cfg.pool_cap,
                    cfg.build_limit,
                    cfg.sampling,
                    cfg.dial_retries
                ),
                expected
            );
        }
    }

    #[test]
    fn invalid_typed_selector_falls_back_to_profile_name() {
        let mut profile = Profile::new(1, 1, "Saved selector", ProfileType::AutoSelector);
        profile.outbound_json = "not json".into();
        let cfg = profile_auto_selector(&profile).expect("typed selector fallback");
        assert_eq!(cfg.name, "Saved selector");
        assert!(cfg.gids.is_empty());
        assert_eq!(cfg.pool_cap, AutoSelectorConfig::default().pool_cap);
    }

    #[test]
    fn untyped_selector_requires_a_valid_tracked_group() {
        let mut profile = Profile::new(1, 1, "Custom", ProfileType::Custom);
        for raw in ["{}", r#"{"gids":[]}"#, r#"{"gids":[-1]}"#, "not json"] {
            profile.outbound_json = raw.into();
            assert!(profile_auto_selector(&profile).is_none(), "{raw}");
        }
        profile.outbound_json = r#"{"gids":[7]}"#.into();
        assert_eq!(profile_auto_selector(&profile).unwrap().gids, vec![7]);
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

    #[test]
    fn keeps_unavailable_members_when_they_are_the_whole_pool() {
        let mut map = HashMap::new();
        map.insert(1, make_node(1, 10, "a", -1));
        map.insert(2, make_node(2, 10, "b", -1));
        let selector_profile = Profile::new(99, 10, "auto", ProfileType::AutoSelector);
        let mut cfg = AutoSelectorConfig::new_for_group(10, "auto");
        cfg.exclude_unavailable = true;
        let group = make_group(10, &[1, 2, 99]);
        let plan = plan_auto_selector(&selector_profile, &cfg, Some(&group), |id| {
            map.get(&id).cloned()
        });
        assert!(plan.error.is_none(), "{:?}", plan.error);
        assert_eq!(plan.kept_unavailable, 2);
        assert_eq!(plan.eligible, 2);
        assert_eq!(plan.pool.len(), 2);
        assert!(plan.pool.contains(&1) && plan.pool.contains(&2));
        assert!(!plan
            .skipped
            .iter()
            .any(|(s, _)| *s == AutoSelectorSkip::Unavailable));
    }

    fn plan_countries(filter: &str, nodes: Vec<Profile>) -> AutoSelectorPlan {
        let mut map = HashMap::new();
        let ids: Vec<_> = nodes.iter().map(|p| p.id).collect();
        for node in nodes {
            map.insert(node.id, node);
        }
        let selector_profile = Profile::new(99, 10, "auto", ProfileType::AutoSelector);
        let mut cfg = AutoSelectorConfig::new_for_group(10, "auto");
        cfg.country_filter = filter.into();
        let group = make_group(10, &ids);
        plan_auto_selector(&selector_profile, &cfg, Some(&group), |id| {
            map.get(&id).cloned()
        })
    }

    #[test]
    fn country_exclusion_skips_named_codes_and_unknowns() {
        let mut us = make_node(1, 10, "us", 40);
        us.test_country = "us".into();
        let mut jp = make_node(2, 10, "jp", 50);
        jp.test_country = " jp ".into();
        let mut unknown = make_node(3, 10, "unknown", 30);
        unknown.test_country = "  ".into();
        let plan = plan_countries("US, !JP, !, ,", vec![us, jp, unknown]);
        assert_eq!(plan.build, vec![1], "{:?}", plan.skipped);
        assert!(plan
            .skipped
            .iter()
            .any(|(s, n)| *s == AutoSelectorSkip::CountryFilter && *n == 2));
    }

    #[test]
    fn exclusion_only_keeps_a_known_country() {
        let mut de = make_node(1, 10, "de", 40);
        de.test_country = "de".into();
        let mut ru = make_node(2, 10, "ru", 20);
        ru.test_country = "RU".into();
        let plan = plan_countries("!ru", vec![de, ru]);
        assert_eq!(plan.build, vec![1]);
    }

    #[test]
    fn stale_latency_does_not_rank_or_exclude() {
        let mut fresh = make_node(1, 10, "fresh", 80);
        fresh.latency_at = now_secs();
        let mut expired = make_node(2, 10, "expired", 10);
        expired.latency_at =
            now_secs() - i64::from(AutoSelectorConfig::default().result_validity_mins) * 60 - 5;
        let mut failed_old = make_node(3, 10, "old-fail", -1);
        failed_old.latency_at = 0;
        let selector_profile = Profile::new(99, 10, "auto", ProfileType::AutoSelector);
        let mut cfg = AutoSelectorConfig::new_for_group(10, "auto");
        cfg.exclude_unavailable = true;
        let group = make_group(10, &[1, 2, 3]);
        let map = HashMap::from([(1, fresh), (2, expired.clone()), (3, failed_old)]);
        let plan = plan_auto_selector(&selector_profile, &cfg, Some(&group), |id| {
            map.get(&id).cloned()
        });
        assert_eq!(plan.build[0], 1);
        assert!(plan.build.contains(&2) && plan.build.contains(&3));
        assert!(!member_works(&cfg, &expired, now_secs()));
        assert!(!plan
            .skipped
            .iter()
            .any(|(s, _)| *s == AutoSelectorSkip::Unavailable));
    }

    #[test]
    fn start_quorum_follows_normalized_active_size() {
        let mut cfg = AutoSelectorConfig::default();
        cfg.expected = 10;
        cfg.active_size = 4;
        cfg.build_limit = 300;
        assert_eq!(start_quorum(&cfg), 10);

        cfg.expected = 20;
        cfg.active_size = 8;
        cfg.build_limit = 5;
        assert_eq!(start_quorum(&cfg), 5);

        cfg.expected = 0;
        cfg.active_size = 0;
        cfg.build_limit = 300;
        assert_eq!(start_quorum(&cfg), 1);
    }

    #[test]
    fn build_changed_ignores_unmeasured_additions() {
        assert!(!selector_build_changed(&[1], &[1, 2], true, |_| true));
        assert!(selector_build_changed(&[1, 2], &[1], false, |_| false));
        assert!(selector_build_changed(&[1], &[1, 2], false, |id| id == 2));
        assert!(!selector_build_changed(&[1], &[1, 2], false, |_| false));
        assert!(!selector_build_changed(&[1, 2], &[2, 1], false, |_| true));
    }

    #[test]
    fn unmeasured_candidates_count_stale_and_working() {
        let (needed, working) = unmeasured_candidates(&[1, 2, 3, 4], &[2], |id| match id {
            1 => 40,
            2 => 40,
            3 => 0,
            _ => -1,
        });
        assert_eq!(needed, vec![2, 3]);
        assert_eq!(working, 1);
    }
}
