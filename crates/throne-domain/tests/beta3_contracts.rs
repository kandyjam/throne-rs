use serde_json::json;
use throne_domain::{AppState, AutoSelectorConfig, ProfileType};

#[test]
fn selector_groups_are_ordered_unique_and_keep_legacy_gid() {
    let old = AutoSelectorConfig::from_outbound_json(r#"{"type":"autoselector","gid":3}"#).unwrap();
    let old: serde_json::Value = serde_json::from_str(&old.to_outbound_json()).unwrap();
    assert_eq!(old["gids"], json!([3]));
    let new = AutoSelectorConfig::from_outbound_json(
        r#"{"type":"autoselector","gid":99,"gids":[3,2,3,-1]}"#,
    )
    .unwrap();
    let new: serde_json::Value = serde_json::from_str(&new.to_outbound_json()).unwrap();
    assert_eq!(new["gids"], json!([3, 2]));
    assert_eq!(new["gid"], 3);
    let empty =
        AutoSelectorConfig::from_outbound_json(r#"{"type":"autoselector","gid":99,"gids":[]}"#)
            .unwrap();
    let empty: serde_json::Value = serde_json::from_str(&empty.to_outbound_json()).unwrap();
    assert_eq!(
        empty["gid"], -1,
        "explicit empty gids must not resurrect legacy gid"
    );
}

#[test]
fn start_and_rerank_resolve_members_across_existing_groups() {
    let mut state = AppState::empty();
    let first = state.add_group("first");
    let second = state.add_group("second");
    let slow = state.add_profile(first, "slow", ProfileType::Socks);
    let fast = state.add_profile(second, "fast", ProfileType::Socks);
    state.set_profile_latency(slow, 200);
    state.set_profile_latency(fast, 20);
    let selector = state.add_auto_selector(first, first, "all").unwrap();
    state
        .update_profile_outbound_json(
            selector,
            json!({"type":"autoselector","gid":first,"gids":[first,999999,second,first]})
                .to_string(),
        )
        .unwrap();
    let (_, plan, members) = state.resolve_auto_selector_members(selector).unwrap();
    assert_eq!(plan.build, vec![fast, slow]);
    assert_eq!(members.len(), 2);
    assert_eq!(
        state.rerank_auto_selector(selector).unwrap(),
        vec![fast, slow]
    );
}

#[test]
fn basic_route_roundtrip_preserves_fragment_options_on_simple_rules() {
    use throne_domain::{RouteProfile, SimpleAction};
    let mut route = RouteProfile::new(1, "fragment");
    assert!(route
        .update_simple_rules("domain:example.com", SimpleAction::Proxy)
        .is_empty());
    let rule = route.rules.iter_mut().find(|r| r.rule_type == 1).unwrap();
    rule.tls_fragment = true;
    rule.tls_record_fragment = true;
    rule.tls_fragment_fallback_delay = "500ms".into();
    let text = route.simple_rules_text(SimpleAction::Proxy);
    assert!(route
        .update_simple_rules(&text, SimpleAction::Proxy)
        .is_empty());
    let rule = route.rules.iter().find(|r| r.rule_type == 1).unwrap();
    assert!(rule.tls_fragment && rule.tls_record_fragment);
    assert_eq!(rule.tls_fragment_fallback_delay, "500ms");
    assert!(route
        .update_simple_rules("", SimpleAction::Proxy)
        .is_empty());
    assert!(
        !route.rules.iter().any(|r| r.rule_type == 1),
        "clearing the simple bucket still removes it"
    );
}

#[test]
fn unconditional_route_options_survive_basic_route_save() {
    use throne_domain::{RouteProfile, RouteRule, SimpleAction};
    let mut route = RouteProfile::new(1, "fragment");
    route.rules.push(RouteRule {
        action: "route-options".into(),
        tls_fragment: true,
        ..Default::default()
    });
    route.update_simple_rules("", SimpleAction::Proxy);
    assert_eq!(route.rules.len(), 1);
    assert_eq!(route.rules[0].action, "route-options");
}

#[test]
fn snapshot_sort_normalizes_order_and_keeps_equal_names_stable() {
    use throne_domain::{AppSettings, Group, Profile};
    let mut group = Group::new(1, "sorted");
    group.sort_method = 3;
    group.profile_ids = vec![3, 2, 1];
    let mut state = AppState::empty();
    state.load_snapshot(
        vec![group],
        vec![
            Profile::new(1, 1, "a", ProfileType::Socks),
            Profile::new(2, 1, "a", ProfileType::Socks),
            Profile::new(3, 1, "z", ProfileType::Socks),
        ],
        vec![1],
        AppSettings::default(),
        vec![],
    );
    assert_eq!(state.group(1).unwrap().profile_ids, vec![2, 1, 3]);
    state.rename_profile(3, "A").unwrap();
    assert_eq!(
        state.group(1).unwrap().profile_ids,
        vec![3, 2, 1],
        "upstream name order is case sensitive and ties stay stable"
    );
}

#[test]
fn raw_group_order_is_unchanged_by_test_updates_and_renames() {
    let mut state = AppState::empty();
    let group = state.add_group("raw");
    let first = state.add_profile(group, "z", ProfileType::Socks);
    let second = state.add_profile(group, "a", ProfileType::Socks);
    state.set_profile_latency(first, 200);
    state.set_profile_latency(second, 10);
    state.rename_profile(second, "A").unwrap();
    assert_eq!(state.group(group).unwrap().profile_ids, vec![first, second]);
    assert_eq!(state.group(group).unwrap().sort_method, 0);
}

#[test]
fn traffic_sort_updates_only_its_group_and_respects_saved_direction() {
    use throne_domain::ProfileSortColumn;
    let mut state = AppState::empty();
    let sorted = state.add_group("traffic");
    let raw = state.add_group("raw");
    let first = state.add_profile(sorted, "a", ProfileType::Socks);
    let second = state.add_profile(sorted, "b", ProfileType::Socks);
    let other = state.add_profile(raw, "c", ProfileType::Socks);
    state.set_active_group(sorted).unwrap();
    state
        .sort_active_group_profiles(ProfileSortColumn::Traffic, false)
        .unwrap();
    state.set_profile_traffic(second, 50, 1);
    assert_eq!(
        state.group(sorted).unwrap().profile_ids,
        vec![second, first]
    );
    state.set_profile_traffic(first, 100, 0);
    assert_eq!(
        state.group(sorted).unwrap().profile_ids,
        vec![first, second]
    );
    assert_eq!(state.group(raw).unwrap().profile_ids, vec![other]);
    assert_eq!(state.group(raw).unwrap().sort_method, 0);
}
