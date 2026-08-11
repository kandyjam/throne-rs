//! SQLite persistence **wire-compatible** with upstream Throne
//! (`throneproj/Throne` `ProfilesRepo` / `GroupsRepo` / `RoutesRepo` / `SettingsRepo`).
//!
//! Original DB layout (see `main.cpp`):
//! - Portable: `<appDir>/config/throne.db`
//! - AppData (`-appdata` / packaged macOS): `<AppConfigLocation>/config/throne.db`
//! - Stats sibling: `throne_stats.db` ([`traffic_stats`])

mod paths;
mod schema;
mod traffic_stats;

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;
use tracing::{info, warn};

use throne_domain::{
    AppSettings, AppState, DefaultOutbound, Group, GroupId, ParsedOutbound, Profile, ProfileId,
    ProfileType, RouteProfile, RouteRule,
};

pub use paths::{default_db_path, discover_throne_databases, resolve_db_path, stats_db_path};
pub use traffic_stats::{
    AppTrafficRow, AppUsage, ConfigMetaRow, ConfigTrafficRow, ConfigUsage, DIRECT_STAT_PROFILE_ID,
    TrafficSeriesPoint, TrafficStatsDb, TrafficStatsError, TrafficStatsManager,
};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Msg(String),
}

pub struct Database {
    conn: Connection,
    path: PathBuf,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path.as_ref())?;
        // Match upstream SQLiteCpp defaults used by Throne.
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        let db = Self {
            conn,
            path: path.as_ref().to_path_buf(),
        };
        db.ensure_schema()?;
        Ok(db)
    }

    /// Open an existing Throne database read/write without creating parent dirs
    /// if the file is missing (returns error).
    pub fn open_existing(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref();
        if !path.is_file() {
            return Err(StorageError::Msg(format!(
                "database not found: {}",
                path.display()
            )));
        }
        Self::open(path)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn ensure_schema(&self) -> Result<(), StorageError> {
        schema::ensure_schema(&self.conn)
    }

    pub fn load_state(&self) -> Result<AppState, StorageError> {
        let mut state = AppState::empty();
        let groups = self.load_groups()?;
        let profiles = self.load_profiles()?;
        let order = self.load_group_order()?;
        let settings = self.load_settings()?;
        let routes = self.load_routes()?;
        let order = if order.is_empty() {
            groups.iter().map(|g| g.id).collect()
        } else {
            order
        };
        state.load_snapshot(groups, profiles, order, settings, routes);
        Ok(state)
    }

    /// Full rewrite save in upstream-compatible column layout.
    /// Unknown `settings` keys are preserved (merge, not wipe).
    pub fn save_state(&self, state: &AppState) -> Result<(), StorageError> {
        let tx = self.conn.unchecked_transaction()?;
        // Child tables first when wiping.
        tx.execute("DELETE FROM route_rules", [])?;
        tx.execute("DELETE FROM route_profiles", [])?;
        tx.execute("DELETE FROM profiles", [])?;
        tx.execute("DELETE FROM groups_order", [])?;
        tx.execute("DELETE FROM groups", [])?;

        // Prefer tab order, but always persist every group so profile FK(gid)
        // cannot fail when group_order is a partial subset.
        let mut saved_groups: Vec<&throne_domain::Group> = state.all_groups();
        let ordered: std::collections::HashSet<_> =
            saved_groups.iter().map(|g| g.id).collect();
        for g in state.groups_map().values() {
            if !ordered.contains(&g.id) {
                saved_groups.push(g);
            }
        }

        for g in &saved_groups {
            let profiles_json = serde_json::to_string(&g.profile_ids)?;
            tx.execute(
                r#"INSERT INTO groups (
                    id, archive, skip_auto_update, auto_clear_unavailable, name, url, info,
                    sub_last_update, front_proxy_id, landing_proxy_id,
                    column_width_json, profiles_json, scroll_last_profile,
                    test_sort_by, traffic_sort_by, test_items_to_show, type_sort_by
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)"#,
                params![
                    g.id,
                    g.archive as i32,
                    g.skip_auto_update as i32,
                    g.auto_clear_unavailable as i32,
                    g.name,
                    g.url,
                    g.info,
                    g.sub_last_update,
                    g.front_proxy_id,
                    g.landing_proxy_id,
                    g.column_width_json,
                    profiles_json,
                    g.scroll_last_profile,
                    g.test_sort_by,
                    g.traffic_sort_by,
                    g.test_items_to_show,
                    g.type_sort_by,
                ],
            )?;
        }

        let saved_ids: std::collections::HashSet<_> =
            saved_groups.iter().map(|g| g.id).collect();
        let mut order_idx = 0i64;
        for gid in state.group_order() {
            if !saved_ids.contains(gid) {
                continue;
            }
            tx.execute(
                "INSERT INTO groups_order (group_id, display_order) VALUES (?1, ?2)",
                params![gid, order_idx],
            )?;
            order_idx += 1;
        }
        for g in &saved_groups {
            if state.group_order().contains(&g.id) {
                continue;
            }
            tx.execute(
                "INSERT INTO groups_order (group_id, display_order) VALUES (?1, ?2)",
                params![g.id, order_idx],
            )?;
            order_idx += 1;
        }

        for p in state.all_profiles() {
            // Skip orphans — profiles.gid has FK → groups(id).
            if !saved_ids.contains(&p.group_id) {
                continue;
            }
            let outbound_json = outbound_json_for_db(p);
            tx.execute(
                r#"INSERT INTO profiles (
                    id, type, name, gid, latency, dl_speed, ul_speed,
                    test_country, ip_out, outbound_json, traffic_dl, traffic_up
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"#,
                params![
                    p.id,
                    p.profile_type.as_str(),
                    p.name,
                    p.group_id,
                    p.latency_ms,
                    p.download_speed,
                    p.upload_speed,
                    p.test_country,
                    p.ip_out,
                    outbound_json,
                    p.traffic_downlink,
                    p.traffic_uplink,
                ],
            )?;
        }

        for r in state.all_routes() {
            tx.execute(
                r#"INSERT INTO route_profiles (
                    id, name, default_outbound_id, is_raw, raw_route,
                    prevent_modifications, is_remote, remote_url, auto_update,
                    remote_last_update
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"#,
                params![
                    r.id,
                    r.name,
                    r.default_outbound.as_id(),
                    r.is_raw as i32,
                    r.raw_route,
                    r.prevent_modifications as i32,
                    r.is_remote as i32,
                    r.remote_url,
                    r.auto_update as i32,
                    r.remote_last_update,
                ],
            )?;
            for (order, rule) in r.rules.iter().enumerate() {
                insert_route_rule(&tx, r.id, order as i32, rule)?;
            }
        }

        let max_p = state.all_profiles().iter().map(|p| p.id).max().unwrap_or(0);
        let max_g = state.all_groups().iter().map(|g| g.id).max().unwrap_or(0);
        let max_r = state.all_routes().iter().map(|r| r.id).max().unwrap_or(0);
        // entity_ids is a single-row table in upstream.
        let n: i64 = tx.query_row("SELECT COUNT(*) FROM entity_ids", [], |row| row.get(0))?;
        if n == 0 {
            tx.execute(
                "INSERT INTO entity_ids (profile_last_id, group_last_id, route_profile_last_id) VALUES (?1,?2,?3)",
                params![max_p, max_g, max_r],
            )?;
        } else {
            tx.execute(
                "UPDATE entity_ids SET profile_last_id = ?1, group_last_id = ?2, route_profile_last_id = ?3",
                params![max_p, max_g, max_r],
            )?;
        }

        merge_settings_tx(&tx, state.settings())?;
        tx.commit()?;
        info!(path = %self.path.display(), "database saved (throne-compatible)");
        Ok(())
    }

    fn load_groups(&self) -> Result<Vec<Group>, StorageError> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, archive, skip_auto_update, auto_clear_unavailable, name, url, info,
                      sub_last_update, front_proxy_id, landing_proxy_id,
                      column_width_json, profiles_json, scroll_last_profile,
                      test_sort_by, traffic_sort_by, test_items_to_show, type_sort_by
               FROM groups"#,
        )?;
        let rows = stmt.query_map([], |row| {
            let profiles_json: String = row.get::<_, Option<String>>(11)?.unwrap_or_else(|| "[]".into());
            let profile_ids: Vec<ProfileId> =
                serde_json::from_str(&profiles_json).unwrap_or_default();
            Ok(Group {
                id: row.get(0)?,
                archive: row.get::<_, i32>(1)? != 0,
                skip_auto_update: row.get::<_, i32>(2)? != 0,
                auto_clear_unavailable: row.get::<_, i32>(3)? != 0,
                name: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                url: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                info: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                sub_last_update: row.get::<_, Option<i64>>(7)?.unwrap_or(0),
                front_proxy_id: row.get::<_, Option<i64>>(8)?.unwrap_or(-1),
                landing_proxy_id: row.get::<_, Option<i64>>(9)?.unwrap_or(-1),
                column_width_json: row.get::<_, Option<String>>(10)?.unwrap_or_default(),
                profile_ids,
                scroll_last_profile: row.get::<_, Option<i64>>(12)?.unwrap_or(-1),
                test_sort_by: row.get::<_, Option<i32>>(13)?.unwrap_or(0),
                traffic_sort_by: row.get::<_, Option<i32>>(14)?.unwrap_or(0),
                test_items_to_show: row.get::<_, Option<i32>>(15)?.unwrap_or(0),
                type_sort_by: row.get::<_, Option<i32>>(16)?.unwrap_or(0),
            })
        })?;
        collect_rows(rows)
    }

    fn load_profiles(&self) -> Result<Vec<Profile>, StorageError> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, type, name, gid, latency, dl_speed, ul_speed,
                      test_country, ip_out, outbound_json, traffic_dl, traffic_up
               FROM profiles"#,
        )?;
        let rows = stmt.query_map([], |row| {
            let ty: String = row.get(1)?;
            let outbound_json: String = row.get::<_, Option<String>>(9)?.unwrap_or_else(|| "{}".into());
            let (outbound, name_from_ob) = parse_outbound_json(&outbound_json);
            let mut name: String = row.get::<_, Option<String>>(2)?.unwrap_or_default();
            // Upstream often stores name in outbound.tag and may leave name empty/stale.
            if name.is_empty() {
                if let Some(n) = name_from_ob {
                    name = n;
                }
            }
            let profile_type = ProfileType::from_upstream(&ty).unwrap_or(ProfileType::Custom);
            let insecure = outbound.insecure.unwrap_or(false);
            Ok(Profile {
                id: row.get(0)?,
                group_id: row.get(3)?,
                name,
                profile_type,
                latency_ms: row.get::<_, Option<i32>>(4)?.unwrap_or(0),
                download_speed: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                upload_speed: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
                test_country: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
                ip_out: row.get::<_, Option<String>>(8)?.unwrap_or_default(),
                outbound_json,
                outbound,
                traffic_downlink: row.get::<_, Option<i64>>(10)?.unwrap_or(0),
                traffic_uplink: row.get::<_, Option<i64>>(11)?.unwrap_or(0),
                insecure,
            })
        })?;
        collect_rows(rows)
    }

    fn load_group_order(&self) -> Result<Vec<GroupId>, StorageError> {
        let mut stmt = self
            .conn
            .prepare("SELECT group_id FROM groups_order ORDER BY display_order ASC")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        collect_rows(rows)
    }

    fn load_settings(&self) -> Result<AppSettings, StorageError> {
        let mut s = AppSettings::default();
        let mut stmt = self.conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |row| {
            let k: String = row.get(0)?;
            let v: String = row.get(1)?;
            Ok((k, v))
        })?;
        for r in rows {
            let (k, v) = r?;
            apply_setting(&mut s, &k, &v);
        }
        Ok(s)
    }

    fn load_routes(&self) -> Result<Vec<RouteProfile>, StorageError> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, name, default_outbound_id, is_raw, raw_route,
                      prevent_modifications, is_remote, remote_url, auto_update,
                      remote_last_update
               FROM route_profiles ORDER BY id ASC"#,
        )?;
        let mut profiles = Vec::new();
        {
            let rows = stmt.query_map([], |row| {
                Ok(RouteProfile {
                    id: row.get(0)?,
                    name: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    default_outbound: DefaultOutbound::from_id(
                        row.get::<_, Option<i64>>(2)?.unwrap_or(-1),
                    ),
                    rules: Vec::new(),
                    is_raw: row.get::<_, i32>(3)? != 0,
                    raw_route: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    prevent_modifications: row.get::<_, i32>(5)? != 0,
                    is_remote: row.get::<_, i32>(6)? != 0,
                    remote_url: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
                    auto_update: row.get::<_, i32>(8)? != 0,
                    remote_last_update: row.get::<_, Option<i64>>(9)?.unwrap_or(0),
                })
            })?;
            for r in rows {
                profiles.push(r?);
            }
        }

        for p in &mut profiles {
            p.rules = self.load_rules_for(p.id)?;
        }
        Ok(profiles)
    }

    fn load_rules_for(&self, profile_id: i64) -> Result<Vec<RouteRule>, StorageError> {
        let mut stmt = self.conn.prepare(
            r#"SELECT name, type, ip_version, network, protocol,
                      inbound_json, domain_json, domain_suffix_json, domain_keyword_json, domain_regex_json,
                      source_ip_cidr_json, source_ip_is_private, ip_cidr_json, ip_is_private,
                      source_port_json, source_port_range_json, port_json, port_range_json,
                      process_name_json, process_path_json, process_path_regex_json, rule_set_json,
                      invert, outbound_id, action, reject_method, no_drop,
                      override_address, override_port, sniffers_json, sniff_override_dest, strategy,
                      wifi_ssid_json, wifi_bssid_json
               FROM route_rules WHERE route_profile_id = ? ORDER BY rule_order"#,
        )?;
        let rows = stmt.query_map(params![profile_id], |row| {
            let type_int: i32 = row.get(1)?;
            Ok(RouteRule {
                name: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                rule_type: type_int,
                rule_type_token: RouteRule::token_from_type(type_int).to_string(),
                ip_version: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                network: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                protocol: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                inbound: json_str_list(row.get::<_, Option<String>>(5)?),
                domain: json_str_list(row.get::<_, Option<String>>(6)?),
                domain_suffix: json_str_list(row.get::<_, Option<String>>(7)?),
                domain_keyword: json_str_list(row.get::<_, Option<String>>(8)?),
                domain_regex: json_str_list(row.get::<_, Option<String>>(9)?),
                source_ip_cidr: json_str_list(row.get::<_, Option<String>>(10)?),
                source_ip_is_private: row.get::<_, Option<i32>>(11)?.unwrap_or(0) != 0,
                ip_cidr: json_str_list(row.get::<_, Option<String>>(12)?),
                ip_is_private: row.get::<_, Option<i32>>(13)?.unwrap_or(0) != 0,
                source_port: json_str_list(row.get::<_, Option<String>>(14)?),
                source_port_range: json_str_list(row.get::<_, Option<String>>(15)?),
                port: json_str_list(row.get::<_, Option<String>>(16)?),
                port_range: json_str_list(row.get::<_, Option<String>>(17)?),
                process_name: json_str_list(row.get::<_, Option<String>>(18)?),
                process_path: json_str_list(row.get::<_, Option<String>>(19)?),
                process_path_regex: json_str_list(row.get::<_, Option<String>>(20)?),
                rule_set: json_str_list(row.get::<_, Option<String>>(21)?),
                invert: row.get::<_, Option<i32>>(22)?.unwrap_or(0) != 0,
                outbound_id: row.get::<_, Option<i64>>(23)?.unwrap_or(-2),
                action: row
                    .get::<_, Option<String>>(24)?
                    .unwrap_or_else(|| "route".into()),
                reject_method: row.get::<_, Option<String>>(25)?.unwrap_or_default(),
                no_drop: row.get::<_, Option<i32>>(26)?.unwrap_or(0) != 0,
                override_address: row.get::<_, Option<String>>(27)?.unwrap_or_default(),
                override_port: row.get::<_, Option<String>>(28)?.unwrap_or_default(),
                sniffers: json_str_list(row.get::<_, Option<String>>(29)?),
                sniff_override_dest: row.get::<_, Option<i32>>(30)?.unwrap_or(0) != 0,
                strategy: row.get::<_, Option<String>>(31)?.unwrap_or_default(),
                wifi_ssid: json_str_list(row.get::<_, Option<String>>(32)?),
                wifi_bssid: json_str_list(row.get::<_, Option<String>>(33)?),
            })
        })?;
        collect_rows(rows)
    }

    pub fn group_count(&self) -> Result<i64, StorageError> {
        let n = self
            .conn
            .query_row("SELECT COUNT(*) FROM groups", [], |r| r.get(0))?;
        Ok(n)
    }

    /// Detect whether this file looks like a Throne main DB (has `profiles` + `settings`).
    pub fn looks_like_throne_db(path: impl AsRef<Path>) -> bool {
        let Ok(conn) = Connection::open(path.as_ref()) else {
            return false;
        };
        let Ok(n): Result<i64, _> = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('profiles','groups','settings')",
            [],
            |r| r.get(0),
        ) else {
            return false;
        };
        n >= 2
    }
}

fn insert_route_rule(
    tx: &rusqlite::Transaction<'_>,
    profile_id: i64,
    order: i32,
    rule: &RouteRule,
) -> Result<(), StorageError> {
    tx.execute(
        r#"INSERT INTO route_rules (
            route_profile_id, rule_order, name, type, ip_version, network, protocol,
            inbound_json, domain_json, domain_suffix_json, domain_keyword_json, domain_regex_json,
            source_ip_cidr_json, source_ip_is_private, ip_cidr_json, ip_is_private,
            source_port_json, source_port_range_json, port_json, port_range_json,
            process_name_json, process_path_json, process_path_regex_json, rule_set_json,
            invert, outbound_id, action, reject_method, no_drop,
            override_address, override_port, sniffers_json, sniff_override_dest, strategy,
            wifi_ssid_json, wifi_bssid_json
        ) VALUES (
            ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,
            ?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33,?34,?35,?36
        )"#,
        params![
            profile_id,
            order,
            rule.name,
            rule.rule_type,
            rule.ip_version,
            rule.network,
            rule.protocol,
            to_json_array(&rule.inbound),
            to_json_array(&rule.domain),
            to_json_array(&rule.domain_suffix),
            to_json_array(&rule.domain_keyword),
            to_json_array(&rule.domain_regex),
            to_json_array(&rule.source_ip_cidr),
            rule.source_ip_is_private as i32,
            to_json_array(&rule.ip_cidr),
            rule.ip_is_private as i32,
            to_json_array(&rule.source_port),
            to_json_array(&rule.source_port_range),
            to_json_array(&rule.port),
            to_json_array(&rule.port_range),
            to_json_array(&rule.process_name),
            to_json_array(&rule.process_path),
            to_json_array(&rule.process_path_regex),
            to_json_array(&rule.rule_set),
            rule.invert as i32,
            rule.outbound_id,
            if rule.action.is_empty() {
                "route"
            } else {
                &rule.action
            },
            rule.reject_method,
            rule.no_drop as i32,
            rule.override_address,
            rule.override_port,
            to_json_array(&rule.sniffers),
            rule.sniff_override_dest as i32,
            rule.strategy,
            to_json_array(&rule.wifi_ssid),
            to_json_array(&rule.wifi_bssid),
        ],
    )?;
    Ok(())
}

/// Write outbound_json the way upstream does: compact ExportToJson object.
///
/// Always goes through domain normalization so Qt Throne can ParseFromJson
/// (requires `"type"`, no ParsedOutbound null dumps).
fn outbound_json_for_db(p: &Profile) -> String {
    p.export_outbound_json()
}

/// Parse upstream outbound_json → ParsedOutbound + optional tag name.
fn parse_outbound_json(s: &str) -> (ParsedOutbound, Option<String>) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(s) else {
        return (ParsedOutbound::default(), None);
    };
    // Prefer structured ParsedOutbound if we stored one
    if let Ok(p) = serde_json::from_value::<ParsedOutbound>(v.clone()) {
        if p.server.is_some() || p.uuid.is_some() || p.raw_json.is_some() {
            let name = p.tag.clone();
            return (p, name);
        }
    }
    let obj = match v.as_object() {
        Some(o) => o,
        None => {
            return (
                ParsedOutbound {
                    raw_json: Some(s.to_string()),
                    ..Default::default()
                },
                None,
            );
        }
    };
    let tag = obj
        .get("tag")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let mut outbound = ParsedOutbound {
        tag: tag.clone(),
        server: obj
            .get("server")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        server_port: obj
            .get("server_port")
            .and_then(|x| x.as_u64().map(|n| n as u16)),
        uuid: obj
            .get("uuid")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        password: obj
            .get("password")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        username: obj
            .get("username")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        method: obj
            .get("method")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        flow: obj
            .get("flow")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        raw_json: Some(s.to_string()),
        ..Default::default()
    };
    if let Some(tls) = obj.get("tls").and_then(|t| t.as_object()) {
        outbound.tls = tls.get("enabled").and_then(|x| x.as_bool()).or(Some(true));
        outbound.sni = tls
            .get("server_name")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        outbound.insecure = tls.get("insecure").and_then(|x| x.as_bool());
    }
    (outbound, tag)
}

fn json_str_list(opt: Option<String>) -> Vec<String> {
    let Some(s) = opt else {
        return Vec::new();
    };
    if s.is_empty() {
        return Vec::new();
    }
    serde_json::from_str(&s).unwrap_or_default()
}

fn to_json_array(list: &[String]) -> String {
    serde_json::to_string(list).unwrap_or_else(|_| "[]".into())
}

fn collect_rows<T, E>(rows: impl IntoIterator<Item = Result<T, E>>) -> Result<Vec<T>, StorageError>
where
    StorageError: From<E>,
{
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Merge known settings; preserve unknown keys already in the DB.
fn merge_settings_tx(
    tx: &rusqlite::Transaction<'_>,
    s: &AppSettings,
) -> Result<(), StorageError> {
    let pairs: Vec<(&str, String)> = vec![
        ("inbound_socks_port", s.inbound_socks_port.to_string()),
        ("inbound_address", s.inbound_address.clone()),
        ("test_url", s.test_latency_url.clone()),
        ("remote_dns", s.remote_dns.clone()),
        ("direct_dns", s.direct_dns.clone()),
        ("vpn_strict_route", bool_str(s.vpn_strict_route)),
        ("vpn_mtu", s.vpn_mtu.to_string()),
        ("vpn_tun_ipv4_cidr", s.vpn_tun_ipv4_cidr.clone()),
        (
            "disable_private_range_bypass",
            bool_str(s.disable_private_range_bypass),
        ),
        ("sub_show_change_popup", bool_str(s.sub_show_change_popup)),
        (
            "allow_stopping_active_profile",
            bool_str(s.allow_stopping_active_profile),
        ),
        ("show_config_security", bool_str(s.show_config_security)),
        ("current_route_id", s.current_route_id.to_string()),
        ("remember_id", s.remember_id.to_string()),
        ("remember_enable", bool_str(s.remember_enable)),
        ("start_with_system", bool_str(s.start_with_system)),
        ("system_proxy_enabled", bool_str(s.system_proxy_enabled)),
        ("tun_mode_enabled", bool_str(s.tun_mode_enabled)),
        ("system_dns_set", bool_str(s.system_dns_set)),
        ("theme", s.theme.clone()),
        ("log_level", s.log_level.clone()),
        ("ruleset_mirror", s.ruleset_mirror.as_id().to_string()),
        ("adblock_enable", bool_str(s.adblock_enable)),
        ("hk_start_stop", s.hk_start_stop.clone()),
        ("hk_import", s.hk_import.clone()),
        ("hk_save", s.hk_save.clone()),
        ("hk_url_test", s.hk_url_test.clone()),
        ("hk_copy_logs", s.hk_copy_logs.clone()),
        // Routing dialog
        ("remote_dns_strategy", s.remote_dns_strategy.clone()),
        ("direct_dns_strategy", s.direct_dns_strategy.clone()),
        ("dns_cache_capacity", s.dns_cache_capacity.to_string()),
        ("dns_disable_cache", bool_str(s.dns_disable_cache)),
        ("dns_disable_expire", bool_str(s.dns_disable_expire)),
        ("dns_reverse_mapping", bool_str(s.dns_reverse_mapping)),
        ("enable_dns_routing", bool_str(s.enable_dns_routing)),
        ("use_dns_object", bool_str(s.use_dns_object)),
        ("dns_object", s.dns_object.clone()),
        ("dns_final_out", s.dns_final_out.clone()),
        ("resolve_domain_strategy", s.resolve_domain_strategy.clone()),
        ("default_domain_strategy", s.default_domain_strategy.clone()),
        ("core_box_underlying_dns", s.core_box_underlying_dns.clone()),
        ("fake_dns", bool_str(s.fake_dns)),
        ("enable_dns_server", bool_str(s.enable_dns_server)),
        ("dns_server_listen_port", s.dns_server_listen_port.to_string()),
        ("dns_v4_resp", s.dns_v4_resp.clone()),
        ("dns_v6_resp", s.dns_v6_resp.clone()),
        ("dns_server_rules", to_json_array(&s.dns_server_rules)),
        ("dns_server_listen_lan", bool_str(s.dns_server_listen_lan)),
        ("enable_redirect", bool_str(s.enable_redirect)),
        ("redirect_listen_address", s.redirect_listen_address.clone()),
        ("redirect_listen_port", s.redirect_listen_port.to_string()),
        ("enable_warp", bool_str(s.enable_warp)),
        ("warp_ep", s.warp_ep.clone()),
        ("warp_private_key", s.warp_private_key.clone()),
        ("warp_public_key", s.warp_public_key.clone()),
        ("warp_ifc_addrs", to_json_array(&s.warp_ifc_addrs)),
        ("warp_reserved", to_json_array(&s.warp_reserved)),
    ];
    for (k, v) in pairs {
        tx.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![k, v],
        )?;
    }
    Ok(())
}

fn bool_str(v: bool) -> String {
    if v { "true".into() } else { "false".into() }
}

fn parse_bool(v: &str) -> bool {
    matches!(v, "true" | "1" | "yes")
}

fn apply_setting(s: &mut AppSettings, key: &str, value: &str) {
    match key {
        "inbound_socks_port" => {
            if let Ok(n) = value.parse() {
                s.inbound_socks_port = n;
            }
        }
        "inbound_address" => s.inbound_address = value.to_string(),
        "test_url" => s.test_latency_url = value.to_string(),
        "remote_dns" => s.remote_dns = value.to_string(),
        "direct_dns" => s.direct_dns = value.to_string(),
        "vpn_strict_route" => s.vpn_strict_route = parse_bool(value),
        "vpn_mtu" => {
            if let Ok(n) = value.parse() {
                s.vpn_mtu = n;
            }
        }
        "vpn_tun_ipv4_cidr" => {
            let t = value.trim();
            if !t.is_empty() {
                s.vpn_tun_ipv4_cidr = t.to_string();
            }
        }
        "disable_private_range_bypass" => s.disable_private_range_bypass = parse_bool(value),
        "sub_show_change_popup" => s.sub_show_change_popup = parse_bool(value),
        "allow_stopping_active_profile" => s.allow_stopping_active_profile = parse_bool(value),
        "show_config_security" => s.show_config_security = parse_bool(value),
        "current_route_id" => {
            if let Ok(n) = value.parse() {
                s.current_route_id = n;
            }
        }
        "remember_id" => {
            if let Ok(n) = value.parse() {
                s.remember_id = n;
            }
        }
        "remember_enable" => s.remember_enable = parse_bool(value),
        "start_with_system" => s.start_with_system = parse_bool(value),
        "system_proxy_enabled" => s.system_proxy_enabled = parse_bool(value),
        "tun_mode_enabled" => s.tun_mode_enabled = parse_bool(value),
        "system_dns_set" => s.system_dns_set = parse_bool(value),
        "theme" => s.theme = value.to_string(),
        "log_level" => s.log_level = value.to_string(),
        "ruleset_mirror" => {
            if let Ok(n) = value.parse::<i32>() {
                s.ruleset_mirror = throne_domain::RulesetMirror::from_id(n);
            } else {
                // Accept labels / snake names from hand-edited DBs.
                s.ruleset_mirror = match value.trim().to_ascii_lowercase().as_str() {
                    "github" | "gh" => throne_domain::RulesetMirror::Github,
                    "gcore" => throne_domain::RulesetMirror::Gcore,
                    "quantil" => throne_domain::RulesetMirror::Quantil,
                    "fastly" => throne_domain::RulesetMirror::Fastly,
                    "cdn" => throne_domain::RulesetMirror::Cdn,
                    _ => throne_domain::RulesetMirror::Cloudflare,
                };
            }
        }
        "adblock_enable" => s.adblock_enable = parse_bool(value),
        "hk_start_stop" => s.hk_start_stop = value.to_string(),
        "hk_import" => s.hk_import = value.to_string(),
        "hk_save" => s.hk_save = value.to_string(),
        "hk_url_test" => s.hk_url_test = value.to_string(),
        "hk_copy_logs" => s.hk_copy_logs = value.to_string(),
        "remote_dns_strategy" => s.remote_dns_strategy = value.to_string(),
        "direct_dns_strategy" => s.direct_dns_strategy = value.to_string(),
        "dns_cache_capacity" => {
            if let Ok(n) = value.parse() {
                s.dns_cache_capacity = n;
            }
        }
        "dns_disable_cache" => s.dns_disable_cache = parse_bool(value),
        "dns_disable_expire" => s.dns_disable_expire = parse_bool(value),
        "dns_reverse_mapping" => s.dns_reverse_mapping = parse_bool(value),
        "enable_dns_routing" => s.enable_dns_routing = parse_bool(value),
        "use_dns_object" => s.use_dns_object = parse_bool(value),
        "dns_object" => s.dns_object = value.to_string(),
        "dns_final_out" => s.dns_final_out = value.to_string(),
        "resolve_domain_strategy" => s.resolve_domain_strategy = value.to_string(),
        "default_domain_strategy" => s.default_domain_strategy = value.to_string(),
        "core_box_underlying_dns" => s.core_box_underlying_dns = value.to_string(),
        "fake_dns" => s.fake_dns = parse_bool(value),
        "enable_dns_server" => s.enable_dns_server = parse_bool(value),
        "dns_server_listen_port" => {
            if let Ok(n) = value.parse() {
                s.dns_server_listen_port = n;
            }
        }
        "dns_v4_resp" => s.dns_v4_resp = value.to_string(),
        "dns_v6_resp" => s.dns_v6_resp = value.to_string(),
        "dns_server_rules" => s.dns_server_rules = json_str_list(Some(value.to_string())),
        "dns_server_listen_lan" => s.dns_server_listen_lan = parse_bool(value),
        "enable_redirect" => s.enable_redirect = parse_bool(value),
        "redirect_listen_address" => s.redirect_listen_address = value.to_string(),
        "redirect_listen_port" => {
            if let Ok(n) = value.parse() {
                s.redirect_listen_port = n;
            }
        }
        "enable_warp" => s.enable_warp = parse_bool(value),
        "warp_ep" => s.warp_ep = value.to_string(),
        "warp_private_key" => s.warp_private_key = value.to_string(),
        "warp_public_key" => s.warp_public_key = value.to_string(),
        "warp_ifc_addrs" => s.warp_ifc_addrs = json_str_list(Some(value.to_string())),
        "warp_reserved" => s.warp_reserved = json_str_list(Some(value.to_string())),
        _ => {}
    }
}

/// Open default path, or first discovered original Throne DB if env not set.
pub fn open_default() -> Result<Database, StorageError> {
    let path = resolve_db_path();
    info!(path = %path.display(), "opening database");
    Database::open(path)
}

/// Prefer existing Throne data; only seed demo when DB is empty.
pub fn load_or_seed_demo(db: &Database) -> Result<AppState, StorageError> {
    let count = db.group_count()?;
    if count == 0 {
        // Don't clobber a real empty-but-initialized Throne install if settings exist.
        let settings_n: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM settings", [], |r| r.get(0))
            .optional()?
            .unwrap_or(0);
        if settings_n > 0 {
            warn!("empty groups but settings present — loading as-is");
            return db.load_state();
        }
        let state = AppState::with_demo_data();
        db.save_state(&state)?;
        info!("seeded demo data into empty database");
        Ok(state)
    } else {
        db.load_state()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use throne_domain::{ProfileSortColumn, ProfileType};

    #[test]
    fn sorted_profile_order_survives_database_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("throne.db");
        let mut state = AppState::empty();
        let group = state.add_group("G");
        let slow = state.add_profile(group, "slow", ProfileType::Vless);
        let fast = state.add_profile(group, "fast", ProfileType::Vless);
        state.set_profile_latency(slow, 200);
        state.set_profile_latency(fast, 40);
        state
            .sort_active_group_profiles(ProfileSortColumn::TestResult, true)
            .unwrap();

        Database::open(&path).unwrap().save_state(&state).unwrap();
        let reopened = Database::open(&path).unwrap().load_state().unwrap();

        assert_eq!(
            reopened
                .visible_profiles()
                .iter()
                .map(|profile| profile.id)
                .collect::<Vec<_>>(),
            vec![fast, slow],
        );
    }

    #[test]
    fn roundtrip_profiles_compatible_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("throne.db");
        let db = Database::open(&path).unwrap();
        let mut state = AppState::empty();
        let g = state.add_group("G1");
        let _id = state.add_profile(g, "node", ProfileType::Vless);
        let rid = state.add_route({
            let mut r = RouteProfile::new(0, "R1");
            r.rules.push(RouteRule {
                name: "block-ads".into(),
                rule_type: 3,
                outbound_id: DefaultOutbound::Block.as_id(),
                domain_suffix: vec!["ads.test".into()],
                action: "route".into(),
                ..Default::default()
            });
            r
        });
        assert!(rid > 0);
        db.save_state(&state).unwrap();

        // Schema checks
        let has_rules: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='route_rules'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has_rules, 1);
        let no_rules_json: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('route_profiles') WHERE name='rules_json'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(no_rules_json, 0);

        let loaded = db.load_state().unwrap();
        assert_eq!(loaded.all_groups().len(), 1);
        assert_eq!(loaded.all_profiles().len(), 1);
        assert_eq!(loaded.all_routes().len(), 1);
        assert_eq!(loaded.all_routes()[0].rules.len(), 1);
        assert_eq!(
            loaded.all_routes()[0].rules[0].outbound_id,
            DefaultOutbound::Block.as_id()
        );
    }

    #[test]
    fn load_upstream_shaped_outbound_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("throne.db");
        let db = Database::open(&path).unwrap();
        db.conn
            .execute(
                "INSERT INTO groups (id, name, profiles_json) VALUES (1, 'g', '[1]')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO groups_order (group_id, display_order) VALUES (1, 0)",
                [],
            )
            .unwrap();
        let ob = r#"{"type":"vless","tag":"HK","server":"1.2.3.4","server_port":443,"uuid":"u"}"#;
        db.conn
            .execute(
                "INSERT INTO profiles (id, type, name, gid, outbound_json) VALUES (1, 'vless', '', 1, ?1)",
                params![ob],
            )
            .unwrap();
        let state = db.load_state().unwrap();
        let p = state.all_profiles()[0];
        assert_eq!(p.name, "HK"); // recovered from tag
        assert_eq!(p.outbound.server.as_deref(), Some("1.2.3.4"));
        assert_eq!(p.profile_type, ProfileType::Vless);
    }

    #[test]
    fn save_repairs_parsed_outbound_dump_missing_type() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("throne.db");
        let db = Database::open(&path).unwrap();
        db.conn
            .execute(
                "INSERT INTO groups (id, name, profiles_json) VALUES (1, 'g', '[1]')",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO groups_order (group_id, display_order) VALUES (1, 0)",
                [],
            )
            .unwrap();
        // Legacy throne-rs dump: has server/tag but no type — breaks Qt ParseFromJson.
        let dump = r#"{"tag":"新加坡01","server":"sg01.example.com","server_port":8443,"uuid":null,"password":"secret","username":null,"method":null,"flow":null,"security":null,"alter_id":null,"transport":null,"host":null,"path":null,"service_name":null,"tls":true,"sni":"localhost","alpn":null,"fp":null,"pbk":null,"sid":null,"spx":null,"insecure":true,"plugin":null,"plugin_opts":null,"obfs":null,"up_mbps":null,"down_mbps":null,"congestion_control":null,"udp_relay_mode":null,"packet_encoding":null,"raw_json":null}"#;
        db.conn
            .execute(
                "INSERT INTO profiles (id, type, name, gid, outbound_json) VALUES (1, 'hysteria2', '新加坡01', 1, ?1)",
                params![dump],
            )
            .unwrap();
        let state = db.load_state().unwrap();
        db.save_state(&state).unwrap();
        let fixed: String = db
            .conn
            .query_row(
                "SELECT outbound_json FROM profiles WHERE id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&fixed).unwrap();
        assert_eq!(v["type"], "hysteria2");
        assert_eq!(v["tag"], "新加坡01");
        assert_eq!(v["server"], "sg01.example.com");
        assert_eq!(v["server_port"], 8443);
        assert_eq!(v["password"], "secret");
        assert_eq!(v["tls"]["enabled"], true);
        assert!(v.get("raw_json").is_none());
        assert!(v.get("uuid").is_none());
    }

    #[test]
    fn warp_bypass_id_is_minus_five() {
        assert_eq!(DefaultOutbound::WarpBypass.as_id(), -5);
    }

    #[test]
    fn preserve_unknown_settings_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("throne.db");
        let db = Database::open(&path).unwrap();
        db.conn
            .execute(
                "INSERT INTO settings (key, value) VALUES ('custom_legacy_flag', 'yes')",
                [],
            )
            .unwrap();
        let state = AppState::with_demo_data();
        db.save_state(&state).unwrap();
        let v: String = db
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key='custom_legacy_flag'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v, "yes");
    }
}
