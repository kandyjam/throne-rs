use std::collections::HashMap;

use thiserror::Error;

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
    settings: AppSettings,
    routes: HashMap<i64, RouteProfile>,
    route_order: Vec<i64>,
    active_route_id: Option<i64>,
    next_route_id: i64,
}

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
        state.status_message = "Demo data loaded · core not connected".into();
        state
    }

    pub fn status_message(&self) -> &str {
        &self.status_message
    }

    pub fn set_status_message(&mut self, msg: impl Into<String>) {
        self.status_message = msg.into();
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

    /// Toggle start/stop for the selected profile (domain-only; core client hooks later).
    pub fn toggle_selected(&mut self) -> Result<(), StoreError> {
        match &self.core_status {
            CoreStatus::Running { .. } => {
                self.core_status = CoreStatus::Stopped;
                self.traffic = TrafficSnapshot::default();
                self.status_message = "Core stopped".into();
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
                self.status_message = format!("Started · {name} (simulated)");
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
        self.status_message = match mode {
            SystemMode::Off => "System mode: off".into(),
            SystemMode::SystemProxy => "System Proxy enabled".into(),
            SystemMode::VpnTun => "Tun Mode enabled".into(),
        };
    }

    /// Upstream `checkBox_SystemProxy`.
    pub fn set_spmode_system_proxy(&mut self, enable: bool) {
        self.settings.system_proxy_enabled = enable;
        self.sync_system_mode_from_flags();
        self.status_message = if enable {
            "System Proxy enabled".into()
        } else {
            "System Proxy disabled".into()
        };
    }

    /// Upstream `checkBox_VPN` (Tun Mode).
    pub fn set_spmode_vpn(&mut self, enable: bool) {
        self.settings.tun_mode_enabled = enable;
        self.sync_system_mode_from_flags();
        self.status_message = if enable {
            "Tun Mode enabled".into()
        } else {
            "Tun Mode disabled".into()
        };
    }

    /// Upstream `system_dns` checkbox.
    pub fn set_system_dns(&mut self, enable: bool) {
        self.settings.system_dns_set = enable;
        self.status_message = if enable {
            "System DNS enabled".into()
        } else {
            "System DNS disabled".into()
        };
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
    pub fn inbound_label(&self) -> String {
        let s = &self.settings;
        format!(
            "Mixed: {}:{}",
            s.inbound_address, s.inbound_socks_port
        )
    }

    /// Speed lines for `label_speed`.
    pub fn speed_label(&self) -> String {
        if !self.core_status.is_running() {
            return String::new();
        }
        format!(
            "Proxy: ↓{}/s ↑{}/s\nDirect: ↓{}/s ↑{}/s",
            human_rate(self.traffic.proxy_down),
            human_rate(self.traffic.proxy_up),
            human_rate(self.traffic.direct_down),
            human_rate(self.traffic.direct_up),
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
        self.core_status = status;
    }

    pub fn set_traffic(&mut self, traffic: TrafficSnapshot) {
        self.traffic = traffic;
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
        self.group_order = group_order;
        self.settings = settings;
        self.route_order = routes.iter().map(|r| r.id).collect();
        self.routes = routes.into_iter().map(|r| (r.id, r)).collect();
        self.next_group_id = self.groups.keys().copied().max().unwrap_or(0) + 1;
        self.next_profile_id = self.profiles.keys().copied().max().unwrap_or(0) + 1;
        self.next_route_id = self.routes.keys().copied().max().unwrap_or(0) + 1;
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
        self.status_message = format!(
            "Loaded {} groups · {} profiles · {} routes",
            self.groups.len(),
            self.profiles.len(),
            self.routes.len()
        );
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
            self.status_message = format!("Imported {n} route profile(s)");
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
            self.status_message = format!("Active route · {}", r.name);
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

    pub fn all_groups(&self) -> Vec<&Group> {
        self.group_order
            .iter()
            .filter_map(|id| self.groups.get(id))
            .collect()
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
            let outbound_json = outbound.to_db_json();
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
            self.status_message = format!("Imported {n} profile(s)");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_has_groups_and_profiles() {
        let s = AppState::with_demo_data();
        assert!(s.group_order().len() >= 2);
        assert!(!s.visible_profiles().is_empty());
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
    fn toggle_start_stop() {
        let mut s = AppState::with_demo_data();
        s.toggle_selected().unwrap();
        assert!(s.core_status().is_running());
        s.toggle_selected().unwrap();
        assert!(!s.core_status().is_running());
    }
}
