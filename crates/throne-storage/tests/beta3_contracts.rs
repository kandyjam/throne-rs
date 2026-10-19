use serde_json::{json, Value};
use throne_domain::{
    AppState, DefaultOutbound, ProfileSortColumn, ProfileType, RouteProfile, RouteRule,
};
use throne_storage::Database;

#[test]
fn new_route_options_and_multigroup_selectors_survive_database_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("throne.db");
    let mut state = AppState::empty();
    let a = state.add_group("a");
    let b = state.add_group("b");
    let selector = state.add_auto_selector(a, a, "both").unwrap();
    state
        .update_profile_outbound_json(
            selector,
            json!({"type":"autoselector","gids":[a,b],"gid":a}).to_string(),
        )
        .unwrap();
    let mut route = RouteProfile::new(0, "TLS fragment");
    route.rules.push(RouteRule {
        tls_fragment: true,
        tls_record_fragment: true,
        tls_fragment_fallback_delay: "500ms".into(),
        action: "route".into(),
        outbound_id: DefaultOutbound::Proxy.as_id(),
        ..Default::default()
    });
    state.add_route(route);
    Database::open(&path).unwrap().save_state(&state).unwrap();
    let reopened = Database::open(&path).unwrap().load_state().unwrap();
    let rule = &reopened.all_routes()[0].rules[0];
    assert!(rule.tls_fragment && rule.tls_record_fragment);
    assert_eq!(rule.tls_fragment_fallback_delay, "500ms");
    let out: Value =
        serde_json::from_str(&reopened.profile(selector).unwrap().outbound_json).unwrap();
    assert_eq!(out["gids"], json!([a, b]));
}

#[test]
fn selected_group_sort_is_retained_after_reopen_and_new_results() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("throne.db");
    let mut state = AppState::empty();
    let gid = state.add_group("sorted");
    let a = state.add_profile(gid, "a", ProfileType::Socks);
    let b = state.add_profile(gid, "b", ProfileType::Socks);
    state.set_profile_latency(a, 10);
    state.set_profile_latency(b, 20);
    state
        .sort_active_group_profiles(ProfileSortColumn::TestResult, true)
        .unwrap();
    Database::open(&path).unwrap().save_state(&state).unwrap();
    let mut reopened = Database::open(&path).unwrap().load_state().unwrap();
    reopened.set_profile_latency(a, 30);
    assert_eq!(reopened.group(gid).unwrap().profile_ids, vec![b, a]);
    let value = serde_json::to_value(reopened.group(gid).unwrap()).unwrap();
    assert_eq!(value["sort_method"], 4);
    assert_eq!(value["sort_descending"], false);
}
