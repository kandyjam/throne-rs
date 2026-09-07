//! Exact CREATE TABLE statements matching upstream Throne repos.

use rusqlite::Connection;

use crate::StorageError;

pub fn ensure_schema(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS entity_ids (
            profile_last_id INTEGER NOT NULL DEFAULT 0,
            group_last_id INTEGER NOT NULL DEFAULT 0,
            route_profile_last_id INTEGER NOT NULL DEFAULT 0
        );

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

        CREATE TABLE IF NOT EXISTS route_profiles (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL DEFAULT '',
            default_outbound_id INTEGER NOT NULL DEFAULT -1,
            is_raw INTEGER NOT NULL DEFAULT 0,
            raw_route TEXT NOT NULL DEFAULT '',
            prevent_modifications INTEGER NOT NULL DEFAULT 0,
            is_remote INTEGER NOT NULL DEFAULT 0,
            remote_url TEXT NOT NULL DEFAULT '',
            auto_update INTEGER NOT NULL DEFAULT 0,
            remote_last_update INTEGER NOT NULL DEFAULT 0,
            endpoint_profile_ids TEXT NOT NULL DEFAULT '[]',
            created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
            updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
        );

        CREATE TABLE IF NOT EXISTS route_rules (
            route_profile_id INTEGER NOT NULL,
            rule_order INTEGER NOT NULL,
            name TEXT NOT NULL DEFAULT '',
            type INTEGER NOT NULL DEFAULT 0,
            ip_version TEXT,
            network TEXT,
            protocol TEXT,
            inbound_json TEXT,
            domain_json TEXT,
            domain_suffix_json TEXT,
            domain_keyword_json TEXT,
            domain_regex_json TEXT,
            source_ip_cidr_json TEXT,
            source_ip_is_private INTEGER NOT NULL DEFAULT 0,
            ip_cidr_json TEXT,
            ip_is_private INTEGER NOT NULL DEFAULT 0,
            source_port_json TEXT,
            source_port_range_json TEXT,
            port_json TEXT,
            port_range_json TEXT,
            process_name_json TEXT,
            process_path_json TEXT,
            process_path_regex_json TEXT,
            rule_set_json TEXT,
            invert INTEGER NOT NULL DEFAULT 0,
            outbound_id INTEGER NOT NULL DEFAULT -2,
            action TEXT NOT NULL DEFAULT 'route',
            reject_method TEXT,
            no_drop INTEGER NOT NULL DEFAULT 0,
            override_address TEXT,
            override_port TEXT,
            sniffers_json TEXT,
            sniff_override_dest INTEGER NOT NULL DEFAULT 0,
            strategy TEXT,
            wifi_ssid_json TEXT,
            wifi_bssid_json TEXT,
            PRIMARY KEY (route_profile_id, rule_order),
            FOREIGN KEY(route_profile_id) REFERENCES route_profiles(id) ON DELETE CASCADE
        );
        "#,
    )?;

    // Migrations for older Throne DBs (same ALTERs as upstream).
    add_column_if_missing(conn, "groups", "type_sort_by", "INTEGER NOT NULL DEFAULT 0")?;
    add_column_if_missing(
        conn,
        "groups",
        "auto_clear_unavailable",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "route_profiles",
        "is_raw",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "route_profiles",
        "raw_route",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    add_column_if_missing(
        conn,
        "route_profiles",
        "prevent_modifications",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "route_profiles",
        "is_remote",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "route_profiles",
        "remote_url",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    add_column_if_missing(
        conn,
        "route_profiles",
        "auto_update",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "route_profiles",
        "remote_last_update",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        conn,
        "route_profiles",
        "endpoint_profile_ids",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    add_column_if_missing(conn, "route_rules", "wifi_ssid_json", "TEXT")?;
    add_column_if_missing(conn, "route_rules", "wifi_bssid_json", "TEXT")?;

    // Drop our earlier mistaken column if present (safe no-op when absent).
    // SQLite < 3.35 may lack DROP COLUMN — ignore errors.
    let _ = conn.execute("ALTER TABLE route_profiles DROP COLUMN rules_json", []);

    // entity_ids single row
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM entity_ids", [], |r| r.get(0))?;
    if n == 0 {
        conn.execute(
            "INSERT INTO entity_ids (profile_last_id, group_last_id, route_profile_last_id) VALUES (0,0,0)",
            [],
        )?;
    } else {
        // Older DBs may miss route_profile_last_id
        add_column_if_missing(
            conn,
            "entity_ids",
            "route_profile_last_id",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
    }

    Ok(())
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    decl: &str,
) -> Result<(), StorageError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = stmt
        .query_map([], |row| {
            let name: String = row.get(1)?;
            Ok(name)
        })?
        .filter_map(|r| r.ok())
        .any(|n| n == column);
    if !exists {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"),
            [],
        )?;
    }
    Ok(())
}
