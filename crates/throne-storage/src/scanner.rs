//! IP list and scanner persistence compatible with upstream 1.4.0-beta.1.

use crate::{Database, StorageError};
use rusqlite::{params, Connection, OptionalExtension, Row};
use throne_domain::{
    normalize_ip_cidr, IpList, IpListEntry, IpListRole, IpListSourceKind, IpScan, IpScanKind,
    IpScanMode, IpScanStatus, ScanConfig,
};

const ENTRIES_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS ip_list_entries (
    ip_list_id INTEGER NOT NULL, generation INTEGER NOT NULL DEFAULT 0,
    entry_order INTEGER NOT NULL, cidr TEXT NOT NULL, port INTEGER NOT NULL DEFAULT 0,
    latency_ms INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(ip_list_id,generation,entry_order),
    FOREIGN KEY(ip_list_id) REFERENCES ip_lists(id) ON DELETE CASCADE) WITHOUT ROWID";

pub(crate) fn ensure_schema(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS ip_lists (
        id INTEGER PRIMARY KEY, name TEXT NOT NULL DEFAULT '', related_test_id INTEGER NOT NULL DEFAULT -1,
        role INTEGER NOT NULL DEFAULT 0, source_kind INTEGER NOT NULL DEFAULT 0, source TEXT NOT NULL DEFAULT '',
        auto_update INTEGER NOT NULL DEFAULT 0, update_interval INTEGER NOT NULL DEFAULT 1440,
        last_update INTEGER NOT NULL DEFAULT 0, last_error TEXT NOT NULL DEFAULT '', sort_order INTEGER NOT NULL DEFAULT 0,
        entries_generation INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')), updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now')));
        CREATE TABLE IF NOT EXISTS ip_scans (
        id INTEGER PRIMARY KEY, name TEXT NOT NULL DEFAULT '', kind INTEGER NOT NULL DEFAULT 0,
        sort_order INTEGER NOT NULL DEFAULT 0, base_list_id INTEGER NOT NULL DEFAULT -1,
        result_list_id INTEGER NOT NULL DEFAULT -1, config_json TEXT NOT NULL DEFAULT '{}',
        status INTEGER NOT NULL DEFAULT 0, mode INTEGER NOT NULL DEFAULT 0,
        snapshot_list_id INTEGER NOT NULL DEFAULT -1, seed INTEGER NOT NULL DEFAULT 0,
        cursor INTEGER NOT NULL DEFAULT 0, total INTEGER NOT NULL DEFAULT 0, spec_hash TEXT NOT NULL DEFAULT '',
        rescan_snapshot_list_id INTEGER NOT NULL DEFAULT -1, rescan_cursor INTEGER NOT NULL DEFAULT 0,
        rescan_total INTEGER NOT NULL DEFAULT 0, found INTEGER NOT NULL DEFAULT 0, removed INTEGER NOT NULL DEFAULT 0,
        last_error TEXT NOT NULL DEFAULT '', started_at INTEGER NOT NULL DEFAULT 0, finished_at INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')), updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now')));")?;
    crate::schema::add_column_if_missing(
        conn,
        "entity_ids",
        "ip_list_last_id",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    crate::schema::add_column_if_missing(
        conn,
        "entity_ids",
        "ip_scan_last_id",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    crate::schema::add_column_if_missing(
        conn,
        "ip_lists",
        "entries_generation",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    conn.execute_batch(ENTRIES_SCHEMA)?;
    let mut columns = conn.prepare("PRAGMA table_info(ip_list_entries)")?;
    let has_generation = columns
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|n| n == "generation");
    drop(columns);
    if !has_generation {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch("ALTER TABLE ip_list_entries RENAME TO ip_list_entries_legacy")?;
        tx.execute_batch(ENTRIES_SCHEMA)?;
        tx.execute_batch(
            "INSERT INTO ip_list_entries (ip_list_id,generation,entry_order,cidr,port,latency_ms)
            SELECT ip_list_id,0,entry_order,cidr,port,latency_ms FROM ip_list_entries_legacy;
            DROP TABLE ip_list_entries_legacy;",
        )?;
        tx.commit()?;
    }
    conn.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS idx_ip_list_entries_target ON ip_list_entries(ip_list_id,generation,cidr,port);
        UPDATE entity_ids SET ip_list_last_id = MAX(ip_list_last_id,(SELECT COALESCE(MAX(id),0) FROM ip_lists)),
            ip_scan_last_id = MAX(ip_scan_last_id,(SELECT COALESCE(MAX(id),0) FROM ip_scans));")?;
    Ok(())
}

const LIST_SELECT: &str = "SELECT id,name,related_test_id,role,source_kind,source,auto_update,update_interval,last_update,last_error,
    (SELECT COUNT(*) FROM ip_list_entries WHERE ip_list_id=ip_lists.id AND generation=ip_lists.entries_generation)
    FROM ip_lists";

fn list_from_row(row: &Row<'_>) -> rusqlite::Result<IpList> {
    Ok(IpList {
        id: row.get(0)?,
        name: row.get(1)?,
        related_test_id: row.get(2)?,
        role: match row.get::<_, i32>(3)? {
            1 => IpListRole::ScanResult,
            2 => IpListRole::ScanSnapshot,
            _ => IpListRole::User,
        },
        source_kind: match row.get::<_, i32>(4)? {
            1 => IpListSourceKind::Url,
            2 => IpListSourceKind::RuleSet,
            _ => IpListSourceKind::Manual,
        },
        source: row.get(5)?,
        auto_update: row.get::<_, i32>(6)? != 0,
        update_interval: row.get(7)?,
        last_update: row.get(8)?,
        last_error: row.get(9)?,
        entry_count: row.get::<_, i64>(10)? as usize,
        entries: Vec::new(),
        entries_loaded: false,
    })
}

const SCAN_SELECT: &str = "SELECT id,name,kind,base_list_id,result_list_id,config_json,status,mode,snapshot_list_id,seed,
    cursor,total,spec_hash,rescan_snapshot_list_id,rescan_cursor,rescan_total,found,removed,last_error,started_at,finished_at FROM ip_scans";

fn scan_from_row(row: &Row<'_>) -> rusqlite::Result<IpScan> {
    let kind = if row.get::<_, i32>(2)? == 1 {
        IpScanKind::Warp
    } else {
        IpScanKind::Generic
    };
    let config = serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or(serde_json::Value::Null);
    Ok(IpScan {
        id: row.get(0)?,
        name: row.get(1)?,
        kind,
        base_list_id: row.get(3)?,
        result_list_id: row.get(4)?,
        config: ScanConfig::from_json(kind, &config),
        status: match row.get::<_, i32>(6)? {
            1 => IpScanStatus::Running,
            2 => IpScanStatus::Paused,
            3 => IpScanStatus::Completed,
            4 => IpScanStatus::Failed,
            _ => IpScanStatus::Idle,
        },
        mode: if row.get::<_, i32>(7)? == 1 {
            IpScanMode::RescanResult
        } else {
            IpScanMode::Initial
        },
        snapshot_list_id: row.get(8)?,
        seed: row.get::<_, i64>(9)? as u64,
        cursor: row.get::<_, i64>(10)? as u64,
        total: row.get::<_, i64>(11)? as u64,
        spec_hash: row.get(12)?,
        rescan_snapshot_list_id: row.get(13)?,
        rescan_cursor: row.get::<_, i64>(14)? as u64,
        rescan_total: row.get::<_, i64>(15)? as u64,
        found: row.get(16)?,
        removed: row.get(17)?,
        last_error: row.get(18)?,
        started_at: row.get(19)?,
        finished_at: row.get(20)?,
    })
}

impl Database {
    /// Visible headers in user-defined order; entries are loaded on demand.
    pub fn load_ip_lists(&self) -> Result<Vec<IpList>, StorageError> {
        let mut stmt = self.conn.prepare(&format!(
            "{LIST_SELECT} WHERE role != 2 ORDER BY sort_order,id"
        ))?;
        let rows = stmt.query_map([], list_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Read header and entries from one SQLite snapshot.
    pub fn load_ip_list(&self, id: i64) -> Result<Option<IpList>, StorageError> {
        let tx = self.conn.unchecked_transaction()?;
        let Some(mut list) = tx
            .query_row(&format!("{LIST_SELECT} WHERE id=?"), [id], list_from_row)
            .optional()?
        else {
            return Ok(None);
        };
        {
            let mut stmt = tx.prepare("SELECT cidr,port,latency_ms FROM ip_list_entries
                WHERE ip_list_id=? AND generation=(SELECT entries_generation FROM ip_lists WHERE id=?) ORDER BY entry_order")?;
            list.entries = stmt
                .query_map([id, id], |row| {
                    Ok(IpListEntry {
                        cidr: row.get(0)?,
                        port: row.get(1)?,
                        latency_ms: row.get(2)?,
                    })
                })?
                .collect::<Result<_, _>>()?;
        }
        list.entry_count = list.entries.len();
        list.entries_loaded = true;
        tx.commit()?;
        Ok(Some(list))
    }

    /// Save a header, replacing entries only when they have been loaded.
    /// Each entry replacement commits the generation switch atomically.
    pub fn save_ip_list(&self, list: &mut IpList) -> Result<(), StorageError> {
        let mut unique = Vec::new();
        let mut seen = std::collections::HashSet::new();
        if list.entries_loaded {
            for entry in &list.entries {
                let cidr = normalize_ip_cidr(&entry.cidr).ok_or_else(|| {
                    StorageError::Msg(format!("invalid IP list entry: {}", entry.cidr))
                })?;
                if seen.insert((cidr.clone(), entry.port)) {
                    unique.push(IpListEntry {
                        cidr,
                        port: entry.port,
                        latency_ms: entry.latency_ms,
                    });
                }
            }
        }
        let tx = self.conn.unchecked_transaction()?;
        let id = if list.id < 0 {
            tx.query_row(
                "UPDATE entity_ids SET ip_list_last_id=ip_list_last_id+1 RETURNING ip_list_last_id",
                [],
                |row| row.get::<_, i64>(0),
            )?
        } else {
            list.id
        };
        tx.execute("INSERT INTO ip_lists (id,name,related_test_id,role,source_kind,source,auto_update,update_interval,last_update,last_error,sort_order)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,(SELECT COALESCE(MAX(sort_order),0)+1 FROM ip_lists))
            ON CONFLICT(id) DO UPDATE SET name=excluded.name,related_test_id=excluded.related_test_id,role=excluded.role,
            source_kind=excluded.source_kind,source=excluded.source,auto_update=excluded.auto_update,update_interval=excluded.update_interval,
            last_update=excluded.last_update,last_error=excluded.last_error,updated_at=strftime('%s','now')",
            params![id,list.name,list.related_test_id,list.role as i32,list.source_kind as i32,list.source,list.auto_update as i32,
                list.update_interval,list.last_update,list.last_error])?;
        if list.entries_loaded {
            let generation: i64 = tx.query_row(
                "SELECT entries_generation FROM ip_lists WHERE id=?",
                [id],
                |r| r.get(0),
            )?;
            let next = generation
                .checked_add(1)
                .ok_or_else(|| StorageError::Msg("IP list generation exhausted".into()))?;
            // Remove a generation left by an interrupted upstream staged writer.
            tx.execute(
                "DELETE FROM ip_list_entries WHERE ip_list_id=? AND generation=?",
                params![id, next],
            )?;
            {
                let mut stmt = tx.prepare("INSERT INTO ip_list_entries (ip_list_id,generation,entry_order,cidr,port,latency_ms) VALUES (?,?,?,?,?,?)")?;
                for (order, entry) in unique.iter().enumerate() {
                    stmt.execute(params![
                        id,
                        next,
                        order as i64,
                        entry.cidr,
                        entry.port,
                        entry.latency_ms
                    ])?;
                }
            }
            tx.execute(
                "UPDATE ip_lists SET entries_generation=? WHERE id=?",
                params![next, id],
            )?;
            tx.execute(
                "DELETE FROM ip_list_entries WHERE ip_list_id=? AND generation!=?",
                params![id, next],
            )?;
        }
        tx.execute(
            "UPDATE entity_ids SET ip_list_last_id=MAX(ip_list_last_id,?)",
            [id],
        )?;
        tx.commit()?;
        list.id = id;
        if list.entries_loaded {
            list.entries = unique;
            list.entry_count = list.entries.len();
        }
        Ok(())
    }

    pub fn delete_ip_list(&self, id: i64) -> Result<(), StorageError> {
        self.conn.execute("DELETE FROM ip_lists WHERE id=?", [id])?;
        Ok(())
    }

    pub fn update_ip_lists_order(&self, ids: &[i64]) -> Result<(), StorageError> {
        let tx = self.conn.unchecked_transaction()?;
        for (index, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE ip_lists SET sort_order=? WHERE id=?",
                params![index as i64 + 1, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_ip_scans(&self) -> Result<Vec<IpScan>, StorageError> {
        let mut stmt = self
            .conn
            .prepare(&format!("{SCAN_SELECT} ORDER BY sort_order,id"))?;
        let rows = stmt.query_map([], scan_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn load_ip_scan(&self, id: i64) -> Result<Option<IpScan>, StorageError> {
        Ok(self
            .conn
            .query_row(&format!("{SCAN_SELECT} WHERE id=?"), [id], scan_from_row)
            .optional()?)
    }

    pub fn save_ip_scan(&self, scan: &mut IpScan) -> Result<(), StorageError> {
        let tx = self.conn.unchecked_transaction()?;
        let id = if scan.id < 0 {
            tx.query_row(
                "UPDATE entity_ids SET ip_scan_last_id=ip_scan_last_id+1 RETURNING ip_scan_last_id",
                [],
                |r| r.get(0),
            )?
        } else {
            scan.id
        };
        let mut config = scan.config.clone();
        config.normalize();
        tx.execute("INSERT INTO ip_scans (id,name,kind,base_list_id,result_list_id,config_json,status,mode,snapshot_list_id,seed,
            cursor,total,spec_hash,rescan_snapshot_list_id,rescan_cursor,rescan_total,found,removed,last_error,started_at,finished_at,sort_order)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,(SELECT COALESCE(MAX(sort_order),0)+1 FROM ip_scans))
            ON CONFLICT(id) DO UPDATE SET name=excluded.name,kind=excluded.kind,base_list_id=excluded.base_list_id,result_list_id=excluded.result_list_id,
            config_json=excluded.config_json,status=excluded.status,mode=excluded.mode,snapshot_list_id=excluded.snapshot_list_id,
            seed=excluded.seed,cursor=excluded.cursor,total=excluded.total,spec_hash=excluded.spec_hash,rescan_snapshot_list_id=excluded.rescan_snapshot_list_id,
            rescan_cursor=excluded.rescan_cursor,rescan_total=excluded.rescan_total,found=excluded.found,removed=excluded.removed,
            last_error=excluded.last_error,started_at=excluded.started_at,finished_at=excluded.finished_at,updated_at=strftime('%s','now')",
            params![id,scan.name,scan.kind as i32,scan.base_list_id,scan.result_list_id,serde_json::to_string(&config)?,scan.status as i32,
                scan.mode as i32,scan.snapshot_list_id,scan.seed as i64,scan.cursor as i64,scan.total as i64,scan.spec_hash,
                scan.rescan_snapshot_list_id,scan.rescan_cursor as i64,scan.rescan_total as i64,scan.found,scan.removed,scan.last_error,scan.started_at,scan.finished_at])?;
        tx.execute(
            "UPDATE entity_ids SET ip_scan_last_id=MAX(ip_scan_last_id,?)",
            [id],
        )?;
        tx.commit()?;
        scan.id = id;
        scan.config = config;
        Ok(())
    }

    pub fn save_ip_scan_progress(
        &self,
        id: i64,
        mode: IpScanMode,
        cursor: u64,
        total: u64,
        found: i32,
        removed: i32,
    ) -> Result<(), StorageError> {
        let (cursor_column, total_column) = if mode == IpScanMode::RescanResult {
            ("rescan_cursor", "rescan_total")
        } else {
            ("cursor", "total")
        };
        self.conn.execute(&format!("UPDATE ip_scans SET mode=?,{cursor_column}=?,{total_column}=?,found=?,removed=?,updated_at=strftime('%s','now') WHERE id=?"),
            params![mode as i32,cursor as i64,total as i64,found,removed,id])?;
        Ok(())
    }

    pub fn save_ip_scan_status(
        &self,
        id: i64,
        status: IpScanStatus,
        error: &str,
    ) -> Result<(), StorageError> {
        self.conn.execute(
            "UPDATE ip_scans SET status=?,last_error=?,updated_at=strftime('%s','now') WHERE id=?",
            params![status as i32, error, id],
        )?;
        Ok(())
    }

    pub fn delete_ip_scan(&self, id: i64) -> Result<(), StorageError> {
        self.conn.execute("DELETE FROM ip_scans WHERE id=?", [id])?;
        Ok(())
    }

    /// Call once at application startup, before starting workers.
    pub fn normalize_interrupted_scans(&self) -> Result<usize, StorageError> {
        Ok(self
            .conn
            .execute("UPDATE ip_scans SET status=2 WHERE status=1", [])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use throne_domain::{AppState, EndpointSource};

    fn db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path().join("throne.db")).unwrap();
        (dir, db)
    }

    #[test]
    fn list_roundtrip_canonicalizes_deduplicates_and_header_save_preserves_entries() {
        let (_dir, db) = db();
        let mut list = IpList::new("CDN");
        list.entries = vec![
            IpListEntry {
                cidr: "192.0.2.19/24".into(),
                port: 443,
                latency_ms: 12,
            },
            IpListEntry {
                cidr: "192.0.2.0/24".into(),
                port: 443,
                latency_ms: 9,
            },
        ];
        db.save_ip_list(&mut list).unwrap();
        assert!(list.id > 0);
        assert_eq!(list.entries.len(), 1);
        let mut header = db.load_ip_lists().unwrap().remove(0);
        assert!(!header.entries_loaded);
        assert_eq!(header.entry_count, 1);
        header.name = "renamed".into();
        db.save_ip_list(&mut header).unwrap();
        let loaded = db.load_ip_list(list.id).unwrap().unwrap();
        assert_eq!(loaded.name, "renamed");
        assert_eq!(loaded.entries, list.entries);
        let mut snapshot = IpList::new("snapshot");
        snapshot.role = IpListRole::ScanSnapshot;
        db.save_ip_list(&mut snapshot).unwrap();
        assert_eq!(db.load_ip_lists().unwrap().len(), 1);
        db.delete_ip_list(list.id).unwrap();
        assert!(db.load_ip_list(list.id).unwrap().is_none());
        assert_eq!(
            db.conn
                .query_row("SELECT COUNT(*) FROM ip_list_entries", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn invalid_replacement_leaves_published_generation_unchanged() {
        let (_dir, db) = db();
        let mut list = IpList::new("valid");
        list.entries = vec![IpListEntry {
            cidr: "::1".into(),
            port: 443,
            latency_ms: 1,
        }];
        db.save_ip_list(&mut list).unwrap();
        let old = list.clone();
        list.name = "bad update".into();
        list.entries[0].cidr = "invalid".into();
        assert!(db.save_ip_list(&mut list).is_err());
        assert_eq!(db.load_ip_list(old.id).unwrap().unwrap(), old);
    }

    #[test]
    fn scan_resume_and_unsigned_seed_roundtrip_and_normalization_is_explicit() {
        let (dir, db) = db();
        let mut scan = IpScan::new("test", IpScanKind::Warp);
        scan.seed = u64::MAX;
        scan.cursor = 7;
        scan.total = 99;
        scan.snapshot_list_id = 5;
        scan.status = IpScanStatus::Running;
        db.save_ip_scan(&mut scan).unwrap();
        assert_eq!(db.load_ip_scan(scan.id).unwrap().unwrap(), scan);
        let other = Database::open(dir.path().join("throne.db")).unwrap();
        assert_eq!(
            other.load_ip_scan(scan.id).unwrap().unwrap().status,
            IpScanStatus::Running
        );
        assert_eq!(db.normalize_interrupted_scans().unwrap(), 1);
        assert_eq!(
            db.load_ip_scan(scan.id).unwrap().unwrap().status,
            IpScanStatus::Paused
        );
        db.save_ip_scan_progress(scan.id, IpScanMode::RescanResult, 3, 10, 4, 1)
            .unwrap();
        let loaded = db.load_ip_scan(scan.id).unwrap().unwrap();
        assert_eq!(
            (loaded.active_cursor(), loaded.active_total(), loaded.cursor),
            (3, 10, 7)
        );
    }

    #[test]
    fn full_state_save_preserves_endpoint_and_upstream_subscription_metadata() {
        let (_dir, db) = db();
        let mut state = AppState::with_demo_data();
        let gid = state.active_group_id();
        let pid = state.all_profiles()[0].id;
        state.settings_mut().kill_switch = true;
        state.settings_mut().vpn_auto_redirect = false;
        state
            .set_group_endpoint(gid, EndpointSource::IpList { list: 17 })
            .unwrap();
        state
            .set_profile_endpoint(pid, EndpointSource::Own)
            .unwrap();
        db.save_state(&state).unwrap();
        db.conn.execute("UPDATE groups SET sub_options_json='{\"update_insecure\":true}',sub_metadata_json='{\"expire\":123}' WHERE id=?", [gid]).unwrap();
        db.conn
            .execute("UPDATE profiles SET latency_at=12345 WHERE id=?", [pid])
            .unwrap();
        let loaded = db.load_state().unwrap();
        assert!(loaded.settings().kill_switch);
        assert!(!loaded.settings().vpn_auto_redirect);
        assert_eq!(
            loaded.group(gid).unwrap().endpoint,
            EndpointSource::IpList { list: 17 }
        );
        assert_eq!(loaded.profile(pid).unwrap().endpoint, EndpointSource::Own);
        assert_eq!(loaded.profile(pid).unwrap().latency_at, 12345);
        db.save_state(&loaded).unwrap();
        let roundtrip = db.load_state().unwrap();
        assert_eq!(
            roundtrip.group(gid).unwrap().sub_options,
            serde_json::json!({"update_insecure":true})
        );
        assert_eq!(
            roundtrip.group(gid).unwrap().sub_metadata,
            serde_json::json!({"expire":123})
        );
        assert_eq!(
            roundtrip.profile(pid).unwrap().profile_type,
            state.profile(pid).unwrap().profile_type
        );
    }

    #[test]
    fn legacy_list_entries_migrate_without_losing_order_ports_or_latency() {
        let (_dir, db) = db();
        db.conn.execute_batch("DROP TABLE ip_list_entries; CREATE TABLE ip_list_entries (
            ip_list_id INTEGER NOT NULL, entry_order INTEGER NOT NULL, cidr TEXT NOT NULL, port INTEGER NOT NULL DEFAULT 0,
            latency_ms INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(ip_list_id,entry_order));
            INSERT INTO ip_lists(id,name) VALUES (41,'legacy');
            INSERT INTO ip_list_entries VALUES(41,0,'192.0.2.1',443,12);") .unwrap();
        ensure_schema(&db.conn).unwrap();
        let list = db.load_ip_list(41).unwrap().unwrap();
        assert_eq!(
            list.entries[0],
            IpListEntry {
                cidr: "192.0.2.1".into(),
                port: 443,
                latency_ms: 12
            }
        );
        let mut new = IpList::new("new");
        db.save_ip_list(&mut new).unwrap();
        assert_eq!(new.id, 42);
    }
}
