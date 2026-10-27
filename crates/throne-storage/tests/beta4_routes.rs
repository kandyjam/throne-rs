use throne_domain::{AppState, RouteDns, RouteProfile, RouteRule};
use throne_storage::Database;

#[test]
fn route_dns_and_disabled_rules_survive_compatible_database_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("throne.db");
    let mut state = AppState::empty();
    let mut route = RouteProfile::new(0, "DNS");
    route.custom_dns = true;
    route.dns = Some(RouteDns {
        remote_dns: "https://own.example/dns-query".into(),
        remote_dns_disable_ipv6: true,
        fake_dns: true,
        ..Default::default()
    });
    route.rules.push(RouteRule {
        name: "off".into(),
        disabled: true,
        domain: vec!["example.com".into()],
        ..Default::default()
    });
    let id = state.add_route(route);
    Database::open(&path).unwrap().save_state(&state).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let (own,dns,disabled):(i32,String,i32)=db.query_row("SELECT custom_dns,dns_json,disabled FROM route_profiles JOIN route_rules ON route_profiles.id=route_rules.route_profile_id",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!((own, disabled), (1, 1));
    assert!(dns.contains("own.example"));
    drop(db);
    let reopened = Database::open(&path).unwrap().load_state().unwrap();
    let route = reopened
        .all_routes()
        .into_iter()
        .find(|r| r.id == id)
        .unwrap();
    assert!(route.rules[0].disabled);
    assert!(route.active_dns().unwrap().fake_dns);
    assert!(route.active_dns().unwrap().remote_dns_disable_ipv6);
}

#[test]
fn legacy_database_migrates_dns_and_rule_columns_without_losing_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    Database::open(&path).unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch("ALTER TABLE route_profiles DROP COLUMN custom_dns; ALTER TABLE route_profiles DROP COLUMN dns_json; ALTER TABLE route_rules DROP COLUMN disabled; INSERT INTO route_profiles(id,name) VALUES(12,'legacy'); INSERT INTO route_rules(route_profile_id,rule_order,name) VALUES(12,0,'rule');").unwrap();
    drop(raw);
    let state = Database::open(&path).unwrap().load_state().unwrap();
    let route = state.all_routes().into_iter().find(|r| r.id == 12).unwrap();
    assert!(!route.custom_dns);
    assert!(route.dns.is_none());
    assert!(!route.rules[0].disabled);
}

#[test]
fn core_presets_keep_upstream_setting_names_and_values_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.db");
    let mut state = AppState::empty();
    let s = state.settings_mut();
    s.disable_mixed_inbound = true;
    s.xray_use_system_ca = true;
    s.mux_default_on = true;
    s.mux_protocol = "yamux".into();
    s.mux_mode = "max_connections".into();
    s.mux_concurrency = 23;
    s.mux_max_connections = 7;
    s.mux_min_streams = 3;
    s.mux_padding = true;
    s.mux_brutal = true;
    s.mux_brutal_up_mbps = 50;
    s.mux_brutal_down_mbps = 100;
    Database::open(&path).unwrap().save_state(&state).unwrap();
    let reopened = Database::open(&path).unwrap().load_state().unwrap();
    let a = serde_json::to_value(state.settings()).unwrap();
    let b = serde_json::to_value(reopened.settings()).unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    for key in [
        "disable_mixed_inbound",
        "xray_use_system_ca",
        "mux_default_on",
        "mux_protocol",
        "mux_mode",
        "mux_concurrency",
        "mux_max_connections",
        "mux_min_streams",
        "mux_padding",
        "mux_brutal",
        "mux_brutal_up_mbps",
        "mux_brutal_down_mbps",
    ] {
        assert_eq!(a[key], b[key], "{key}");
        let count: i64 = raw
            .query_row("SELECT COUNT(*) FROM settings WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1, "wire key {key}");
    }
}
