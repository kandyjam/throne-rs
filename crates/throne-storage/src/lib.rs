//! SQLite store with table shapes matching upstream Throne
//! (`ProfilesRepo` / `GroupsRepo` / `SettingsRepo`).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use thiserror::Error;
use tracing::info;

use throne_domain::{
    AppSettings, AppState, Group, GroupId, ParsedOutbound, Profile, ProfileId, ProfileType,
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
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        let db = Self {
            conn,
            path: path.as_ref().to_path_buf(),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&self) -> Result<(), StorageError> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS groups (
                id INTEGER PRIMARY KEY,
                archive INTEGER NOT NULL DEFAULT 0,
                skip_auto_update INTEGER NOT NULL DEFAULT 0,
                name TEXT NOT NULL DEFAULT '',
                url TEXT,
                info TEXT,
                sub_last_update INTEGER NOT NULL DEFAULT 0,
                front_proxy_id INTEGER NOT NULL DEFAULT -1,
                landing_proxy_id INTEGER NOT NULL DEFAULT -1,
                column_width_json TEXT,
                profiles_json TEXT NOT NULL DEFAULT '[]',
                scroll_last_profile INTEGER NOT NULL DEFAULT -1,
                auto_clear_unavailable INTEGER NOT NULL DEFAULT 0,
                test_sort_by INTEGER NOT NULL DEFAULT 0,
                traffic_sort_by INTEGER NOT NULL DEFAULT 0,
                test_items_to_show INTEGER NOT NULL DEFAULT 0,
                type_sort_by INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            );

            CREATE TABLE IF NOT EXISTS groups_order (
                group_id INTEGER NOT NULL PRIMARY KEY,
                display_order INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS profiles (
                id INTEGER PRIMARY KEY,
                type TEXT NOT NULL,
                name TEXT,
                gid INTEGER NOT NULL DEFAULT 0,
                latency INTEGER NOT NULL DEFAULT 0,
                dl_speed TEXT,
                ul_speed TEXT,
                test_country TEXT,
                ip_out TEXT,
                outbound_json TEXT NOT NULL,
                traffic_dl INTEGER NOT NULL DEFAULT 0,
                traffic_up INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                FOREIGN KEY(gid) REFERENCES groups(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_profiles_name ON profiles(name);

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS entity_ids (
                profile_last_id INTEGER NOT NULL DEFAULT 0,
                group_last_id INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )?;
        // Ensure single row for entity_ids
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM entity_ids", [], |r| r.get(0))?;
        if n == 0 {
            self.conn
                .execute("INSERT INTO entity_ids (profile_last_id, group_last_id) VALUES (0, 0)", [])?;
        }
        Ok(())
    }

    pub fn load_state(&self) -> Result<AppState, StorageError> {
        let mut state = AppState::empty();
        let groups = self.load_groups()?;
        let profiles = self.load_profiles()?;
        let order = self.load_group_order()?;
        let settings = self.load_settings()?;
        let order = if order.is_empty() {
            groups.iter().map(|g| g.id).collect()
        } else {
            order
        };
        state.load_snapshot(groups, profiles, order, settings);
        Ok(state)
    }

    pub fn save_state(&self, state: &AppState) -> Result<(), StorageError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM profiles", [])?;
        tx.execute("DELETE FROM groups_order", [])?;
        tx.execute("DELETE FROM groups", [])?;

        for g in state.all_groups() {
            let profiles_json = serde_json::to_string(&g.profile_ids)?;
            tx.execute(
                r#"INSERT INTO groups (
                    id, archive, skip_auto_update, name, url, info,
                    sub_last_update, profiles_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
                params![
                    g.id,
                    g.archive as i32,
                    g.skip_auto_update as i32,
                    g.name,
                    g.url,
                    g.info,
                    g.sub_last_update.map(|t| t.timestamp()).unwrap_or(0),
                    profiles_json,
                ],
            )?;
        }

        for (idx, gid) in state.group_order().iter().enumerate() {
            tx.execute(
                "INSERT INTO groups_order (group_id, display_order) VALUES (?1, ?2)",
                params![gid, idx as i64],
            )?;
        }

        for p in state.all_profiles() {
            tx.execute(
                r#"INSERT INTO profiles (
                    id, type, name, gid, latency, dl_speed, ul_speed,
                    test_country, ip_out, outbound_json, traffic_dl, traffic_up
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"#,
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
                    if p.outbound_json.is_empty() {
                        p.outbound.to_db_json()
                    } else {
                        p.outbound_json.clone()
                    },
                    p.traffic_downlink,
                    p.traffic_uplink,
                ],
            )?;
        }

        // entity id counters
        let max_p = state
            .all_profiles()
            .iter()
            .map(|p| p.id)
            .max()
            .unwrap_or(0);
        let max_g = state.all_groups().iter().map(|g| g.id).max().unwrap_or(0);
        tx.execute(
            "UPDATE entity_ids SET profile_last_id = ?1, group_last_id = ?2",
            params![max_p, max_g],
        )?;

        save_settings_tx(&tx, state.settings())?;
        tx.commit()?;
        info!(path = %self.path.display(), "database saved");
        Ok(())
    }

    fn load_groups(&self) -> Result<Vec<Group>, StorageError> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, archive, skip_auto_update, name, url, info,
                      sub_last_update, profiles_json
               FROM groups"#,
        )?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let archive: i32 = row.get(1)?;
            let skip: i32 = row.get(2)?;
            let name: String = row.get(3)?;
            let url: String = row.get::<_, Option<String>>(4)?.unwrap_or_default();
            let info: String = row.get::<_, Option<String>>(5)?.unwrap_or_default();
            let _sub_last: i64 = row.get(6)?;
            let profiles_json: String = row.get(7)?;
            let profile_ids: Vec<ProfileId> =
                serde_json::from_str(&profiles_json).unwrap_or_default();
            Ok(Group {
                id,
                name,
                url,
                info,
                archive: archive != 0,
                skip_auto_update: skip != 0,
                sub_last_update: None,
                profile_ids,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    fn load_profiles(&self) -> Result<Vec<Profile>, StorageError> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, type, name, gid, latency, dl_speed, ul_speed,
                      test_country, ip_out, outbound_json, traffic_dl, traffic_up
               FROM profiles"#,
        )?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let ty: String = row.get(1)?;
            let name: String = row.get::<_, Option<String>>(2)?.unwrap_or_default();
            let gid: i64 = row.get(3)?;
            let latency: i32 = row.get(4)?;
            let dl: String = row.get::<_, Option<String>>(5)?.unwrap_or_default();
            let ul: String = row.get::<_, Option<String>>(6)?.unwrap_or_default();
            let country: String = row.get::<_, Option<String>>(7)?.unwrap_or_default();
            let ip: String = row.get::<_, Option<String>>(8)?.unwrap_or_default();
            let outbound_json: String = row.get(9)?;
            let tdl: i64 = row.get(10)?;
            let tul: i64 = row.get(11)?;
            let profile_type = ProfileType::from_upstream(&ty).unwrap_or(ProfileType::Custom);
            let outbound: ParsedOutbound =
                serde_json::from_str(&outbound_json).unwrap_or_default();
            let insecure = outbound.insecure.unwrap_or(false);
            Ok(Profile {
                id,
                group_id: gid,
                name,
                profile_type,
                latency_ms: latency,
                download_speed: dl,
                upload_speed: ul,
                test_country: country,
                traffic_downlink: tdl,
                traffic_uplink: tul,
                ip_out: ip,
                outbound_json,
                outbound,
                insecure,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    fn load_group_order(&self) -> Result<Vec<GroupId>, StorageError> {
        let mut stmt = self
            .conn
            .prepare("SELECT group_id FROM groups_order ORDER BY display_order ASC")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
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

    pub fn group_count(&self) -> Result<i64, StorageError> {
        let n = self
            .conn
            .query_row("SELECT COUNT(*) FROM groups", [], |r| r.get(0))?;
        Ok(n)
    }
}

fn save_settings_tx(tx: &rusqlite::Transaction<'_>, s: &AppSettings) -> Result<(), StorageError> {
    tx.execute("DELETE FROM settings", [])?;
    let pairs: Vec<(&str, String)> = vec![
        ("inbound_socks_port", s.inbound_socks_port.to_string()),
        ("inbound_address", s.inbound_address.clone()),
        ("test_url", s.test_latency_url.clone()),
        ("remote_dns", s.remote_dns.clone()),
        ("direct_dns", s.direct_dns.clone()),
        ("vpn_strict_route", bool_str(s.vpn_strict_route)),
        ("vpn_mtu", s.vpn_mtu.to_string()),
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
        ("system_proxy_enabled", bool_str(s.system_proxy_enabled)),
        ("tun_mode_enabled", bool_str(s.tun_mode_enabled)),
        ("theme", s.theme.clone()),
        ("log_level", s.log_level.clone()),
    ];
    for (k, v) in pairs {
        tx.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)",
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
        "system_proxy_enabled" => s.system_proxy_enabled = parse_bool(value),
        "tun_mode_enabled" => s.tun_mode_enabled = parse_bool(value),
        "theme" => s.theme = value.to_string(),
        "log_level" => s.log_level = value.to_string(),
        _ => {}
    }
}

/// Default DB path: `~/Library/Application Support/throne-rs/throne.db` on macOS,
/// `~/.local/share/throne-rs/throne.db` elsewhere (XDG).
pub fn default_db_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("throne-rs").join("throne.db")
}

/// Open default DB or return empty in-memory-backed path for first run.
pub fn open_default() -> Result<Database, StorageError> {
    Database::open(default_db_path())
}

/// Seed demo data only when DB has no groups.
pub fn load_or_seed_demo(db: &Database) -> Result<AppState, StorageError> {
    if db.group_count()? == 0 {
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
    use throne_domain::ProfileType;

    #[test]
    fn roundtrip_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let db = Database::open(&path).unwrap();
        let mut state = AppState::empty();
        let g = state.add_group("G1");
        let _id = state.add_profile(g, "node", ProfileType::Vless);
        db.save_state(&state).unwrap();

        let loaded = db.load_state().unwrap();
        assert_eq!(loaded.all_groups().len(), 1);
        assert_eq!(loaded.all_profiles().len(), 1);
        assert_eq!(loaded.all_profiles()[0].name, "node");
        assert_eq!(loaded.settings().remote_dns, "https://dns.google/dns-query");
    }
}
