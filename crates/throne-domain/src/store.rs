use std::collections::HashMap;

use thiserror::Error;

use crate::models::{
    CoreStatus, Group, GroupId, Profile, ProfileId, ProfileType, SystemMode, TrafficSnapshot,
};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("group {0} not found")]
    GroupNotFound(GroupId),
    #[error("profile {0} not found")]
    ProfileNotFound(ProfileId),
    #[error("no profile selected")]
    NoSelection,
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
        }
    }

    /// Seed data so the GPUI shell is usable before persistence lands.
    pub fn with_demo_data() -> Self {
        let mut state = Self::empty();

        let g1 = state.add_group("Default");
        let g2 = state.add_group("Subscriptions");

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
        self.status_message = match mode {
            SystemMode::Off => "System mode: off".into(),
            SystemMode::SystemProxy => "System mode: system proxy".into(),
            SystemMode::VpnTun => "System mode: TUN / VPN".into(),
        };
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
