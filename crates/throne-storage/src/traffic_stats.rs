//! Historical traffic stats DB (`throne_stats.db`) — wire-compatible with upstream
//! `TrafficStatsRepo` (minute + hour tiers, config/app series, meta).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;

/// Reserved profile id for direct (non-proxy) traffic — upstream `DIRECT_STAT_PROFILE_ID`.
pub const DIRECT_STAT_PROFILE_ID: i64 = -101;

#[derive(Debug, Error)]
pub enum TrafficStatsError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{0}")]
    Msg(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigTrafficRow {
    pub bucket_start: i64,
    pub profile_id: i64,
    pub up: i64,
    pub down: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppTrafficRow {
    pub bucket_start: i64,
    pub process_name: String,
    pub up: i64,
    pub down: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigUsage {
    pub profile_id: i64,
    pub up: i64,
    pub down: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppUsage {
    pub process_name: String,
    pub up: i64,
    pub down: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrafficSeriesPoint {
    pub bucket_start: i64,
    pub up: i64,
    pub down: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigMetaRow {
    pub profile_id: i64,
    pub name: String,
    pub group_name: String,
    pub type_name: String,
    pub server_address: String,
    pub first_seen: i64,
    pub last_seen: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(dead_code)] // reserved for app-dimension meta UI (upstream app_meta)
pub struct AppMetaRow {
    pub process_name: String,
    pub last_path: String,
    pub first_seen: i64,
    pub last_seen: i64,
}

/// Separate SQLite file next to `throne.db` (upstream design).
pub struct TrafficStatsDb {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl TrafficStatsDb {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, TrafficStatsError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| TrafficStatsError::Msg(e.to_string()))?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;",
        )?;
        let db = Self {
            conn: Mutex::new(conn),
            path,
        };
        db.create_tables()?;
        Ok(db)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn create_tables(&self) -> Result<(), TrafficStatsError> {
        let conn = self.conn.lock().map_err(|e| TrafficStatsError::Msg(e.to_string()))?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS config_traffic_minute (
                bucket_start INTEGER NOT NULL,
                profile_id   INTEGER NOT NULL,
                up           INTEGER NOT NULL DEFAULT 0,
                down         INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (bucket_start, profile_id)
            );
            CREATE TABLE IF NOT EXISTS config_traffic_hour (
                bucket_start INTEGER NOT NULL,
                profile_id   INTEGER NOT NULL,
                up           INTEGER NOT NULL DEFAULT 0,
                down         INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (bucket_start, profile_id)
            );
            CREATE TABLE IF NOT EXISTS app_traffic_minute (
                bucket_start INTEGER NOT NULL,
                process_name TEXT NOT NULL,
                up           INTEGER NOT NULL DEFAULT 0,
                down         INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (bucket_start, process_name)
            );
            CREATE TABLE IF NOT EXISTS app_traffic_hour (
                bucket_start INTEGER NOT NULL,
                process_name TEXT NOT NULL,
                up           INTEGER NOT NULL DEFAULT 0,
                down         INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (bucket_start, process_name)
            );
            CREATE TABLE IF NOT EXISTS config_meta (
                profile_id     INTEGER PRIMARY KEY,
                name           TEXT,
                group_name     TEXT,
                type           TEXT,
                server_address TEXT,
                first_seen     INTEGER NOT NULL DEFAULT 0,
                last_seen      INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS app_meta (
                process_name TEXT PRIMARY KEY,
                last_path    TEXT,
                first_seen   INTEGER NOT NULL DEFAULT 0,
                last_seen    INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )?;
        Ok(())
    }

    pub fn upsert_config_minute_batch(
        &self,
        rows: &[ConfigTrafficRow],
    ) -> Result<(), TrafficStatsError> {
        if rows.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().map_err(|e| TrafficStatsError::Msg(e.to_string()))?;
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO config_traffic_minute (bucket_start, profile_id, up, down)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(bucket_start, profile_id) DO UPDATE SET
                   up = up + excluded.up, down = down + excluded.down",
            )?;
            for r in rows {
                stmt.execute(params![r.bucket_start, r.profile_id, r.up, r.down])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn upsert_app_minute_batch(&self, rows: &[AppTrafficRow]) -> Result<(), TrafficStatsError> {
        if rows.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().map_err(|e| TrafficStatsError::Msg(e.to_string()))?;
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO app_traffic_minute (bucket_start, process_name, up, down)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(bucket_start, process_name) DO UPDATE SET
                   up = up + excluded.up, down = down + excluded.down",
            )?;
            for r in rows {
                stmt.execute(params![r.bucket_start, r.process_name, r.up, r.down])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn upsert_config_meta(&self, m: &ConfigMetaRow) -> Result<(), TrafficStatsError> {
        let conn = self.conn.lock().map_err(|e| TrafficStatsError::Msg(e.to_string()))?;
        conn.execute(
            "INSERT INTO config_meta
             (profile_id, name, group_name, type, server_address, first_seen, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(profile_id) DO UPDATE SET
               name = excluded.name, group_name = excluded.group_name, type = excluded.type,
               server_address = excluded.server_address, last_seen = excluded.last_seen",
            params![
                m.profile_id,
                m.name,
                m.group_name,
                m.type_name,
                m.server_address,
                m.first_seen,
                m.last_seen
            ],
        )?;
        Ok(())
    }

    pub fn upsert_app_meta(
        &self,
        process_name: &str,
        last_path: &str,
        now_secs: i64,
    ) -> Result<(), TrafficStatsError> {
        let conn = self.conn.lock().map_err(|e| TrafficStatsError::Msg(e.to_string()))?;
        conn.execute(
            "INSERT INTO app_meta (process_name, last_path, first_seen, last_seen)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(process_name) DO UPDATE SET
               last_path = excluded.last_path, last_seen = excluded.last_seen",
            params![process_name, last_path, now_secs, now_secs],
        )?;
        Ok(())
    }

    /// Aggregate minute rows older than `older_than_secs` into hour tier, then delete them.
    pub fn rollup_minute_to_hour(&self, older_than_secs: i64) -> Result<(), TrafficStatsError> {
        let conn = self.conn.lock().map_err(|e| TrafficStatsError::Msg(e.to_string()))?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO config_traffic_hour (bucket_start, profile_id, up, down)
             SELECT (bucket_start / 3600) * 3600, profile_id, SUM(up), SUM(down)
             FROM config_traffic_minute WHERE bucket_start < ?1
             GROUP BY (bucket_start / 3600) * 3600, profile_id
             ON CONFLICT(bucket_start, profile_id) DO UPDATE SET
               up = up + excluded.up, down = down + excluded.down",
            params![older_than_secs],
        )?;
        tx.execute(
            "DELETE FROM config_traffic_minute WHERE bucket_start < ?1",
            params![older_than_secs],
        )?;
        tx.execute(
            "INSERT INTO app_traffic_hour (bucket_start, process_name, up, down)
             SELECT (bucket_start / 3600) * 3600, process_name, SUM(up), SUM(down)
             FROM app_traffic_minute WHERE bucket_start < ?1
             GROUP BY (bucket_start / 3600) * 3600, process_name
             ON CONFLICT(bucket_start, process_name) DO UPDATE SET
               up = up + excluded.up, down = down + excluded.down",
            params![older_than_secs],
        )?;
        tx.execute(
            "DELETE FROM app_traffic_minute WHERE bucket_start < ?1",
            params![older_than_secs],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn prune_hour(&self, older_than_secs: i64) -> Result<(), TrafficStatsError> {
        let conn = self.conn.lock().map_err(|e| TrafficStatsError::Msg(e.to_string()))?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM config_traffic_hour WHERE bucket_start < ?1",
            params![older_than_secs],
        )?;
        tx.execute(
            "DELETE FROM app_traffic_hour WHERE bucket_start < ?1",
            params![older_than_secs],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn query_config_usage(
        &self,
        from_secs: i64,
        to_secs: i64,
    ) -> Result<Vec<ConfigUsage>, TrafficStatsError> {
        let conn = self.conn.lock().map_err(|e| TrafficStatsError::Msg(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT profile_id, SUM(u), SUM(d) FROM (
               SELECT profile_id, up AS u, down AS d FROM config_traffic_minute
                 WHERE bucket_start >= ?1 AND bucket_start < ?2
               UNION ALL
               SELECT profile_id, up AS u, down AS d FROM config_traffic_hour
                 WHERE bucket_start >= ?1 AND bucket_start < ?2
             ) GROUP BY profile_id",
        )?;
        let rows = stmt
            .query_map(params![from_secs, to_secs], |r| {
                Ok(ConfigUsage {
                    profile_id: r.get(0)?,
                    up: r.get(1)?,
                    down: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn query_app_usage(
        &self,
        from_secs: i64,
        to_secs: i64,
    ) -> Result<Vec<AppUsage>, TrafficStatsError> {
        let conn = self.conn.lock().map_err(|e| TrafficStatsError::Msg(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT process_name, SUM(u), SUM(d) FROM (
               SELECT process_name, up AS u, down AS d FROM app_traffic_minute
                 WHERE bucket_start >= ?1 AND bucket_start < ?2
               UNION ALL
               SELECT process_name, up AS u, down AS d FROM app_traffic_hour
                 WHERE bucket_start >= ?1 AND bucket_start < ?2
             ) GROUP BY process_name",
        )?;
        let rows = stmt
            .query_map(params![from_secs, to_secs], |r| {
                Ok(AppUsage {
                    process_name: r.get(0)?,
                    up: r.get(1)?,
                    down: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Time series totalled across configs; `bucket_secs` e.g. 3600 or 86400.
    /// `utc_offset_secs` shifts grouping to local calendar boundaries.
    pub fn query_config_series(
        &self,
        from_secs: i64,
        to_secs: i64,
        bucket_secs: i64,
        utc_offset_secs: i64,
    ) -> Result<Vec<TrafficSeriesPoint>, TrafficStatsError> {
        self.query_series(
            "config_traffic_minute",
            "config_traffic_hour",
            from_secs,
            to_secs,
            bucket_secs,
            utc_offset_secs,
        )
    }

    pub fn query_app_series(
        &self,
        from_secs: i64,
        to_secs: i64,
        bucket_secs: i64,
        utc_offset_secs: i64,
    ) -> Result<Vec<TrafficSeriesPoint>, TrafficStatsError> {
        self.query_series(
            "app_traffic_minute",
            "app_traffic_hour",
            from_secs,
            to_secs,
            bucket_secs,
            utc_offset_secs,
        )
    }

    fn query_series(
        &self,
        minute_table: &str,
        hour_table: &str,
        from_secs: i64,
        to_secs: i64,
        bucket_secs: i64,
        utc_offset_secs: i64,
    ) -> Result<Vec<TrafficSeriesPoint>, TrafficStatsError> {
        if bucket_secs <= 0 {
            return Ok(Vec::new());
        }
        // Safe: only internal integers embedded (same as upstream).
        let bkt = format!(
            "((bucket_start + ({off})) / {b}) * {b} - ({off})",
            off = utc_offset_secs,
            b = bucket_secs
        );
        let sql = format!(
            "SELECT {bkt} AS bkt, SUM(u), SUM(d) FROM (
               SELECT bucket_start, up AS u, down AS d FROM {minute}
                 WHERE bucket_start >= ?1 AND bucket_start < ?2
               UNION ALL
               SELECT bucket_start, up AS u, down AS d FROM {hour}
                 WHERE bucket_start >= ?1 AND bucket_start < ?2
             ) GROUP BY bkt ORDER BY bkt",
            bkt = bkt,
            minute = minute_table,
            hour = hour_table,
        );
        let conn = self.conn.lock().map_err(|e| TrafficStatsError::Msg(e.to_string()))?;
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![from_secs, to_secs], |r| {
                Ok(TrafficSeriesPoint {
                    bucket_start: r.get(0)?,
                    up: r.get(1)?,
                    down: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn all_config_meta(&self) -> Result<Vec<ConfigMetaRow>, TrafficStatsError> {
        let conn = self.conn.lock().map_err(|e| TrafficStatsError::Msg(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT profile_id, name, group_name, type, server_address, first_seen, last_seen
             FROM config_meta",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ConfigMetaRow {
                    profile_id: r.get(0)?,
                    name: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    group_name: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    type_name: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    server_address: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    first_seen: r.get(5)?,
                    last_seen: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_config_meta(&self, profile_id: i64) -> Result<Option<ConfigMetaRow>, TrafficStatsError> {
        let conn = self.conn.lock().map_err(|e| TrafficStatsError::Msg(e.to_string()))?;
        let row = conn
            .query_row(
                "SELECT profile_id, name, group_name, type, server_address, first_seen, last_seen
                 FROM config_meta WHERE profile_id = ?1",
                params![profile_id],
                |r| {
                    Ok(ConfigMetaRow {
                        profile_id: r.get(0)?,
                        name: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        group_name: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        type_name: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        server_address: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                        first_seen: r.get(5)?,
                        last_seen: r.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }
}

/// In-memory minute accumulator + flush into [`TrafficStatsDb`] (upstream manager).
#[derive(Debug, Default)]
pub struct TrafficStatsManager {
    config_accum: std::collections::HashMap<i64, (i64, i64)>, // id -> (up, down)
    app_accum: std::collections::HashMap<String, (i64, i64)>,
    current_bucket: i64,
}

impl TrafficStatsManager {
    pub fn align_minute(secs: i64) -> i64 {
        (secs / 60) * 60
    }

    pub fn add_config_delta(
        &mut self,
        db: &TrafficStatsDb,
        profile_id: i64,
        up: i64,
        down: i64,
        now_secs: i64,
    ) -> Result<(), TrafficStatsError> {
        if up == 0 && down == 0 {
            return Ok(());
        }
        let now_bucket = Self::align_minute(now_secs);
        let mut flush_cfg = Vec::new();
        let mut flush_app = Vec::new();
        if self.current_bucket != 0 && now_bucket != self.current_bucket {
            self.drain(self.current_bucket, &mut flush_cfg, &mut flush_app);
        }
        self.current_bucket = now_bucket;
        let e = self.config_accum.entry(profile_id).or_default();
        e.0 = e.0.saturating_add(up.max(0));
        e.1 = e.1.saturating_add(down.max(0));
        if !flush_cfg.is_empty() {
            db.upsert_config_minute_batch(&flush_cfg)?;
        }
        if !flush_app.is_empty() {
            db.upsert_app_minute_batch(&flush_app)?;
        }
        Ok(())
    }

    pub fn add_app_delta(
        &mut self,
        db: &TrafficStatsDb,
        process_name: &str,
        up: i64,
        down: i64,
        now_secs: i64,
    ) -> Result<(), TrafficStatsError> {
        if process_name.is_empty() || (up == 0 && down == 0) {
            return Ok(());
        }
        let now_bucket = Self::align_minute(now_secs);
        let mut flush_cfg = Vec::new();
        let mut flush_app = Vec::new();
        if self.current_bucket != 0 && now_bucket != self.current_bucket {
            self.drain(self.current_bucket, &mut flush_cfg, &mut flush_app);
        }
        self.current_bucket = now_bucket;
        let e = self.app_accum.entry(process_name.to_string()).or_default();
        e.0 = e.0.saturating_add(up.max(0));
        e.1 = e.1.saturating_add(down.max(0));
        if !flush_cfg.is_empty() {
            db.upsert_config_minute_batch(&flush_cfg)?;
        }
        if !flush_app.is_empty() {
            db.upsert_app_minute_batch(&flush_app)?;
        }
        Ok(())
    }

    pub fn flush(&mut self, db: &TrafficStatsDb) -> Result<(), TrafficStatsError> {
        if self.current_bucket == 0 {
            return Ok(());
        }
        let mut flush_cfg = Vec::new();
        let mut flush_app = Vec::new();
        let bucket = self.current_bucket;
        self.drain(bucket, &mut flush_cfg, &mut flush_app);
        if !flush_cfg.is_empty() {
            db.upsert_config_minute_batch(&flush_cfg)?;
        }
        if !flush_app.is_empty() {
            db.upsert_app_minute_batch(&flush_app)?;
        }
        Ok(())
    }

    fn drain(
        &mut self,
        bucket: i64,
        cfg: &mut Vec<ConfigTrafficRow>,
        app: &mut Vec<AppTrafficRow>,
    ) {
        for (id, (up, down)) in self.config_accum.drain() {
            if up == 0 && down == 0 {
                continue;
            }
            cfg.push(ConfigTrafficRow {
                bucket_start: bucket,
                profile_id: id,
                up,
                down,
            });
        }
        for (name, (up, down)) in self.app_accum.drain() {
            if up == 0 && down == 0 {
                continue;
            }
            app.push(AppTrafficRow {
                bucket_start: bucket,
                process_name: name,
                up,
                down,
            });
        }
    }

    /// One maintenance pass: rollup 48h minute window, prune hour by retention days.
    pub fn run_rollup(
        db: &TrafficStatsDb,
        now_secs: i64,
        retention_days: i32,
    ) -> Result<(), TrafficStatsError> {
        const MINUTE_WINDOW: i64 = 48 * 3600;
        db.rollup_minute_to_hour(now_secs - MINUTE_WINDOW)?;
        let days = retention_days.max(1) as i64;
        db.prune_hour(now_secs - days * 86400)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn minute_upsert_and_usage_query() {
        let dir = tempdir().unwrap();
        let db = TrafficStatsDb::open(dir.path().join("throne_stats.db")).unwrap();
        db.upsert_config_minute_batch(&[
            ConfigTrafficRow {
                bucket_start: 1_000,
                profile_id: 7,
                up: 100,
                down: 200,
            },
            ConfigTrafficRow {
                bucket_start: 1_000,
                profile_id: 7,
                up: 50,
                down: 25,
            },
        ])
        .unwrap();
        let usage = db.query_config_usage(0, 10_000).unwrap();
        assert_eq!(usage.len(), 1);
        assert_eq!(usage[0].profile_id, 7);
        assert_eq!(usage[0].up, 150);
        assert_eq!(usage[0].down, 225);
    }

    #[test]
    fn manager_flushes_on_minute_rollover() {
        let dir = tempdir().unwrap();
        let db = TrafficStatsDb::open(dir.path().join("throne_stats.db")).unwrap();
        let mut mgr = TrafficStatsManager::default();
        // First minute bucket
        mgr.add_config_delta(&db, 1, 10, 20, 60).unwrap();
        // Next minute → flushes previous
        mgr.add_config_delta(&db, 1, 5, 5, 120).unwrap();
        mgr.flush(&db).unwrap();
        let usage = db.query_config_usage(0, 10_000).unwrap();
        assert_eq!(usage.len(), 1);
        assert_eq!(usage[0].up, 15);
        assert_eq!(usage[0].down, 25);
    }

    #[test]
    fn series_groups_into_hour_buckets() {
        let dir = tempdir().unwrap();
        let db = TrafficStatsDb::open(dir.path().join("throne_stats.db")).unwrap();
        db.upsert_config_minute_batch(&[
            ConfigTrafficRow {
                bucket_start: 3600,
                profile_id: 1,
                up: 1,
                down: 2,
            },
            ConfigTrafficRow {
                bucket_start: 3660,
                profile_id: 1,
                up: 3,
                down: 4,
            },
        ])
        .unwrap();
        let series = db.query_config_series(0, 10_000, 3600, 0).unwrap();
        assert!(!series.is_empty());
        let pt = series.iter().find(|p| p.bucket_start == 3600).unwrap();
        assert_eq!(pt.up, 4);
        assert_eq!(pt.down, 6);
    }
}
