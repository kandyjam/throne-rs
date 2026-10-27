use throne_domain::{RouteDns, RouteProfile, RouteRule};
use throne_import::{import_route_payload, to_share_object};

#[test]
fn structured_and_raw_shares_round_trip_active_dns_and_rule_disabled() {
    for raw in [false, true] {
        let mut route = RouteProfile::new(1, "DNS");
        route.is_raw = raw;
        route.raw_route = "{\"rules\":[]}".into();
        route.custom_dns = true;
        route.dns = Some(RouteDns {
            remote_dns: "https://own.example/dns-query".into(),
            fake_dns: true,
            ..Default::default()
        });
        route.rules.push(RouteRule {
            name: "off".into(),
            disabled: true,
            domain: vec!["example.com".into()],
            ..Default::default()
        });
        let exported = to_share_object(&route);
        assert!(exported["dns"].is_object());
        let imported = import_route_payload(&exported.to_string());
        assert!(imported.errors.is_empty(), "{:?}", imported.errors);
        let restored = &imported.routes[0];
        assert_eq!(restored.active_dns(), route.active_dns());
        if !raw {
            assert!(restored.rules[0].disabled);
        }
        route.custom_dns = false;
        assert!(to_share_object(&route).get("dns").is_none());
    }
}

#[test]
fn invalid_route_dns_is_reported_and_falls_back_to_global() {
    let imported = import_route_payload(
        r#"{"kind":"throne-route-profile","name":"bad DNS","dns":{"remote_dns":"https://:99999/dns-query"},"rules":[]}"#,
    );
    assert!(imported.errors.is_empty());
    assert_eq!(imported.warnings.len(), 1);
    assert!(imported.routes[0].active_dns().is_none());
}
