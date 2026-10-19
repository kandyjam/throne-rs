use serde_json::{json, Value};
use throne_core_client::build_load_config;
use throne_domain::{AppSettings, Profile, ProfileType, RouteProfile, RouteRule};

#[test]
fn route_options_fragment_action_and_delay_reach_core() {
    let mut profile = Profile::new(1, 1, "socks", ProfileType::Socks);
    profile.outbound.server = Some("127.0.0.1".into());
    profile.outbound.server_port = Some(1080);
    let mut route = RouteProfile::new(1, "fragment");
    let mut rule = serde_json::to_value(RouteRule::default()).unwrap();
    rule["action"] = json!("route-options");
    rule["domain"] = json!(["example.com"]);
    rule["tls_fragment"] = json!(true);
    rule["tls_fragment_fallback_delay"] = json!("500ms");
    route.rules.push(serde_json::from_value(rule).unwrap());
    let built = build_load_config(&profile, &AppSettings::default(), Some(&route)).unwrap();
    let config: Value = serde_json::from_str(&built.core_config_json).unwrap();
    let rule = config["route"]["rules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["domain"] == json!(["example.com"]))
        .unwrap();
    assert_eq!(rule["action"], "route-options");
    assert_eq!(rule["tls_fragment"], true);
    assert_eq!(rule["tls_fragment_fallback_delay"], "500ms");
    assert!(rule.get("outbound").is_none());
}
