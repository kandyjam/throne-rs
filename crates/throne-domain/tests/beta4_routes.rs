use throne_domain::{
    AppSettings, AppState, DefaultOutbound, RouteDns, RouteProfile, RouteRule, SimpleAction,
};

#[test]
fn route_dns_overrides_only_owned_fields_and_restores_global_choice() {
    let mut global = AppSettings {
        remote_dns: "https://global.example/dns-query".into(),
        direct_dns: "192.0.2.53".into(),
        remote_dns_strategy: "ipv4_only".into(),
        use_dns_object: true,
        dns_object: "{\"global\":true}".into(),
        dns_cache_capacity: 1234,
        ..Default::default()
    };
    let mut profile = RouteProfile::new(1, "private DNS");
    profile.dns = Some(RouteDns::from_settings(&global));
    assert!(profile.dns.as_ref().unwrap().remote_dns_disable_ipv6);
    assert!(profile.active_dns().is_none());
    profile.custom_dns = true;
    profile.dns.as_mut().unwrap().remote_dns = "https://own.example/dns-query".into();
    profile.dns.as_mut().unwrap().remote_dns_disable_ipv6 = false;
    profile.active_dns().unwrap().apply_to_settings(&mut global);
    assert_eq!(global.remote_dns, "https://own.example/dns-query");
    assert!(!global.remote_dns_disable_ipv6);
    assert!(global.remote_dns_strategy.is_empty());
    assert!(!global.use_dns_object);
    assert_eq!(global.dns_cache_capacity, 1234);
    assert_eq!(global.dns_object, "{\"global\":true}");
    profile.custom_dns = false;
    assert!(profile.active_dns().is_none());
    assert_eq!(
        profile.dns.unwrap().remote_dns,
        "https://own.example/dns-query"
    );
}

#[test]
fn disabled_rule_survives_simple_editor_and_does_not_shadow_other_actions() {
    let mut profile = RouteProfile::new(1, "rules");
    profile
        .append_simple_rule("suffix:example.com", SimpleAction::Bypass)
        .unwrap();
    profile.rules[0].disabled = true;
    profile.rules[0].tls_fragment = true;
    assert!(profile
        .covering_simple_rule("suffix:api.example.com", SimpleAction::Proxy)
        .is_none());
    let text = profile.simple_rules_text(SimpleAction::Bypass);
    assert!(profile
        .update_simple_rules(&text, SimpleAction::Bypass)
        .is_empty());
    assert!(profile.rules[0].disabled && profile.rules[0].tls_fragment);
    assert!(profile
        .simple_rules_text(SimpleAction::Bypass)
        .contains("example.com"));
}

#[test]
fn remote_refresh_preserves_local_dns_endpoint_and_disabled_choices() {
    let mut local = RouteProfile::new(1, "local name");
    local.custom_dns = true;
    local.dns = Some(RouteDns {
        remote_dns: "https://local.example/dns-query".into(),
        ..Default::default()
    });
    local.endpoint_profile_ids = vec![4];
    local.inner_hop_endpoint_ids = vec![4];
    local.rules = vec![
        RouteRule {
            name: "stay-off".into(),
            disabled: true,
            domain: vec!["old.example".into()],
            ..Default::default()
        },
        RouteRule {
            name: "endpoint".into(),
            rule_type: 13,
            outbound_id: 4,
            ..Default::default()
        },
    ];
    let mut incoming = RouteProfile::new(2, "remote name");
    incoming.default_outbound = DefaultOutbound::Direct;
    incoming.custom_dns = true;
    incoming.dns = Some(RouteDns::default());
    incoming.rules = vec![
        RouteRule {
            name: "stay-off".into(),
            domain: vec!["new.example".into()],
            ..Default::default()
        },
        RouteRule {
            name: "new".into(),
            domain: vec!["new2.example".into()],
            ..Default::default()
        },
        RouteRule {
            name: "remote endpoint".into(),
            rule_type: 13,
            outbound_id: 999,
            ..Default::default()
        },
    ];
    local.apply_remote_content(incoming).unwrap();
    assert_eq!(local.name, "local name");
    assert_eq!(local.default_outbound, DefaultOutbound::Direct);
    assert!(local.rules[0].disabled);
    assert_eq!(local.rules[0].domain, ["new.example"]);
    assert_eq!(local.rules[1].outbound_id, 4);
    assert!(!local.rules[2].disabled);
    assert_eq!(local.endpoint_profile_ids, [4]);
    assert_eq!(
        local.active_dns().unwrap().remote_dns,
        "https://local.example/dns-query"
    );
    let mut raw = RouteProfile::new(3, "raw");
    raw.is_raw = true;
    assert!(local.apply_remote_content(raw).is_err());
    assert_eq!(local.rules.len(), 3);
}

#[test]
fn state_remote_refresh_uses_the_same_preservation_contract() {
    let mut state = AppState::empty();
    let mut local = RouteProfile::new(0, "local");
    local.is_remote = true;
    local.remote_url = "https://example.com/rules".into();
    local.custom_dns = true;
    local.dns = Some(RouteDns::default());
    local.rules.push(RouteRule {
        name: "off".into(),
        disabled: true,
        ..Default::default()
    });
    let id = state.add_route(local);
    let mut incoming = RouteProfile::new(0, "download");
    incoming.rules.push(RouteRule {
        name: "off".into(),
        ..Default::default()
    });
    state.replace_route_content(id, incoming).unwrap();
    let route = state.all_routes().into_iter().find(|r| r.id == id).unwrap();
    assert!(route.rules[0].disabled);
    assert!(route.active_dns().is_some());
    assert_eq!(route.remote_url, "https://example.com/rules");
}

#[test]
fn route_dns_rejects_invalid_resolvers_without_rejecting_supported_transports() {
    for address in [
        "localhost",
        "192.0.2.53:5353",
        "tls://dns.example",
        "https://dns.example/dns-query",
        "h3://[2001:db8::1]:443/dns-query",
        "dhcp://auto",
        "udp://[fe80::1%en0]:53",
    ] {
        assert!(
            RouteDns {
                remote_dns: address.into(),
                ..Default::default()
            }
            .validate()
            .is_ok(),
            "{address}"
        );
    }
    for address in [
        "",
        "udp://",
        "https://:443/dns-query",
        "8.8.8.8:65536",
        "udp://example.com/path",
        "https://[2001:db8::1",
        "https://example.com:0/dns-query",
        "ftp://example.com",
        "999.1.1.1",
        "dns example",
    ] {
        assert!(
            RouteDns {
                remote_dns: address.into(),
                ..Default::default()
            }
            .validate()
            .is_err(),
            "{address}"
        );
    }
    assert!(RouteDns {
        dns_final_out: "missing".into(),
        ..Default::default()
    }
    .validate()
    .is_err());
}

#[test]
fn routing_settings_save_commits_dns_flags_in_both_directions() {
    let mut state = AppState::empty();
    state.settings_mut().log_level = "debug".into();
    for enabled in [true, false] {
        let mut draft = state.settings().clone();
        draft.remote_dns_disable_ipv6 = enabled;
        draft.direct_dns_disable_ipv6 = enabled;
        draft.fakeip_disable_ipv6 = enabled;
        draft.dns_persist_cache = enabled;
        draft.log_level = "error".into();
        state.apply_routing_dialog_settings(draft);
        let saved = state.settings();
        assert_eq!(saved.remote_dns_disable_ipv6, enabled);
        assert_eq!(saved.direct_dns_disable_ipv6, enabled);
        assert_eq!(saved.fakeip_disable_ipv6, enabled);
        assert_eq!(saved.dns_persist_cache, enabled);
        assert_eq!(saved.log_level, "debug");
    }
}
