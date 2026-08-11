use std::collections::{HashMap, HashSet, VecDeque};

use thiserror::Error;

use crate::auto_selector::{
    AutoSelectorConfig, AutoSelectorPlan, plan_auto_selector, profile_auto_selector,
    rerank_auto_selector_pool,
};
use crate::models::{
    AppSettings, CoreStatus, Group, GroupId, ParsedOutbound, Profile, ProfileId, ProfileType,
    RouteProfile, SystemMode, TrafficSnapshot,
};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("group {0} not found")]
    GroupNotFound(GroupId),
    #[error("profile {0} not found")]
    ProfileNotFound(ProfileId),
    #[error("no profile selected")]
    NoSelection,
    #[error("{0}")]
    Msg(String),
}

/// Result of replacing a group's profiles from a subscription refresh.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SubUpdateSummary {
    /// Profiles written into the group after update.
    pub total: usize,
    /// Nodes present in the new feed but not the old set (by server identity).
    pub added: usize,
    /// Nodes present in the old set but missing from the new feed.
    pub removed: usize,
    /// Nodes that match by server identity across old and new.
    pub kept: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionChange {
    pub profile_id: ProfileId,
    pub display: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubscriptionUpdateReport {
    pub added: Vec<SubscriptionChange>,
    pub updated: Vec<SubscriptionChange>,
    pub deleted: Vec<SubscriptionChange>,
    /// Running profiles kept because they are still in use (upstream 1.2.4 #1753).
    pub kept_in_use: Vec<SubscriptionChange>,
    pub unchanged: usize,
    pub result_order: Vec<ProfileId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileSortColumn {
    Type,
    Address,
    Name,
    TestResult,
    Traffic,
}

impl SubUpdateSummary {
    pub fn format_status(&self) -> String {
        format!(
            "{} profile(s) · +{} −{} · {} kept",
            self.total, self.added, self.removed, self.kept
        )
    }
}

/// Application state shared between UI and services.
#[derive(Debug, Clone)]
pub struct AppState {
    groups: HashMap<GroupId, Group>,
    profiles: HashMap<ProfileId, Profile>,
    /// Visible group tab order.
    group_order: Vec<GroupId>,
    active_group_id: GroupId,
    selected_profile_id: Option<ProfileId>,
    core_status: CoreStatus,
    system_mode: SystemMode,
    traffic: TrafficSnapshot,
    search_query: String,
    next_profile_id: ProfileId,
    next_group_id: GroupId,
    status_message: String,
    /// Ring buffer of recent status/log lines (for Logs panel + copy).
    log_lines: VecDeque<String>,
    settings: AppSettings,
    routes: HashMap<i64, RouteProfile>,
    route_order: Vec<i64>,
    active_route_id: Option<i64>,
    next_route_id: i64,
}

/// Ring buffer for Logs panel. Sized for core traffic (inbound/outbound) lines.
const LOG_HISTORY_CAP: usize = 1_000;

impl Default for AppState {
    fn default() -> Self {
        Self::with_demo_data()
    }
}

impl AppState {
    /// Empty store (for tests / real DB load later).
    pub fn empty() -> Self {
        Self {
            groups: HashMap::new(),
            profiles: HashMap::new(),
            group_order: Vec::new(),
            active_group_id: 0,
            selected_profile_id: None,
            core_status: CoreStatus::Stopped,
            system_mode: SystemMode::Off,
            traffic: TrafficSnapshot::default(),
            search_query: String::new(),
            next_profile_id: 1,
            next_group_id: 1,
            status_message: "Ready".into(),
            log_lines: VecDeque::new(),
            settings: AppSettings::default(),
            routes: HashMap::new(),
            route_order: Vec::new(),
            active_route_id: None,
            next_route_id: 1,
        }
    }

    /// Seed data so the GPUI shell is usable before persistence lands.
    pub fn with_demo_data() -> Self {
        let mut state = Self::empty();

        let g1 = state.add_group("Default");
        let g2 = state.add_group("Subscriptions");
        let rid = state.add_route(RouteProfile::new(0, "Default"));
        state.active_route_id = Some(rid);
        state.settings.current_route_id = rid;

        let samples = [
            ("HK-01 · Edge", ProfileType::Vless, 48),
            ("JP-Tokyo-A", ProfileType::Vmess, 72),
            ("US-West", ProfileType::Shadowsocks, 148),
            ("SG-Relay", ProfileType::Trojan, 95),
            ("Direct", ProfileType::Direct, 0),
            ("WG-Home", ProfileType::Wireguard, 33),
            ("HY2-EU", ProfileType::Hysteria2, 61),
            ("Custom JSON", ProfileType::Custom, -1),
        ];

        for (name, ty, latency) in samples {
            let id = state.add_profile(g1, name, ty);
            if let Some(p) = state.profiles.get_mut(&id) {
                p.latency_ms = latency;
                p.traffic_downlink = (latency.max(0) as i64) * 1_024_000;
                p.traffic_uplink = (latency.max(0) as i64) * 120_000;
            }
        }

        let sub_samples = [
            ("Sub · Node 1", ProfileType::Vless, 55),
            ("Sub · Node 2", ProfileType::Hysteria2, 88),
            ("Sub · Node 3", ProfileType::Tuic, 120),
        ];
        for (name, ty, latency) in sub_samples {
            let id = state.add_profile(g2, name, ty);
            if let Some(p) = state.profiles.get_mut(&id) {
                p.latency_ms = latency;
            }
        }

        state.active_group_id = g1;
        if let Some(first) = state
            .groups
            .get(&g1)
            .and_then(|g| g.profile_ids.first().copied())
        {
            state.selected_profile_id = Some(first);
        }
        state.push_log("Demo data loaded · core not connected");
        state
    }

    pub fn status_message(&self) -> &str {
        &self.status_message
    }

    /// Full log text for the Logs panel / clipboard (newest last).
    pub fn logs_text(&self) -> String {
        if self.log_lines.is_empty() {
            self.status_message.clone()
        } else {
            self.log_lines.iter().cloned().collect::<Vec<_>>().join("\n")
        }
    }

    pub fn set_status_message(&mut self, msg: impl Into<String>) {
        self.push_log(msg);
    }

    pub fn set_status_message_only(&mut self, msg: impl Into<String>) {
        self.status_message = msg.into();
    }

    /// Append a log line and update the status strip. Dedupes consecutive identical lines.
    pub fn push_log(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        if msg.is_empty() {
            return;
        }
        self.append_log_line(&msg);
        self.status_message = msg;
    }

    /// Append log line(s) without touching the status strip.
    ///
    /// Used for high-volume core traffic (inbound/outbound connection) logs so
    /// the status bar is not overwritten on every connection.
    pub fn push_log_only(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        if msg.is_empty() {
            return;
        }
        self.append_log_line(&msg);
    }

    fn append_log_line(&mut self, msg: &str) {
        // Dedupe consecutive identical messages (ignore timestamp prefix).
        let dup = self
            .log_lines
            .back()
            .and_then(|s| s.split_once("] ").map(|(_, body)| body == msg))
            .unwrap_or(false);
        if !dup {
            self.log_lines.push_back(format_log_line(msg));
            while self.log_lines.len() > LOG_HISTORY_CAP {
                self.log_lines.pop_front();
            }
        }
    }

    pub fn clear_logs(&mut self) {
        self.log_lines.clear();
        self.status_message = "Logs cleared".into();
        self.log_lines.push_back(format_log_line("Logs cleared"));
    }

    pub fn core_status(&self) -> &CoreStatus {
        &self.core_status
    }

    pub fn system_mode(&self) -> SystemMode {
        self.system_mode
    }

    pub fn traffic(&self) -> &TrafficSnapshot {
        &self.traffic
    }

    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    pub fn set_search_query(&mut self, q: impl Into<String>) {
        self.search_query = q.into();
    }

    pub fn active_group_id(&self) -> GroupId {
        self.active_group_id
    }

    pub fn selected_profile_id(&self) -> Option<ProfileId> {
        self.selected_profile_id
    }

    pub fn group_order(&self) -> &[GroupId] {
        &self.group_order
    }

    pub fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups.get(&id)
    }

    pub fn profile(&self, id: ProfileId) -> Option<&Profile> {
        self.profiles.get(&id)
    }

    pub fn set_active_group(&mut self, id: GroupId) -> Result<(), StoreError> {
        if !self.groups.contains_key(&id) {
            return Err(StoreError::GroupNotFound(id));
        }
        self.active_group_id = id;
        // Prefer keeping selection if it belongs to the new group; else first profile.
        let keep = self
            .selected_profile_id
            .and_then(|pid| self.profiles.get(&pid))
            .is_some_and(|p| p.group_id == id);
        if !keep {
            self.selected_profile_id = self
                .groups
                .get(&id)
                .and_then(|g| g.profile_ids.first().copied());
        }
        Ok(())
    }

    pub fn select_profile(&mut self, id: ProfileId) -> Result<(), StoreError> {
        let profile = self
            .profiles
            .get(&id)
            .ok_or(StoreError::ProfileNotFound(id))?;
        self.active_group_id = profile.group_id;
        self.selected_profile_id = Some(id);
        Ok(())
    }

    /// Profiles in the active group, filtered by search query.
    pub fn visible_profiles(&self) -> Vec<&Profile> {
        let Some(group) = self.groups.get(&self.active_group_id) else {
            return Vec::new();
        };
        let q = self.search_query.to_lowercase();
        group
            .profile_ids
            .iter()
            .filter_map(|id| self.profiles.get(id))
            .filter(|p| {
                if q.is_empty() {
                    return true;
                }
                p.name.to_lowercase().contains(&q)
                    || p.profile_type.as_str().contains(&q)
                    || p.test_country.to_lowercase().contains(&q)
            })
            .collect()
    }

    pub fn sort_active_group_profiles(
        &mut self,
        column: ProfileSortColumn,
        ascending: bool,
    ) -> Result<(), StoreError> {
        let group_id = self.active_group_id;
        let mut ids = self
            .groups
            .get(&group_id)
            .ok_or(StoreError::GroupNotFound(group_id))?
            .profile_ids
            .clone();
        ids.sort_by(|left_id, right_id| {
            let ordering = match (self.profiles.get(left_id), self.profiles.get(right_id)) {
                (Some(left), Some(right)) => compare_profiles(left, right, column),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => left_id.cmp(right_id),
            };
            if ascending {
                ordering
            } else {
                ordering.reverse()
            }
        });
        self.groups
            .get_mut(&group_id)
            .expect("active group checked above")
            .profile_ids = ids;
        Ok(())
    }

    pub fn add_group(&mut self, name: impl Into<String>) -> GroupId {
        let id = self.next_group_id;
        self.next_group_id += 1;
        self.groups.insert(id, Group::new(id, name));
        self.group_order.push(id);
        if self.active_group_id == 0 {
            self.active_group_id = id;
        }
        id
    }

    pub fn rename_group(&mut self, id: GroupId, name: impl Into<String>) -> Result<(), StoreError> {
        let g = self
            .groups
            .get_mut(&id)
            .ok_or(StoreError::GroupNotFound(id))?;
        g.name = name.into();
        Ok(())
    }

    /// Rename a profile (display name only; outbound config unchanged).
    pub fn rename_profile(
        &mut self,
        id: ProfileId,
        name: impl Into<String>,
    ) -> Result<(), StoreError> {
        let name = name.into();
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(StoreError::Msg("profile name cannot be empty".into()));
        }
        let new_name = trimmed.to_string();
        {
            let p = self
                .profiles
                .get_mut(&id)
                .ok_or(StoreError::ProfileNotFound(id))?;
            p.name = new_name.clone();
        }
        self.push_log(format!("Renamed profile → {new_name}"));
        Ok(())
    }

    pub fn set_group_url(&mut self, id: GroupId, url: impl Into<String>) -> Result<(), StoreError> {
        let g = self
            .groups
            .get_mut(&id)
            .ok_or(StoreError::GroupNotFound(id))?;
        g.url = url.into();
        Ok(())
    }

    /// Apply full group edit fields (upstream DialogEditGroup::accept).
    pub fn apply_group_edit(
        &mut self,
        id: GroupId,
        name: impl Into<String>,
        url: impl Into<String>,
        skip_auto_update: bool,
        auto_clear_unavailable: bool,
        front_proxy_id: i64,
        landing_proxy_id: i64,
    ) -> Result<(), StoreError> {
        let name = name.into();
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(StoreError::Msg("group name cannot be empty".into()));
        }
        let url = url.into();
        let g = self
            .groups
            .get_mut(&id)
            .ok_or(StoreError::GroupNotFound(id))?;
        // Existing subscription groups must keep a non-empty URL (upstream warning).
        if !g.url.is_empty() && url.trim().is_empty() {
            return Err(StoreError::Msg("Please input URL".into()));
        }
        g.name = trimmed.to_string();
        g.url = url.trim().to_string();
        g.skip_auto_update = skip_auto_update;
        g.auto_clear_unavailable = auto_clear_unavailable;
        g.front_proxy_id = front_proxy_id;
        g.landing_proxy_id = landing_proxy_id;
        Ok(())
    }

    /// Create a group from full edit fields (upstream New group → DialogEditGroup).
    pub fn create_group_from_edit(
        &mut self,
        name: impl Into<String>,
        url: impl Into<String>,
        skip_auto_update: bool,
        auto_clear_unavailable: bool,
        front_proxy_id: i64,
        landing_proxy_id: i64,
    ) -> Result<GroupId, StoreError> {
        let name = name.into();
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(StoreError::Msg("group name cannot be empty".into()));
        }
        let id = self.add_group(trimmed.to_string());
        let url = url.into().trim().to_string();
        if let Some(g) = self.groups.get_mut(&id) {
            g.url = url;
            g.skip_auto_update = skip_auto_update;
            g.auto_clear_unavailable = auto_clear_unavailable;
            g.front_proxy_id = front_proxy_id;
            g.landing_proxy_id = landing_proxy_id;
        }
        Ok(id)
    }

    pub fn delete_group(&mut self, id: GroupId) -> Result<(), StoreError> {
        if self.groups.len() <= 1 {
            return Err(StoreError::Msg("cannot delete the last group".into()));
        }
        let g = self
            .groups
            .remove(&id)
            .ok_or(StoreError::GroupNotFound(id))?;
        for pid in &g.profile_ids {
            self.profiles.remove(pid);
        }
        self.group_order.retain(|x| *x != id);
        if self.active_group_id == id {
            self.active_group_id = self.group_order.first().copied().unwrap_or(0);
            self.selected_profile_id = self
                .groups
                .get(&self.active_group_id)
                .and_then(|g| g.profile_ids.first().copied());
        }
        Ok(())
    }

    pub fn apply_basic_settings(
        &mut self,
        inbound_address: String,
        inbound_socks_port: i32,
        test_url: String,
        remote_dns: String,
        direct_dns: String,
        log_level: String,
        ruleset_mirror: crate::models::RulesetMirror,
        adblock_enable: bool,
    ) {
        self.settings.inbound_address = inbound_address;
        self.settings.inbound_socks_port = inbound_socks_port.max(1).min(65535);
        self.settings.test_latency_url = test_url;
        self.settings.remote_dns = remote_dns;
        self.settings.direct_dns = direct_dns;
        self.settings.log_level = log_level;
        self.settings.ruleset_mirror = ruleset_mirror;
        self.settings.adblock_enable = adblock_enable;
        self.push_log("Basic Settings saved");
    }

    pub fn add_profile(
        &mut self,
        group_id: GroupId,
        name: impl Into<String>,
        ty: ProfileType,
    ) -> ProfileId {
        let id = self.next_profile_id;
        self.next_profile_id += 1;
        let profile = Profile::new(id, group_id, name, ty);
        self.profiles.insert(id, profile);
        if let Some(g) = self.groups.get_mut(&group_id) {
            g.profile_ids.push(id);
        }
        id
    }

    /// Create an upstream-style Auto Selector profile that tracks `tracked_group_id`.
    ///
    /// The profile is stored in `home_group_id` (usually the active group). The
    /// selector draws members from `tracked_group_id` (often the same group).
    pub fn add_auto_selector(
        &mut self,
        home_group_id: GroupId,
        tracked_group_id: GroupId,
        name: impl Into<String>,
    ) -> Result<ProfileId, StoreError> {
        if !self.groups.contains_key(&home_group_id) {
            return Err(StoreError::GroupNotFound(home_group_id));
        }
        if !self.groups.contains_key(&tracked_group_id) {
            return Err(StoreError::GroupNotFound(tracked_group_id));
        }
        let name = name.into();
        let name = if name.trim().is_empty() {
            let gname = self
                .groups
                .get(&tracked_group_id)
                .map(|g| g.name.as_str())
                .unwrap_or("group");
            format!("Auto · {gname}")
        } else {
            name.trim().to_string()
        };
        let mut cfg = AutoSelectorConfig::new_for_group(tracked_group_id, &name);
        cfg.normalize();
        let id = self.add_profile(home_group_id, name, ProfileType::AutoSelector);
        if let Some(p) = self.profiles.get_mut(&id) {
            p.outbound_json = cfg.to_outbound_json();
            p.outbound = ParsedOutbound {
                tag: Some(cfg.name.clone()),
                raw_json: Some(p.outbound_json.clone()),
                ..Default::default()
            };
        }
        self.push_log(format!("Created Auto Selector · {}", cfg.name));
        Ok(id)
    }

    /// Plan membership for an Auto Selector profile.
    pub fn plan_auto_selector_profile(&self, profile_id: ProfileId) -> Result<AutoSelectorPlan, StoreError> {
        let profile = self
            .profiles
            .get(&profile_id)
            .ok_or(StoreError::ProfileNotFound(profile_id))?;
        let cfg = profile_auto_selector(profile).ok_or_else(|| {
            StoreError::Msg("Profile is not an Auto Selector".into())
        })?;
        let group = self.groups.get(&cfg.gid);
        Ok(plan_auto_selector(profile, &cfg, group, |id| {
            self.profiles.get(&id).cloned()
        }))
    }

    /// Persist ranked pool after URL testing members of an Auto Selector.
    pub fn rerank_auto_selector(&mut self, profile_id: ProfileId) -> Result<Vec<ProfileId>, StoreError> {
        let profile = self
            .profiles
            .get(&profile_id)
            .ok_or(StoreError::ProfileNotFound(profile_id))?
            .clone();
        let mut cfg = profile_auto_selector(&profile).ok_or_else(|| {
            StoreError::Msg("Profile is not an Auto Selector".into())
        })?;
        let group = self.groups.get(&cfg.gid).cloned();
        let ranked = rerank_auto_selector_pool(
            &mut cfg,
            profile_id,
            group.as_ref(),
            |id| self.profiles.get(&id).cloned(),
        );
        if let Some(p) = self.profiles.get_mut(&profile_id) {
            p.outbound_json = cfg.to_outbound_json();
            p.outbound.raw_json = Some(p.outbound_json.clone());
        }
        Ok(ranked)
    }

    /// Replace a profile's outbound_json (used after Auto Selector re-rank / last_built).
    pub fn update_profile_outbound_json(
        &mut self,
        profile_id: ProfileId,
        outbound_json: String,
    ) -> Result<(), StoreError> {
        let p = self
            .profiles
            .get_mut(&profile_id)
            .ok_or(StoreError::ProfileNotFound(profile_id))?;
        p.outbound_json = outbound_json.clone();
        p.outbound.raw_json = Some(outbound_json);
        Ok(())
    }

    /// Resolve Auto Selector build members (profiles) for core config generation.
    pub fn resolve_auto_selector_members(
        &self,
        profile_id: ProfileId,
    ) -> Result<(AutoSelectorConfig, AutoSelectorPlan, Vec<Profile>), StoreError> {
        let profile = self
            .profiles
            .get(&profile_id)
            .ok_or(StoreError::ProfileNotFound(profile_id))?;
        let cfg = profile_auto_selector(profile).ok_or_else(|| {
            StoreError::Msg("Profile is not an Auto Selector".into())
        })?;
        let group = self.groups.get(&cfg.gid);
        let plan = plan_auto_selector(profile, &cfg, group, |id| {
            self.profiles.get(&id).cloned()
        });
        if let Some(err) = &plan.error {
            return Err(StoreError::Msg(err.clone()));
        }
        let mut members = Vec::new();
        for id in &plan.build {
            if let Some(p) = self.profiles.get(id) {
                members.push(p.clone());
            }
        }
        if members.is_empty() {
            return Err(StoreError::Msg(
                "Auto selector produced no usable members".into(),
            ));
        }
        Ok((cfg, plan, members))
    }

    /// Toggle start/stop for the selected profile (domain-only; core client hooks later).
    pub fn toggle_selected(&mut self) -> Result<(), StoreError> {
        match &self.core_status {
            CoreStatus::Running { .. } => {
                self.core_status = CoreStatus::Stopped;
                self.traffic = TrafficSnapshot::default();
                self.push_log("Core stopped");
                Ok(())
            }
            CoreStatus::Stopped | CoreStatus::Error(_) => {
                let id = self.selected_profile_id.ok_or(StoreError::NoSelection)?;
                let name = self
                    .profiles
                    .get(&id)
                    .ok_or(StoreError::ProfileNotFound(id))?
                    .name
                    .clone();
                self.core_status = CoreStatus::Running {
                    profile_id: id,
                    profile_name: name.clone(),
                };
                self.push_log(format!("Started · {name} (simulated)"));
                Ok(())
            }
            CoreStatus::Starting | CoreStatus::Stopping => Ok(()),
        }
    }

    pub fn set_system_mode(&mut self, mode: SystemMode) {
        self.system_mode = mode;
        self.settings.system_proxy_enabled = matches!(mode, SystemMode::SystemProxy);
        self.settings.tun_mode_enabled = matches!(mode, SystemMode::VpnTun);
        // TUN and system proxy can both be on in upstream; we track primary mode
        // but keep independent checkboxes via set_spmode_*.
        let msg = match mode {
            SystemMode::Off => "System mode: off",
            SystemMode::SystemProxy => "System Proxy enabled",
            SystemMode::VpnTun => "Tun Mode enabled",
        };
        self.push_log(msg);
    }

    /// Upstream `checkBox_SystemProxy`.
    pub fn set_spmode_system_proxy(&mut self, enable: bool) {
        self.settings.system_proxy_enabled = enable;
        self.sync_system_mode_from_flags();
        self.push_log(if enable {
            "System Proxy enabled"
        } else {
            "System Proxy disabled"
        });
    }

    /// Upstream `checkBox_VPN` (Tun Mode).
    pub fn set_spmode_vpn(&mut self, enable: bool) {
        self.settings.tun_mode_enabled = enable;
        self.sync_system_mode_from_flags();
        self.push_log(if enable {
            "Tun Mode enabled"
        } else {
            "Tun Mode disabled"
        });
    }

    /// Upstream `system_dns` checkbox.
    pub fn set_system_dns(&mut self, enable: bool) {
        self.settings.system_dns_set = enable;
        self.push_log(if enable {
            "System DNS enabled"
        } else {
            "System DNS disabled"
        });
    }

    fn sync_system_mode_from_flags(&mut self) {
        self.system_mode = if self.settings.tun_mode_enabled {
            SystemMode::VpnTun
        } else if self.settings.system_proxy_enabled {
            SystemMode::SystemProxy
        } else {
            SystemMode::Off
        };
    }

    /// Running profile display for `label_running` (upstream refresh_status).
    pub fn running_label(&self) -> String {
        match &self.core_status {
            CoreStatus::Running { profile_name, .. } => profile_name.clone(),
            CoreStatus::Starting => "Starting…".into(),
            CoreStatus::Stopping => "Stopping…".into(),
            CoreStatus::Error(e) => format!("Error: {e}"),
            CoreStatus::Stopped => {
                if let Some(id) = self.selected_profile_id {
                    if let Some(p) = self.profiles.get(&id) {
                        return format!("{}  (stopped)", p.name);
                    }
                }
                "Not running".into()
            }
        }
    }

    /// Inbound summary for `label_inbound`.
    ///
    /// Upstream: `Mixed: {DisplayAddress(inbound_address, port)}` — show the
    /// stored listen host, including `::` / `0.0.0.0` when Allow LAN is on.
    /// IPv6 hosts are bracket-wrapped (`[::]:2080`) like `WrapIPV6Host`.
    pub fn inbound_label(&self) -> String {
        let s = &self.settings;
        let raw = s.inbound_address.trim();
        let host = if raw.is_empty() { "127.0.0.1" } else { raw };
        let display = display_listen_address(host, s.inbound_socks_port);
        format!("Mixed: {display}")
    }

    /// Speed lines for `label_speed`.
    pub fn speed_label(&self) -> String {
        if !self.core_status.is_running() {
            return String::new();
        }
        format!(
            "Proxy: {}↑ {}↓\nDirect: {}↑ {}↓",
            human_rate(self.traffic.proxy_up),
            human_rate(self.traffic.proxy_down),
            human_rate(self.traffic.direct_up),
            human_rate(self.traffic.direct_down),
        )
    }

    pub fn tick_traffic_demo(&mut self) {
        if self.core_status.is_running() {
            self.traffic.proxy_down += 12_000;
            self.traffic.proxy_up += 3_500;
            self.traffic.direct_down += 800;
            self.traffic.direct_up += 200;
        }
    }

    pub fn set_core_status(&mut self, status: CoreStatus) {
        if !status.is_running() {
            self.traffic = TrafficSnapshot::default();
        }
        self.core_status = status;
    }

    pub fn set_traffic(&mut self, traffic: TrafficSnapshot) {
        self.traffic = traffic;
    }

    pub fn update_live_traffic(&mut self, sample: TrafficSnapshot) -> bool {
        if !self.core_status.is_running() {
            return false;
        }

        let retain_rate = |next_rate: i64, current_rate: i64| {
            let next_rate = next_rate.max(0);
            if next_rate == 0 && current_rate > 0 {
                current_rate
            } else {
                next_rate
            }
        };
        let updated = TrafficSnapshot {
            proxy_up: retain_rate(sample.proxy_up, self.traffic.proxy_up),
            proxy_down: retain_rate(sample.proxy_down, self.traffic.proxy_down),
            direct_up: retain_rate(sample.direct_up, self.traffic.direct_up),
            direct_down: retain_rate(sample.direct_down, self.traffic.direct_down),
        };
        if updated == self.traffic {
            return false;
        }

        self.traffic = updated;
        true
    }

    /// Add core traffic deltas to the profile currently carrying proxy traffic.
    pub fn set_profile_traffic(&mut self, id: ProfileId, downlink: i64, uplink: i64) {
        if let Some(profile) = self.profiles.get_mut(&id) {
            profile.traffic_downlink = profile.traffic_downlink.saturating_add(downlink.max(0));
            profile.traffic_uplink = profile.traffic_uplink.saturating_add(uplink.max(0));
        }
    }

    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut AppSettings {
        &mut self.settings
    }

    pub fn set_settings(&mut self, settings: AppSettings) {
        self.settings = settings;
    }

    /// Replace store contents from persistence (keeps runtime fields reset).
    pub fn load_snapshot(
        &mut self,
        groups: Vec<Group>,
        profiles: Vec<Profile>,
        group_order: Vec<GroupId>,
        settings: AppSettings,
        routes: Vec<RouteProfile>,
    ) {
        self.groups = groups.into_iter().map(|g| (g.id, g)).collect();
        self.profiles = profiles.into_iter().map(|p| (p.id, p)).collect();
        // Prefer persisted tab order, then append any groups missing from it so
        // save never writes profiles whose gid is absent from the groups table.
        let mut order = group_order
            .into_iter()
            .filter(|id| self.groups.contains_key(id))
            .collect::<Vec<_>>();
        for id in self.groups.keys().copied() {
            if !order.contains(&id) {
                order.push(id);
            }
        }
        self.group_order = order;
        self.settings = settings;
        self.route_order = routes.iter().map(|r| r.id).collect();
        self.routes = routes.into_iter().map(|r| (r.id, r)).collect();
        self.next_group_id = self.groups.keys().copied().max().unwrap_or(0) + 1;
        self.next_profile_id = self.profiles.keys().copied().max().unwrap_or(0) + 1;
        self.next_route_id = self.routes.keys().copied().max().unwrap_or(0) + 1;
        // Drop profiles that reference a missing group (corrupt / partial DB).
        self.profiles
            .retain(|_, p| self.groups.contains_key(&p.group_id));
        for g in self.groups.values_mut() {
            g.profile_ids
                .retain(|id| self.profiles.contains_key(id));
        }
        self.active_group_id = self.group_order.first().copied().unwrap_or(0);
        self.selected_profile_id = self
            .groups
            .get(&self.active_group_id)
            .and_then(|g| g.profile_ids.first().copied());
        self.active_route_id = if self.settings.current_route_id > 0
            && self.routes.contains_key(&self.settings.current_route_id)
        {
            Some(self.settings.current_route_id)
        } else {
            self.route_order.first().copied()
        };
        self.core_status = CoreStatus::Stopped;
        self.system_mode = if self.settings.tun_mode_enabled {
            SystemMode::VpnTun
        } else if self.settings.system_proxy_enabled {
            SystemMode::SystemProxy
        } else {
            SystemMode::Off
        };
        self.push_log(format!(
            "Loaded {} groups · {} profiles · {} routes",
            self.groups.len(),
            self.profiles.len(),
            self.routes.len()
        ));
    }

    /// Ensure every group is listed in `group_order` and every profile's `group_id`
    /// exists — required for SQLite `profiles.gid → groups.id` on full rewrite save.
    pub fn sanitize_group_profile_refs(&mut self) {
        for id in self.groups.keys().copied().collect::<Vec<_>>() {
            if !self.group_order.contains(&id) {
                self.group_order.push(id);
            }
        }
        self.group_order.retain(|id| self.groups.contains_key(id));
        // Re-home or drop orphan profiles.
        let fallback = self.group_order.first().copied();
        let orphan_ids: Vec<ProfileId> = self
            .profiles
            .iter()
            .filter(|(_, p)| !self.groups.contains_key(&p.group_id))
            .map(|(id, _)| *id)
            .collect();
        for id in orphan_ids {
            if let Some(gid) = fallback {
                if let Some(p) = self.profiles.get_mut(&id) {
                    p.group_id = gid;
                }
                if let Some(g) = self.groups.get_mut(&gid) {
                    if !g.profile_ids.contains(&id) {
                        g.profile_ids.push(id);
                    }
                }
            } else {
                self.profiles.remove(&id);
            }
        }
        // Keep each group's profile_ids list aligned with live profiles.
        for g in self.groups.values_mut() {
            g.profile_ids
                .retain(|id| self.profiles.contains_key(id));
        }
        for (id, p) in &self.profiles {
            if let Some(g) = self.groups.get_mut(&p.group_id) {
                if !g.profile_ids.contains(id) {
                    g.profile_ids.push(*id);
                }
            }
        }
    }

    pub fn add_route(&mut self, mut route: RouteProfile) -> i64 {
        let id = if route.id > 0 {
            route.id
        } else {
            let id = self.next_route_id;
            self.next_route_id += 1;
            id
        };
        route.id = id;
        if !self.route_order.contains(&id) {
            self.route_order.push(id);
        }
        self.routes.insert(id, route);
        if self.active_route_id.is_none() {
            self.active_route_id = Some(id);
            self.settings.current_route_id = id;
        }
        id
    }

    pub fn import_routes(&mut self, routes: impl IntoIterator<Item = RouteProfile>) -> usize {
        let mut n = 0usize;
        for mut r in routes {
            r.id = 0; // assign fresh ids
            self.add_route(r);
            n += 1;
        }
        if n > 0 {
            self.push_log(format!("Imported {n} route profile(s)"));
        }
        n
    }

    pub fn all_routes(&self) -> Vec<&RouteProfile> {
        self.route_order
            .iter()
            .filter_map(|id| self.routes.get(id))
            .collect()
    }

    pub fn active_route(&self) -> Option<&RouteProfile> {
        self.active_route_id.and_then(|id| self.routes.get(&id))
    }

    pub fn set_active_route(&mut self, id: i64) -> Result<(), StoreError> {
        if !self.routes.contains_key(&id) {
            return Err(StoreError::Msg(format!("route {id} not found")));
        }
        self.active_route_id = Some(id);
        self.settings.current_route_id = id;
        if let Some(r) = self.routes.get(&id) {
            self.push_log(format!("Active route · {}", r.name));
        }
        Ok(())
    }

    pub fn cycle_active_route(&mut self) {
        if self.route_order.is_empty() {
            return;
        }
        let cur = self.active_route_id.unwrap_or(self.route_order[0]);
        let idx = self
            .route_order
            .iter()
            .position(|id| *id == cur)
            .unwrap_or(0);
        let next = self.route_order[(idx + 1) % self.route_order.len()];
        let _ = self.set_active_route(next);
    }

    pub fn route_mut(&mut self, id: i64) -> Option<&mut RouteProfile> {
        self.routes.get_mut(&id)
    }

    pub fn update_route_meta(
        &mut self,
        id: i64,
        name: String,
        remote_url: String,
        auto_update: bool,
        default_outbound: crate::models::DefaultOutbound,
    ) -> Result<(), StoreError> {
        {
            let r = self
                .routes
                .get_mut(&id)
                .ok_or_else(|| StoreError::Msg(format!("route {id} not found")))?;
            r.name = name;
            r.remote_url = remote_url.clone();
            r.auto_update = auto_update;
            r.default_outbound = default_outbound;
            if !remote_url.trim().is_empty() {
                r.is_remote = true;
            }
        }
        let label = self
            .routes
            .get(&id)
            .map(|r| r.name.clone())
            .unwrap_or_default();
        self.push_log(format!("Route «{label}» saved"));
        Ok(())
    }

    /// Replace rules/raw body of a route after remote fetch / import.
    pub fn replace_route_content(&mut self, id: i64, incoming: RouteProfile) -> Result<(), StoreError> {
        let label = {
            let r = self
                .routes
                .get_mut(&id)
                .ok_or_else(|| StoreError::Msg(format!("route {id} not found")))?;
            let name = r.name.clone();
            let remote_url = r.remote_url.clone();
            let auto_update = r.auto_update;
            *r = incoming;
            r.id = id;
            if r.name.is_empty() {
                r.name = name;
            }
            r.remote_url = remote_url;
            r.auto_update = auto_update;
            r.is_remote = !r.remote_url.is_empty();
            r.remote_last_update = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            r.name.clone()
        };
        self.push_log(format!("Route «{label}» content updated"));
        Ok(())
    }

    /// Delete a route profile. Refuses when it is the last remaining profile.
    pub fn delete_route(&mut self, id: i64) -> Result<(), StoreError> {
        if self.routes.len() <= 1 {
            return Err(StoreError::Msg(
                "Routing Profiles cannot be empty, try adding another profile or editing this one"
                    .into(),
            ));
        }
        if !self.routes.contains_key(&id) {
            return Err(StoreError::Msg(format!("route {id} not found")));
        }
        self.routes.remove(&id);
        self.route_order.retain(|x| *x != id);
        if self.active_route_id == Some(id) {
            let next = self.route_order.first().copied();
            self.active_route_id = next;
            self.settings.current_route_id = next.unwrap_or(-1);
        }
        self.push_log(format!("Route {id} deleted"));
        Ok(())
    }

    /// Clone a route profile (`name + " clone"`, fresh id).
    pub fn clone_route(&mut self, id: i64) -> Result<i64, StoreError> {
        let mut copy = self
            .routes
            .get(&id)
            .cloned()
            .ok_or_else(|| StoreError::Msg(format!("route {id} not found")))?;
        copy.name = format!("{} clone", copy.name);
        copy.id = 0;
        Ok(self.add_route(copy))
    }

    /// Insert or replace a full route profile (editor save). Keeps id when > 0.
    pub fn upsert_route(&mut self, mut route: RouteProfile) -> i64 {
        if route.id > 0 && self.routes.contains_key(&route.id) {
            let id = route.id;
            self.routes.insert(id, route);
            id
        } else {
            route.id = 0;
            self.add_route(route)
        }
    }

    /// Replace the entire route set from a routing-settings draft (accept()).
    /// `active_id` becomes the current route; ids ≤ 0 get fresh numbers.
    pub fn commit_routes(
        &mut self,
        routes: Vec<RouteProfile>,
        active_id: i64,
    ) -> Result<(), StoreError> {
        if routes.is_empty() {
            return Err(StoreError::Msg("Routing profile cannot be empty".into()));
        }
        self.routes.clear();
        self.route_order.clear();
        let mut max_id = 0i64;
        let mut mapped_active = None;
        for mut r in routes {
            let wanted = r.id;
            if r.id <= 0 {
                r.id = 0;
            }
            let id = self.add_route(r);
            max_id = max_id.max(id);
            if wanted > 0 && wanted == active_id {
                mapped_active = Some(id);
            }
        }
        self.next_route_id = max_id + 1;
        let active = mapped_active
            .or_else(|| {
                if self.routes.contains_key(&active_id) {
                    Some(active_id)
                } else {
                    self.route_order.first().copied()
                }
            })
            .unwrap_or(1);
        self.active_route_id = Some(active);
        self.settings.current_route_id = active;
        Ok(())
    }

    /// Apply routing-dialog settings fields (DNS / Hijack / Warp / Common).
    pub fn apply_routing_dialog_settings(&mut self, s: AppSettings) {
        // Keep non-routing fields from current settings; overlay routing-related ones.
        let cur = self.settings.clone();
        self.settings = AppSettings {
            // preserve main/basic settings not owned by this dialog
            inbound_socks_port: cur.inbound_socks_port,
            inbound_address: cur.inbound_address,
            test_latency_url: cur.test_latency_url,
            vpn_strict_route: cur.vpn_strict_route,
            vpn_mtu: cur.vpn_mtu,
            vpn_tun_ipv4_cidr: cur.vpn_tun_ipv4_cidr,
            disable_private_range_bypass: cur.disable_private_range_bypass,
            sub_show_change_popup: cur.sub_show_change_popup,
            allow_stopping_active_profile: cur.allow_stopping_active_profile,
            show_config_security: cur.show_config_security,
            remember_id: cur.remember_id,
            remember_enable: cur.remember_enable,
            start_with_system: cur.start_with_system,
            system_proxy_enabled: cur.system_proxy_enabled,
            tun_mode_enabled: cur.tun_mode_enabled,
            system_dns_set: cur.system_dns_set,
            theme: cur.theme,
            log_level: cur.log_level,
            adblock_enable: cur.adblock_enable,
            hk_start_stop: cur.hk_start_stop,
            hk_import: cur.hk_import,
            hk_save: cur.hk_save,
            hk_url_test: cur.hk_url_test,
            hk_copy_logs: cur.hk_copy_logs,
            // from dialog draft
            remote_dns: s.remote_dns,
            direct_dns: s.direct_dns,
            current_route_id: s.current_route_id,
            ruleset_mirror: s.ruleset_mirror,
            remote_dns_strategy: s.remote_dns_strategy,
            direct_dns_strategy: s.direct_dns_strategy,
            dns_cache_capacity: s.dns_cache_capacity,
            dns_disable_cache: s.dns_disable_cache,
            dns_disable_expire: s.dns_disable_expire,
            dns_reverse_mapping: s.dns_reverse_mapping,
            enable_dns_routing: s.enable_dns_routing,
            use_dns_object: s.use_dns_object,
            dns_object: s.dns_object,
            dns_final_out: s.dns_final_out,
            resolve_domain_strategy: s.resolve_domain_strategy,
            default_domain_strategy: s.default_domain_strategy,
            core_box_underlying_dns: s.core_box_underlying_dns,
            fake_dns: s.fake_dns,
            enable_dns_server: s.enable_dns_server,
            dns_server_listen_port: s.dns_server_listen_port,
            dns_v4_resp: s.dns_v4_resp,
            dns_v6_resp: s.dns_v6_resp,
            dns_server_rules: s.dns_server_rules,
            dns_server_listen_lan: s.dns_server_listen_lan,
            enable_redirect: s.enable_redirect,
            redirect_listen_address: s.redirect_listen_address,
            redirect_listen_port: s.redirect_listen_port,
            enable_warp: s.enable_warp,
            warp_ep: s.warp_ep,
            warp_private_key: s.warp_private_key,
            warp_public_key: s.warp_public_key,
            warp_ifc_addrs: s.warp_ifc_addrs,
            warp_reserved: s.warp_reserved,
        };
        self.push_log("Routing settings applied");
    }

    pub fn apply_tun_settings(
        &mut self,
        vpn_mtu: i32,
        vpn_strict_route: bool,
        disable_private_range_bypass: bool,
        vpn_tun_ipv4_cidr: Option<String>,
    ) {
        self.settings.vpn_mtu = vpn_mtu.clamp(1280, 65535);
        self.settings.vpn_strict_route = vpn_strict_route;
        self.settings.disable_private_range_bypass = disable_private_range_bypass;
        if let Some(cidr) = vpn_tun_ipv4_cidr {
            let t = cidr.trim();
            if !t.is_empty() {
                self.settings.vpn_tun_ipv4_cidr = t.to_string();
            }
        }
        self.push_log(format!(
            "Tun settings · mtu={} strict={} bypass_private={} addr={}",
            self.settings.vpn_mtu,
            vpn_strict_route,
            !disable_private_range_bypass,
            self.settings.vpn_tun_ipv4_cidr
        ));
    }

    pub fn apply_hotkey_settings(
        &mut self,
        start_stop: String,
        import: String,
        save: String,
        url_test: String,
        copy_logs: String,
    ) {
        self.settings.hk_start_stop = start_stop;
        self.settings.hk_import = import;
        self.settings.hk_save = save;
        self.settings.hk_url_test = url_test;
        self.settings.hk_copy_logs = copy_logs;
        self.push_log("Hotkey settings saved (labels; rebind on next launch wave)");
    }

    pub fn all_groups(&self) -> Vec<&Group> {
        self.group_order
            .iter()
            .filter_map(|id| self.groups.get(id))
            .collect()
    }

    /// All groups by id (including any not currently in tab order).
    pub fn groups_map(&self) -> &HashMap<GroupId, Group> {
        &self.groups
    }

    pub fn all_profiles(&self) -> Vec<&Profile> {
        self.profiles.values().collect()
    }

    /// Insert imported profiles into the active group.
    pub fn import_profiles(
        &mut self,
        items: impl IntoIterator<Item = (String, ProfileType, ParsedOutbound, bool)>,
    ) -> usize {
        let gid = if self.active_group_id == 0 {
            self.add_group("Imported")
        } else {
            self.active_group_id
        };
        let mut n = 0usize;
        for (name, ty, outbound, insecure) in items {
            let id = self.next_profile_id;
            self.next_profile_id += 1;
            let outbound_json = outbound.to_db_json(ty);
            let mut profile = Profile::new(id, gid, name, ty);
            profile.outbound = outbound;
            profile.outbound_json = outbound_json;
            profile.insecure = insecure;
            self.profiles.insert(id, profile);
            if let Some(g) = self.groups.get_mut(&gid) {
                g.profile_ids.push(id);
            }
            n += 1;
        }
        if n > 0 {
            self.active_group_id = gid;
            self.push_log(format!("Imported {n} profile(s)"));
        }
        n
    }

    pub fn delete_selected_profiles(&mut self, ids: &[ProfileId]) {
        for id in ids {
            if let Some(p) = self.profiles.remove(id) {
                if let Some(g) = self.groups.get_mut(&p.group_id) {
                    g.profile_ids.retain(|x| x != id);
                }
            }
            if self.selected_profile_id == Some(*id) {
                self.selected_profile_id = None;
            }
        }
    }

    /// Apply URL-test latency for one profile (`latency_ms`; negative = fail).
    pub fn set_profile_latency(&mut self, id: ProfileId, latency_ms: i32) {
        if let Some(p) = self.profiles.get_mut(&id) {
            p.latency_ms = latency_ms;
        }
    }

    pub fn set_profile_ip_country(&mut self, id: ProfileId, ip: &str, country: &str) {
        if let Some(p) = self.profiles.get_mut(&id) {
            p.ip_out = ip.to_string();
            p.test_country = country.to_string();
        }
    }

    pub fn set_profile_speeds(
        &mut self,
        id: ProfileId,
        dl: &str,
        ul: &str,
        latency_ms: i32,
    ) {
        if let Some(p) = self.profiles.get_mut(&id) {
            p.download_speed = dl.to_string();
            p.upload_speed = ul.to_string();
            if latency_ms > 0 {
                p.latency_ms = latency_ms;
            }
        }
    }

    /// Apply a batch of URL-test results. `latency_ms < 0` or non-empty error → fail.
    pub fn apply_url_test_results(&mut self, results: &[(ProfileId, i32, &str)]) -> usize {
        let mut n = 0usize;
        for (id, lat, err) in results {
            if *id <= 0 {
                continue;
            }
            let v = if !err.is_empty() || *lat < 0 {
                -1
            } else {
                *lat
            };
            if let Some(p) = self.profiles.get_mut(id) {
                p.latency_ms = v;
                n += 1;
            }
        }
        if n > 0 {
            self.push_log(format!("URL Test updated {n} profile(s)"));
        }
        n
    }

    /// Remove duplicate profiles in a group (same type + server + port + identity).
    /// Keeps the first occurrence (lowest id order in group list).
    pub fn remove_duplicates_in_group(&mut self, group_id: GroupId) -> usize {
        let Some(g) = self.groups.get(&group_id) else {
            return 0;
        };
        let ids = g.profile_ids.clone();
        let mut seen = std::collections::HashSet::new();
        let mut drop = Vec::new();
        for id in ids {
            let Some(p) = self.profiles.get(&id) else {
                continue;
            };
            let key = profile_dedupe_key(p);
            if !seen.insert(key) {
                drop.push(id);
            }
        }
        let n = drop.len();
        if n > 0 {
            self.delete_selected_profiles(&drop);
            self.push_log(format!("Removed {n} duplicate profile(s)"));
        } else {
            self.push_log("No duplicates found");
        }
        n
    }

    /// IDs of profiles that failed the last URL test (`latency_ms < 0`).
    pub fn unavailable_profile_ids_in_group(&self, group_id: GroupId) -> Vec<ProfileId> {
        let Some(g) = self.groups.get(&group_id) else {
            return Vec::new();
        };
        g
            .profile_ids
            .iter()
            .copied()
            .filter(|id| {
                self.profiles
                    .get(id)
                    .map(|p| p.latency_ms < 0)
                    .unwrap_or(false)
            })
            .collect()
    }

    /// Delete profiles that failed the last URL test (`latency_ms < 0`).
    pub fn remove_unavailable_in_group(&mut self, group_id: GroupId) -> usize {
        let drop = self.unavailable_profile_ids_in_group(group_id);
        let n = drop.len();
        if n > 0 {
            self.delete_selected_profiles(&drop);
            self.push_log(format!("Removed {n} unavailable profile(s)"));
        } else {
            self.push_log("No unavailable profiles (run URL Test first)");
        }
        n
    }

    pub fn remove_invalid_in_group(&mut self, group_id: GroupId) -> usize {
        let Some(group) = self.groups.get(&group_id) else {
            return 0;
        };
        let drop: Vec<_> = group
            .profile_ids
            .iter()
            .copied()
            .filter(|id| {
                self.profiles
                    .get(id)
                    .is_some_and(profile_is_structurally_invalid)
            })
            .collect();
        let count = drop.len();
        if count > 0 {
            self.delete_selected_profiles(&drop);
            self.push_log(format!("Removed {count} structurally invalid profile(s)"));
        } else {
            self.push_log("No structurally invalid profiles found");
        }
        count
    }

    /// Delete profiles flagged insecure when security display is meaningful.
    pub fn remove_insecure_in_group(&mut self, group_id: GroupId) -> usize {
        let Some(g) = self.groups.get(&group_id) else {
            return 0;
        };
        let drop: Vec<_> = g
            .profile_ids
            .iter()
            .copied()
            .filter(|id| self.profiles.get(id).map(|p| p.insecure).unwrap_or(false))
            .collect();
        let n = drop.len();
        if n > 0 {
            self.delete_selected_profiles(&drop);
            self.push_log(format!("Removed {n} insecure profile(s)"));
        } else {
            self.push_log("No insecure profiles found");
        }
        n
    }

    /// Replace all profiles in `group_id` with a fresh import set (subscription update).
    pub fn replace_group_profiles(
        &mut self,
        group_id: GroupId,
        items: impl IntoIterator<Item = (String, ProfileType, ParsedOutbound, bool)>,
    ) -> Result<SubUpdateSummary, StoreError> {
        let g = self
            .groups
            .get(&group_id)
            .ok_or(StoreError::GroupNotFound(group_id))?;
        let old: Vec<_> = g.profile_ids.clone();
        let old_keys: HashSet<String> = old
            .iter()
            .filter_map(|id| self.profiles.get(id))
            .map(profile_dedupe_key)
            .collect();

        let new_items: Vec<_> = items.into_iter().collect();
        let mut new_keys = HashSet::new();
        for (name, ty, outbound, _) in &new_items {
            // Build a temporary profile only for identity hashing.
            let mut tmp = Profile::new(0, group_id, name.clone(), *ty);
            tmp.outbound = outbound.clone();
            new_keys.insert(profile_dedupe_key(&tmp));
        }
        let kept = old_keys.intersection(&new_keys).count();
        let added = new_keys.difference(&old_keys).count();
        let removed = old_keys.difference(&new_keys).count();

        self.delete_selected_profiles(&old);
        let mut n = 0usize;
        for (name, ty, outbound, insecure) in new_items {
            let id = self.next_profile_id;
            self.next_profile_id += 1;
            let outbound_json = outbound.to_db_json(ty);
            let mut profile = Profile::new(id, group_id, name, ty);
            profile.outbound = outbound;
            profile.outbound_json = outbound_json;
            profile.insecure = insecure;
            self.profiles.insert(id, profile);
            if let Some(g) = self.groups.get_mut(&group_id) {
                g.profile_ids.push(id);
            }
            n += 1;
        }
        self.active_group_id = group_id;
        self.selected_profile_id = self
            .groups
            .get(&group_id)
            .and_then(|g| g.profile_ids.first().copied());
        let summary = SubUpdateSummary {
            total: n,
            added,
            removed,
            kept,
        };
        self.push_log(format!("Subscription updated · {}", summary.format_status()));
        Ok(summary)
    }

    /// Apply one successfully parsed subscription snapshot while retaining local
    /// measurements for profiles whose stable remote identity still exists.
    pub fn apply_subscription_snapshot(
        &mut self,
        group_id: GroupId,
        items: Vec<(String, ProfileType, ParsedOutbound, bool)>,
        info: String,
        updated_at: i64,
    ) -> Result<SubscriptionUpdateReport, StoreError> {
        let old_ids = self
            .groups
            .get(&group_id)
            .ok_or(StoreError::GroupNotFound(group_id))?
            .profile_ids
            .clone();
        let mut available: HashMap<String, VecDeque<ProfileId>> = HashMap::new();
        for id in &old_ids {
            if let Some(profile) = self.profiles.get(id) {
                available
                    .entry(profile_identity_key(profile))
                    .or_default()
                    .push_back(*id);
            }
        }

        let mut protected_running_id = None;
        if let CoreStatus::Running { profile_id, .. } = &self.core_status {
            if old_ids.contains(profile_id) {
                protected_running_id = Some(*profile_id);
            }
        }

        let mut report = SubscriptionUpdateReport::default();
        let mut retained = HashSet::new();
        for (name, profile_type, outbound, insecure) in items {
            let identity = subscription_item_identity_key(profile_type, &outbound);
            let exact_id = available.get(&identity).and_then(|ids| {
                ids.iter().copied().find(|id| {
                    self.profiles.get(id).is_some_and(|profile| {
                        profile.name == name
                            && profile.profile_type == profile_type
                            && profile.outbound == outbound
                            && profile.insecure == insecure
                    })
                })
            });
            let same_name_id = available.get(&identity).and_then(|ids| {
                ids.iter().copied().find(|id| {
                    self.profiles
                        .get(id)
                        .is_some_and(|profile| profile.name == name)
                })
            });
            let matches_running = protected_running_id.is_some_and(|running_id| {
                !retained.contains(&running_id)
                    && self.profiles.get(&running_id).is_some_and(|running| {
                        profile_identity_key(running) == identity
                    })
            });
            let existing_id = if let Some(exact_id) = exact_id {
                if let Some(ids) = available.get_mut(&identity) {
                    if let Some(position) = ids.iter().position(|id| *id == exact_id) {
                        ids.remove(position);
                    }
                }
                Some(exact_id)
            } else if let Some(same_name_id) = same_name_id {
                if let Some(ids) = available.get_mut(&identity) {
                    if let Some(position) = ids.iter().position(|id| *id == same_name_id) {
                        ids.remove(position);
                    }
                }
                Some(same_name_id)
            } else if matches_running {
                let running_id = protected_running_id.expect("running ID checked above");
                if let Some(ids) = available.get_mut(&identity) {
                    if let Some(position) = ids.iter().position(|id| *id == running_id) {
                        ids.remove(position);
                    }
                }
                Some(running_id)
            } else {
                available.get_mut(&identity).and_then(VecDeque::pop_front)
            };
            if let Some(id) = existing_id {
                retained.insert(id);
                let profile = self
                    .profiles
                    .get_mut(&id)
                    .ok_or(StoreError::ProfileNotFound(id))?;
                // Keep gid aligned with the subscription group (save FK safety).
                profile.group_id = group_id;
                let changed = profile.name != name
                    || profile.profile_type != profile_type
                    || profile.outbound != outbound
                    || profile.insecure != insecure;
                if changed {
                    profile.name = name;
                    profile.profile_type = profile_type;
                    profile.outbound_json = outbound.to_db_json(profile_type);
                    profile.outbound = outbound;
                    profile.insecure = insecure;
                    report.updated.push(subscription_change(profile));
                } else {
                    report.unchanged += 1;
                }
                report.result_order.push(id);
            } else {
                let id = self.next_profile_id;
                self.next_profile_id += 1;
                let mut profile = Profile::new(id, group_id, name, profile_type);
                profile.outbound_json = outbound.to_db_json(profile_type);
                profile.outbound = outbound;
                profile.insecure = insecure;
                report.added.push(subscription_change(&profile));
                self.profiles.insert(id, profile);
                retained.insert(id);
                report.result_order.push(id);
            }
        }

        if let Some(running_id) = protected_running_id {
            if !retained.contains(&running_id) {
                // Remote snapshot dropped the running profile; keep it so Start
                // state and Auto Selector pools stay consistent (#1753 / 1.2.4).
                retained.insert(running_id);
                report.result_order.push(running_id);
                if let Some(profile) = self.profiles.get(&running_id) {
                    report.kept_in_use.push(subscription_change(profile));
                } else {
                    report.unchanged += 1;
                }
            }
        }

        for id in old_ids {
            if !retained.contains(&id) {
                if let Some(profile) = self.profiles.remove(&id) {
                    report.deleted.push(subscription_change(&profile));
                }
            }
        }
        let group = self
            .groups
            .get_mut(&group_id)
            .ok_or(StoreError::GroupNotFound(group_id))?;
        group.profile_ids = report.result_order.clone();
        group.info = info;
        group.sub_last_update = updated_at;
        if self.selected_profile_id.is_some_and(|id| !self.profiles.contains_key(&id)) {
            self.selected_profile_id = group.profile_ids.first().copied();
        }
        if let CoreStatus::Running {
            profile_id,
            profile_name,
        } = &mut self.core_status
        {
            if let Some(profile) = self.profiles.get(profile_id) {
                *profile_name = profile.name.clone();
            }
        }
        Ok(report)
    }

    pub fn clear_test_results_in_group(&mut self, group_id: GroupId) {
        let Some(g) = self.groups.get(&group_id) else {
            return;
        };
        let ids = g.profile_ids.clone();
        for id in ids {
            if let Some(p) = self.profiles.get_mut(&id) {
                p.latency_ms = 0;
                p.download_speed.clear();
                p.upload_speed.clear();
                p.test_country.clear();
            }
        }
        self.push_log("Test results cleared");
    }
}

fn profile_dedupe_key(p: &Profile) -> String {
    let o = &p.outbound;
    format!(
        "{}|{}|{}|{}|{}",
        p.profile_type.as_str(),
        o.server.as_deref().unwrap_or(""),
        o.server_port.unwrap_or(0),
        o.uuid.as_deref().or(o.password.as_deref()).unwrap_or(""),
        o.sni.as_deref().unwrap_or("")
    )
}

fn profile_identity_key(profile: &Profile) -> String {
    subscription_item_identity_key(profile.profile_type, &profile.outbound)
}

fn subscription_item_identity_key(profile_type: ProfileType, outbound: &ParsedOutbound) -> String {
    let has_structured_identity = outbound.server.is_some()
        || outbound.server_port.is_some()
        || outbound.uuid.is_some()
        || outbound.password.is_some()
        || outbound.username.is_some();
    serde_json::json!({
        "type": profile_type.as_str(),
        "server": outbound.server,
        "port": outbound.server_port,
        "uuid": outbound.uuid,
        "password": outbound.password,
        "username": outbound.username,
        "raw": if has_structured_identity { None } else { outbound.raw_json.as_deref() },
    })
    .to_string()
}

fn subscription_change(profile: &Profile) -> SubscriptionChange {
    SubscriptionChange {
        profile_id: profile.id,
        display: format!("{} {}", profile.profile_type.display_name(), profile.name),
    }
}

fn compare_profiles(
    left: &Profile,
    right: &Profile,
    column: ProfileSortColumn,
) -> std::cmp::Ordering {
    match column {
        ProfileSortColumn::Type => left
            .profile_type
            .display_name()
            .cmp(right.profile_type.display_name()),
        ProfileSortColumn::Address => left
            .display_address()
            .to_lowercase()
            .cmp(&right.display_address().to_lowercase()),
        ProfileSortColumn::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
        ProfileSortColumn::TestResult => upstream_latency_sort_key(left.latency_ms)
            .cmp(&upstream_latency_sort_key(right.latency_ms)),
        ProfileSortColumn::Traffic => left
            .traffic_downlink
            .saturating_add(left.traffic_uplink)
            .cmp(
                &right
                    .traffic_downlink
                    .saturating_add(right.traffic_uplink),
            ),
    }
}

fn upstream_latency_sort_key(ms: i32) -> i32 {
    if ms == 0 {
        100_000
    } else if ms < 0 {
        99_999
    } else {
        ms
    }
}

fn human_rate(bytes: i64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes.max(0) as f64;
    let mut i = 0usize;
    while v >= 1024.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes}{}", UNITS[i])
    } else {
        format!("{v:.1}{}", UNITS[i])
    }
}

/// Upstream `DisplayAddress` / `WrapIPV6Host` for status bar inbound label.
fn display_listen_address(host: &str, port: i32) -> String {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    let wrapped = match bare.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V6(ip)) => format!("[{ip}]"),
        Ok(std::net::IpAddr::V4(ip)) => ip.to_string(),
        Err(_) => host.to_string(),
    };
    format!("{wrapped}:{port}")
}

fn format_log_line(msg: &str) -> String {
    format_log_line_at(chrono::Local::now(), msg)
}

fn format_log_line_at<Tz: chrono::TimeZone>(timestamp: chrono::DateTime<Tz>, msg: &str) -> String
where
    Tz::Offset: std::fmt::Display,
{
    format!("[{}] {msg}", timestamp.format("%H:%M:%S"))
}

fn profile_is_structurally_invalid(profile: &Profile) -> bool {
    !matches!(
        profile.profile_type,
        ProfileType::Chain
            | ProfileType::AutoSelector
            | ProfileType::Custom
            | ProfileType::Direct
            | ProfileType::Tailscale
            | ProfileType::ExtraCore
    ) && profile.display_address().is_empty()
}

#[cfg(test)]
mod tests {

    use super::*;
    use chrono::{FixedOffset, TimeZone};

    #[test]
    fn formats_log_timestamps_in_the_supplied_local_timezone() {
        let offset = FixedOffset::east_opt(8 * 60 * 60).expect("valid UTC+8 offset");
        let timestamp = offset
            .with_ymd_and_hms(2026, 7, 30, 11, 23, 43)
            .single()
            .expect("valid timestamp");

        assert_eq!(
            format_log_line_at(timestamp, "Logs copied"),
            "[11:23:43] Logs copied"
        );
    }

    #[test]
    fn status_only_message_does_not_append_to_logs() {
        let mut state = AppState::empty();
        state.push_log("Existing log");
        let logs_before = state.logs_text();

        state.set_status_message_only("Running [Tun]");

        assert_eq!(state.status_message(), "Running [Tun]");
        assert_eq!(state.logs_text(), logs_before);
    }

    #[test]
    fn push_log_only_appends_without_changing_status() {
        let mut state = AppState::empty();
        state.set_status_message_only("Running [System Proxy]");
        state.push_log_only(
            "inbound/mixed[mixed-in]: inbound connection from 127.0.0.1:12345",
        );
        state.push_log_only(
            "outbound/direct[direct]: outbound connection to apple.com:443",
        );

        assert_eq!(state.status_message(), "Running [System Proxy]");
        let logs = state.logs_text();
        assert!(logs.contains("inbound/mixed"));
        assert!(logs.contains("outbound/direct"));
        assert!(logs.contains("apple.com"));
    }

    #[test]
    fn demo_has_groups_and_profiles() {
        let s = AppState::with_demo_data();
        assert!(s.group_order().len() >= 2);
        assert!(!s.visible_profiles().is_empty());
    }

    #[test]
    fn unavailable_removal_is_scoped_to_active_group_and_excludes_untested_profiles() {
        let mut state = AppState::empty();
        let active_group = state.add_group("Active");
        let other_group = state.add_group("Other");
        state.set_active_group(active_group).unwrap();

        let unavailable = state.add_profile(active_group, "Unavailable", ProfileType::Vless);
        let untested = state.add_profile(active_group, "Untested", ProfileType::Vless);
        let other_unavailable = state.add_profile(other_group, "Other unavailable", ProfileType::Vless);
        state.set_profile_latency(unavailable, -1);
        state.set_profile_latency(untested, 0);
        state.set_profile_latency(other_unavailable, -1);

        assert_eq!(state.remove_unavailable_in_group(active_group), 1);
        assert!(state.profile(unavailable).is_none());
        assert!(state.profile(untested).is_some());
        assert!(state.profile(other_unavailable).is_some());
    }

    #[test]
    fn url_test_result_with_zero_latency_remains_untested() {
        let mut state = AppState::empty();
        let group_id = state.add_group("Active");
        let profile_id = state.add_profile(group_id, "Untested", ProfileType::Vless);

        state.apply_url_test_results(&[(profile_id, 0, "")]);

        assert_eq!(state.profile(profile_id).unwrap().latency_ms, 0);
    }

    #[test]
    fn active_group_sort_by_latency_persists_upstream_order() {
        let mut state = AppState::empty();
        let group = state.add_group("G");
        let slow = state.add_profile(group, "slow", ProfileType::Vless);
        let untested = state.add_profile(group, "untested", ProfileType::Vless);
        let failed = state.add_profile(group, "failed", ProfileType::Vless);
        let fast = state.add_profile(group, "fast", ProfileType::Vless);
        state.set_profile_latency(slow, 180);
        state.set_profile_latency(untested, 0);
        state.set_profile_latency(failed, -1);
        state.set_profile_latency(fast, 47);

        state
            .sort_active_group_profiles(ProfileSortColumn::TestResult, true)
            .unwrap();
        assert_eq!(
            state
                .visible_profiles()
                .iter()
                .map(|profile| profile.id)
                .collect::<Vec<_>>(),
            vec![fast, slow, failed, untested],
        );

        state
            .sort_active_group_profiles(ProfileSortColumn::TestResult, false)
            .unwrap();
        assert_eq!(
            state
                .visible_profiles()
                .iter()
                .map(|profile| profile.id)
                .collect::<Vec<_>>(),
            vec![untested, failed, slow, fast],
        );
    }

    #[test]
    fn search_filters() {
        let mut s = AppState::with_demo_data();
        s.set_search_query("tokyo");
        let vis = s.visible_profiles();
        assert_eq!(vis.len(), 1);
        assert!(vis[0].name.to_lowercase().contains("tokyo"));
    }

    #[test]
    fn accumulates_live_traffic_on_the_running_profile() {
        let mut state = AppState::with_demo_data();
        let profile_id = state.selected_profile_id().expect("demo selection");
        let initial = state.profile(profile_id).expect("selected profile");
        let initial_downlink = initial.traffic_downlink;
        let initial_uplink = initial.traffic_uplink;

        state.set_profile_traffic(profile_id, 2_048, 1_024);
        state.set_profile_traffic(profile_id, 512, 256);

        let profile = state.profile(profile_id).expect("selected profile");
        assert_eq!(profile.traffic_downlink, initial_downlink + 2_560);
        assert_eq!(profile.traffic_uplink, initial_uplink + 1_280);
    }

    #[test]
    fn speed_label_matches_upstream_traffic_format() {
        let mut state = AppState::with_demo_data();
        state.toggle_selected().unwrap();
        state.set_traffic(TrafficSnapshot {
            proxy_up: 1_024,
            proxy_down: 17_510,
            direct_up: 0,
            direct_down: 0,
        });

        assert_eq!(
            state.speed_label(),
            "Proxy: 1.0KB↑ 17.1KB↓\nDirect: 0B↑ 0B↓"
        );
    }

    #[test]
    fn zero_traffic_sample_preserves_live_rates_while_running() {
        let mut state = AppState::with_demo_data();
        state.toggle_selected().unwrap();
        state.set_traffic(TrafficSnapshot {
            proxy_up: 1_024,
            ..TrafficSnapshot::default()
        });

        assert!(!state.update_live_traffic(TrafficSnapshot::default()));
        assert_eq!(state.traffic().proxy_up, 1_024);
    }

    #[test]
    fn stopping_core_clears_retained_live_rates() {
        let mut state = AppState::with_demo_data();
        state.toggle_selected().unwrap();
        state.set_traffic(TrafficSnapshot {
            proxy_down: 1_024,
            ..TrafficSnapshot::default()
        });

        state.set_core_status(CoreStatus::Stopped);

        assert_eq!(state.traffic(), &TrafficSnapshot::default());
    }

    #[test]
    fn toggle_start_stop() {
        let mut s = AppState::with_demo_data();
        s.toggle_selected().unwrap();
        assert!(s.core_status().is_running());
        s.toggle_selected().unwrap();
        assert!(!s.core_status().is_running());
    }

    #[test]
    fn apply_basic_settings_clamps_port() {
        let mut s = AppState::with_demo_data();
        s.apply_basic_settings(
            "127.0.0.1".into(),
            99999,
            "https://example.com/204".into(),
            "https://dns.google/dns-query".into(),
            "localhost".into(),
            "info".into(),
            crate::models::RulesetMirror::Github,
            true,
        );
        assert_eq!(s.settings().inbound_socks_port, 65535);
        assert_eq!(s.settings().inbound_address, "127.0.0.1");
        assert_eq!(s.settings().test_latency_url, "https://example.com/204");
        assert_eq!(s.settings().log_level, "info");
        assert_eq!(s.settings().ruleset_mirror, crate::models::RulesetMirror::Github);
        assert!(s.settings().adblock_enable);
    }

    #[test]
    fn inbound_label_shows_allow_lan_addresses() {
        let mut s = AppState::with_demo_data();
        assert_eq!(s.inbound_label(), "Mixed: 127.0.0.1:2080");

        s.settings_mut().inbound_address = "::".into();
        assert_eq!(s.inbound_label(), "Mixed: [::]:2080");

        s.settings_mut().inbound_address = "0.0.0.0".into();
        s.settings_mut().inbound_socks_port = 1080;
        assert_eq!(s.inbound_label(), "Mixed: 0.0.0.0:1080");
    }

    #[test]
    fn manage_groups_add_rename_delete() {
        let mut s = AppState::with_demo_data();
        let id = s.add_group("Work");
        s.rename_group(id, "Work VPN").unwrap();
        s.set_group_url(id, "https://example.com/sub").unwrap();
        let g = s.group(id).unwrap();
        assert_eq!(g.name, "Work VPN");
        assert_eq!(g.url, "https://example.com/sub");
        s.delete_group(id).unwrap();
        assert!(s.group(id).is_none());
    }

    #[test]
    fn cannot_delete_last_group() {
        let mut s = AppState::empty();
        let id = s.add_group("only");
        assert!(s.delete_group(id).is_err());
    }

    #[test]
    fn rename_profile_trims_and_rejects_empty() {
        let mut s = AppState::with_demo_data();
        let id = s.selected_profile_id().expect("demo selection");
        s.rename_profile(id, "  Tokyo Express  ").unwrap();
        assert_eq!(s.profile(id).unwrap().name, "Tokyo Express");
        assert!(s.rename_profile(id, "   ").is_err());
    }

    #[test]
    fn removes_only_profiles_that_are_structurally_invalid() {
        let mut state = AppState::empty();
        let group_id = state.add_group("test");

        let invalid_id = state.add_profile(group_id, "missing endpoint", ProfileType::Vless);
        let valid_id = state.add_profile(group_id, "valid", ProfileType::Vless);
        let valid = state.profiles.get_mut(&valid_id).unwrap();
        valid.outbound.server = Some("example.com".into());
        valid.outbound.server_port = Some(443);
        valid.outbound.uuid = Some("user-id".into());

        let custom_id = state.add_profile(group_id, "custom", ProfileType::Custom);
        state.profiles.get_mut(&custom_id).unwrap().outbound_json = "not parsed yet".into();

        assert_eq!(state.remove_invalid_in_group(group_id), 1);
        assert!(state.profile(invalid_id).is_none());
        assert!(state.profile(valid_id).is_some());
        assert!(state.profile(custom_id).is_some());
    }

    #[test]
    fn replace_group_profiles_reports_add_remove_kept() {
        let mut s = AppState::empty();
        let gid = s.add_group("sub");
        // Seed one node.
        let mut o1 = ParsedOutbound::default();
        o1.server = Some("1.1.1.1".into());
        o1.server_port = Some(443);
        o1.uuid = Some("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into());
        s.replace_group_profiles(
            gid,
            vec![(
                "old-a".into(),
                ProfileType::Vless,
                o1.clone(),
                false,
            )],
        )
        .unwrap();

        // Keep same identity, rename; add a second node; first identity stays → kept=1, added=1, removed=0
        let mut o2 = ParsedOutbound::default();
        o2.server = Some("2.2.2.2".into());
        o2.server_port = Some(443);
        o2.uuid = Some("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb".into());
        let sum = s
            .replace_group_profiles(
                gid,
                vec![
                    ("renamed-a".into(), ProfileType::Vless, o1, false),
                    ("new-b".into(), ProfileType::Vless, o2, false),
                ],
            )
            .unwrap();
        assert_eq!(sum.total, 2);
        assert_eq!(sum.kept, 1);
        assert_eq!(sum.added, 1);
        assert_eq!(sum.removed, 0);

        // Drop both, add only a third → removed=2, added=1, kept=0
        let mut o3 = ParsedOutbound::default();
        o3.server = Some("3.3.3.3".into());
        o3.server_port = Some(8443);
        let sum2 = s
            .replace_group_profiles(
                gid,
                vec![("only-c".into(), ProfileType::Vless, o3, false)],
            )
            .unwrap();
        assert_eq!(sum2.total, 1);
        assert_eq!(sum2.kept, 0);
        assert_eq!(sum2.added, 1);
        assert_eq!(sum2.removed, 2);
        assert!(sum2.format_status().contains("+1"));
    }

    #[test]
    fn subscription_snapshot_preserves_identity_metadata_and_remote_order() {
        let mut state = AppState::empty();
        let group_id = state.add_group("sub");
        let mut old_a = ParsedOutbound::default();
        old_a.server = Some("a.example".into());
        old_a.server_port = Some(443);
        old_a.uuid = Some("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into());
        old_a.sni = Some("old.example".into());
        let mut old_b = ParsedOutbound::default();
        old_b.server = Some("b.example".into());
        old_b.server_port = Some(443);
        old_b.uuid = Some("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb".into());
        state
            .replace_group_profiles(
                group_id,
                vec![
                    ("A".into(), ProfileType::Vless, old_a.clone(), false),
                    ("B".into(), ProfileType::Vless, old_b.clone(), false),
                ],
            )
            .unwrap();
        let old_ids = state.group(group_id).unwrap().profile_ids.clone();
        state.profiles.get_mut(&old_ids[0]).unwrap().latency_ms = 42;
        state.profiles.get_mut(&old_ids[0]).unwrap().traffic_downlink = 900;

        let mut changed_a = old_a;
        changed_a.sni = Some("new.example".into());
        let mut new_c = ParsedOutbound::default();
        new_c.server = Some("c.example".into());
        new_c.server_port = Some(8443);
        new_c.password = Some("secret".into());
        let report = state
            .apply_subscription_snapshot(
                group_id,
                vec![
                    ("B".into(), ProfileType::Vless, old_b, false),
                    ("A renamed".into(), ProfileType::Vless, changed_a, true),
                    ("C".into(), ProfileType::Trojan, new_c, false),
                ],
                "upload=1; download=2; total=10".into(),
                1_785_500_000,
            )
            .unwrap();

        assert_eq!(report.added.len(), 1);
        assert_eq!(report.updated.len(), 1);
        assert_eq!(report.deleted.len(), 0);
        assert_eq!(report.unchanged, 1);
        assert_eq!(report.result_order[..2], [old_ids[1], old_ids[0]]);
        assert_eq!(state.profile(old_ids[0]).unwrap().latency_ms, 42);
        assert_eq!(state.profile(old_ids[0]).unwrap().traffic_downlink, 900);
        assert_eq!(state.profile(old_ids[0]).unwrap().name, "A renamed");
        assert!(state.profile(old_ids[0]).unwrap().insecure);
        let group = state.group(group_id).unwrap();
        assert_eq!(group.info, "upload=1; download=2; total=10");
        assert_eq!(group.sub_last_update, 1_785_500_000);
    }

    #[test]
    fn subscription_snapshot_reports_deletions() {
        let mut state = AppState::empty();
        let group_id = state.add_group("sub");
        let mut outbound = ParsedOutbound::default();
        outbound.server = Some("gone.example".into());
        outbound.server_port = Some(443);
        state
            .replace_group_profiles(
                group_id,
                vec![("Gone".into(), ProfileType::Trojan, outbound, false)],
            )
            .unwrap();
        let removed_id = state.group(group_id).unwrap().profile_ids[0];

        let report = state
            .apply_subscription_snapshot(group_id, Vec::new(), String::new(), 123)
            .unwrap();

        assert_eq!(report.deleted.len(), 1);
        assert_eq!(report.deleted[0].profile_id, removed_id);
        assert!(state.profile(removed_id).is_none());
    }

    #[test]
    fn subscription_snapshot_retains_running_profile_removed_by_remote_snapshot() {
        let mut state = AppState::empty();
        let group_id = state.add_group("subscription");
        let running_id = state.add_profile(group_id, "running", ProfileType::Vless);
        state.set_core_status(CoreStatus::Running {
            profile_id: running_id,
            profile_name: "running".into(),
        });

        let report = state
            .apply_subscription_snapshot(group_id, Vec::new(), "new-info".into(), 123)
            .unwrap();

        assert!(state.profile(running_id).is_some());
        assert_eq!(state.group(group_id).unwrap().profile_ids, vec![running_id]);
        assert!(report.deleted.is_empty());
        assert_eq!(report.result_order, vec![running_id]);
    }

    #[test]
    fn subscription_snapshot_preserves_running_id_with_duplicate_identities() {
        let mut state = AppState::empty();
        let group_id = state.add_group("subscription");
        let outbound = ParsedOutbound {
            server: Some("same.example".into()),
            server_port: Some(443),
            uuid: Some("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into()),
            ..Default::default()
        };
        state
            .replace_group_profiles(
                group_id,
                vec![
                    ("first".into(), ProfileType::Vless, outbound.clone(), false),
                    ("running".into(), ProfileType::Vless, outbound.clone(), false),
                ],
            )
            .unwrap();
        let ids = state.group(group_id).unwrap().profile_ids.clone();
        state.set_core_status(CoreStatus::Running {
            profile_id: ids[1],
            profile_name: "running".into(),
        });

        let report = state
            .apply_subscription_snapshot(
                group_id,
                vec![("running".into(), ProfileType::Vless, outbound, false)],
                String::new(),
                123,
            )
            .unwrap();

        assert_eq!(report.result_order, vec![ids[1]]);
        assert!(state.profile(ids[1]).is_some());
        assert!(state.profile(ids[0]).is_none());
    }

    #[test]
    fn subscription_snapshot_updates_running_profile_when_identity_is_unchanged() {
        let mut state = AppState::empty();
        let group_id = state.add_group("subscription");
        let old = ParsedOutbound {
            server: Some("same.example".into()),
            server_port: Some(443),
            uuid: Some("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into()),
            sni: Some("old.example".into()),
            ..Default::default()
        };
        state
            .replace_group_profiles(
                group_id,
                vec![("old name".into(), ProfileType::Vless, old.clone(), false)],
            )
            .unwrap();
        let running_id = state.group(group_id).unwrap().profile_ids[0];
        state.set_core_status(CoreStatus::Running {
            profile_id: running_id,
            profile_name: "old name".into(),
        });
        let mut changed = old;
        changed.sni = Some("new.example".into());

        let report = state
            .apply_subscription_snapshot(
                group_id,
                vec![("new name".into(), ProfileType::Vless, changed, false)],
                String::new(),
                123,
            )
            .unwrap();

        assert_eq!(report.result_order, vec![running_id]);
        assert_eq!(report.updated.len(), 1);
        assert_eq!(state.profile(running_id).unwrap().name, "new name");
        assert!(matches!(
            state.core_status(),
            CoreStatus::Running { profile_name, .. } if profile_name == "new name"
        ));
    }

    #[test]
    fn subscription_snapshot_does_not_swap_duplicate_identity_profiles() {
        let mut state = AppState::empty();
        let group_id = state.add_group("subscription");
        let shared = ParsedOutbound {
            server: Some("same.example".into()),
            server_port: Some(443),
            uuid: Some("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into()),
            ..Default::default()
        };
        state
            .replace_group_profiles(
                group_id,
                vec![
                    ("first".into(), ProfileType::Vless, shared.clone(), false),
                    ("running".into(), ProfileType::Vless, shared.clone(), false),
                ],
            )
            .unwrap();
        let ids = state.group(group_id).unwrap().profile_ids.clone();
        state.set_core_status(CoreStatus::Running {
            profile_id: ids[1],
            profile_name: "running".into(),
        });
        let mut changed_running = shared.clone();
        changed_running.sni = Some("changed.example".into());

        let report = state
            .apply_subscription_snapshot(
                group_id,
                vec![
                    ("first".into(), ProfileType::Vless, shared, false),
                    (
                        "running".into(),
                        ProfileType::Vless,
                        changed_running,
                        false,
                    ),
                ],
                String::new(),
                123,
            )
            .unwrap();

        assert_eq!(report.result_order, ids);
        assert_eq!(state.profile(ids[0]).unwrap().name, "first");
        assert_eq!(state.profile(ids[1]).unwrap().name, "running");
        assert_eq!(
            state.profile(ids[1]).unwrap().outbound.sni.as_deref(),
            Some("changed.example")
        );
    }

    #[test]
    fn subscription_snapshot_matches_changed_duplicate_identities_by_name() {
        let mut state = AppState::empty();
        let group_id = state.add_group("subscription");
        let first = ParsedOutbound {
            server: Some("same.example".into()),
            server_port: Some(443),
            uuid: Some("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into()),
            sni: Some("old-first.example".into()),
            ..Default::default()
        };
        let mut running = first.clone();
        running.sni = Some("old-running.example".into());
        state
            .replace_group_profiles(
                group_id,
                vec![
                    ("first".into(), ProfileType::Vless, first.clone(), false),
                    (
                        "running".into(),
                        ProfileType::Vless,
                        running.clone(),
                        false,
                    ),
                ],
            )
            .unwrap();
        let ids = state.group(group_id).unwrap().profile_ids.clone();
        state.set_core_status(CoreStatus::Running {
            profile_id: ids[1],
            profile_name: "running".into(),
        });
        let mut changed_first = first;
        changed_first.sni = Some("new-first.example".into());
        let mut changed_running = running;
        changed_running.sni = Some("new-running.example".into());

        let report = state
            .apply_subscription_snapshot(
                group_id,
                vec![
                    (
                        "first".into(),
                        ProfileType::Vless,
                        changed_first,
                        false,
                    ),
                    (
                        "running".into(),
                        ProfileType::Vless,
                        changed_running,
                        false,
                    ),
                ],
                String::new(),
                123,
            )
            .unwrap();

        assert_eq!(report.result_order, ids);
        assert_eq!(
            state.profile(ids[0]).unwrap().outbound.sni.as_deref(),
            Some("new-first.example")
        );
        assert_eq!(
            state.profile(ids[1]).unwrap().outbound.sni.as_deref(),
            Some("new-running.example")
        );
    }

    #[test]
    fn subscription_identity_does_not_collide_when_usernames_differ() {
        let first = ParsedOutbound {
            server: Some("proxy.example".into()),
            server_port: Some(1080),
            username: Some("alice".into()),
            password: Some("secret".into()),
            ..Default::default()
        };
        let second = ParsedOutbound {
            username: Some("bob".into()),
            ..first.clone()
        };

        assert_ne!(
            subscription_item_identity_key(ProfileType::Socks, &first),
            subscription_item_identity_key(ProfileType::Socks, &second)
        );
    }

    #[test]
    fn subscription_identity_uses_raw_config_when_no_structured_identity_exists() {
        let first = ParsedOutbound {
            raw_json: Some("{\"endpoint\":\"one\"}".into()),
            ..Default::default()
        };
        let second = ParsedOutbound {
            raw_json: Some("{\"endpoint\":\"two\"}".into()),
            ..Default::default()
        };

        assert_ne!(
            subscription_item_identity_key(ProfileType::Custom, &first),
            subscription_item_identity_key(ProfileType::Custom, &second)
        );
    }
}
