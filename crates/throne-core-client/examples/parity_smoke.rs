//! Run as `target/parity-smoke/Throne` beside a fresh, non-setuid ThroneCore.
//! Only validates configuration, counts targets, parses a local rule set, and
//! probes a loopback TCP listener; never starts a profile or changes network settings.

use std::error::Error;
use std::net::TcpListener;

use throne_core_client::{
    build_load_config, CoreConfig, CoreError, CoreSession, ScanEntry, ScanProbeRequest,
    ScanTargetSpec, ScanTcpOptions,
};
use throne_domain::{
    materialize_profile_endpoint, AppSettings, EndpointSource, Profile, ProfileType,
};

fn check_websocket_endpoints(
    core: &mut CoreSession,
    settings: &AppSettings,
) -> Result<(), Box<dyn Error>> {
    for (transport, expected_host) in [
        (
            serde_json::json!({"type":"ws","path":"/socket"}),
            "origin.example",
        ),
        (
            serde_json::json!({"type":"ws","path":"/socket","headers":{"host":"cdn.example"}}),
            "cdn.example",
        ),
        (
            serde_json::json!({"type":"ws","path":"/socket","host":"legacy.example"}),
            "legacy.example",
        ),
    ] {
        let mut profile = Profile::new(2, 1, "WebSocket endpoint smoke", ProfileType::Vless);
        profile.endpoint = EndpointSource::Address {
            address: "192.0.2.9".into(),
        };
        profile.outbound.transport = Some("ws".into());
        profile.outbound_json = serde_json::json!({
            "type": "vless",
            "server": "origin.example",
            "server_port": 443,
            "uuid": "01234567-89ab-cdef-0123-456789abcdef",
            "tls": {"enabled": true},
            "transport": transport,
        })
        .to_string();
        let saved = profile.outbound_json.clone();
        let materialized = materialize_profile_endpoint(&profile, None, None)?;
        let built = build_load_config(&materialized, settings, None)?;
        let mut config: serde_json::Value = serde_json::from_str(&built.core_config_json)?;
        let proxy = config["outbounds"]
            .as_array_mut()
            .ok_or("generated config has no outbounds")?
            .iter_mut()
            .find(|outbound| outbound["tag"] == "proxy")
            .ok_or("generated config has no proxy outbound")?;
        assert_eq!(proxy["server"], "192.0.2.9");
        assert_eq!(proxy["server_port"], 443);
        assert_eq!(proxy["tls"]["server_name"], "origin.example");
        assert!(proxy["transport"].get("host").is_none());
        let headers = proxy["transport"]["headers"]
            .as_object()
            .ok_or("WebSocket transport has no headers")?;
        let host = headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("host"))
            .ok_or("WebSocket transport lost its Host header")?;
        assert_eq!(host.1, expected_host);
        assert_eq!(
            profile.outbound_json, saved,
            "materialization mutated saved profile"
        );
        core.check_config_json(&built.core_config_json)?;

        // Negative control: this exact malformed field caused the regression.
        // The real core must reject it, proving CheckConfig exercises the schema.
        proxy["transport"]["host"] = expected_host.into();
        assert!(matches!(
            core.check_config_json(&config.to_string()),
            Err(CoreError::Config(_))
        ));
    }
    println!(
        "PASS: materialized WebSocket endpoints preserve SNI/Host and validate; transport.host is rejected"
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let executable = std::env::current_exe()?;
    let directory = executable.parent().ok_or("no executable directory")?;
    let core_path = directory.join("ThroneCore");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if core_path.metadata()?.permissions().mode() & 0o6000 != 0 {
            return Err("smoke test refuses a setuid/setgid core binary".into());
        }
    }
    let config = CoreConfig {
        binary_path: core_path,
        work_dir: directory.into(),
        extra_args: Vec::new(),
        asset_dir: directory.into(),
    };
    let mut core = CoreSession::new(config);
    let mut profile = Profile::new(1, 1, "loopback smoke", ProfileType::Socks);
    profile.outbound.server = Some("127.0.0.1".into());
    profile.outbound.server_port = Some(1080);
    let settings = AppSettings::default();
    assert!(!settings.tun_mode_enabled && !settings.system_proxy_enabled);
    let built = build_load_config(&profile, &settings, None)?;
    let json: serde_json::Value = serde_json::from_str(&built.core_config_json)?;
    assert_eq!(json["services"][0]["type"], "api");
    assert_eq!(json["services"][0]["listen"], "127.0.0.1");
    core.check_config_json(&built.core_config_json)?;
    println!("PASS: generated SOCKS config with native API service validates");
    check_websocket_endpoints(&mut core, &settings)?;
    check_beta2_imports(&mut core)?;
    check_beta3_contracts(&mut core)?;

    let session = format!("parity-smoke-{}", std::process::id());
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = i32::from(listener.local_addr()?.port());
    let mut request = ScanProbeRequest {
        session_id: session.clone(),
        spec: ScanTargetSpec {
            entries: vec![ScanEntry {
                cidr: "127.0.0.1".into(),
                port,
            }],
            shuffle: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let count = core.scan_probe(&request)?;
    assert!(count.error.is_empty(), "count: {}", count.error);
    assert_eq!(count.total, 1);
    assert!(count.results.is_empty());
    println!("PASS: count-only ScanProbe reports one target without probing");

    request.max_targets = 1;
    request.concurrency = 1;
    request.tcp = ScanTcpOptions {
        enabled: true,
        timeout_ms: 1000,
        ..Default::default()
    };
    let response = core.scan_probe(&request)?;
    assert!(response.error.is_empty(), "probe: {}", response.error);
    assert_eq!(response.next_cursor, 1);
    assert_eq!(response.results.len(), 1);
    assert!(response.results[0].passed, "{:?}", response.results[0]);
    assert_eq!(response.results[0].probe_port, port);
    let progress = core.query_scan(&session, 0)?;
    assert_eq!((progress.probed, progress.probe_passed), (1, 1));
    println!("PASS: loopback TCP probe and cumulative progress report success");

    core.stop_scan(&session)?;
    let stopped = core.scan_probe(&request)?;
    assert!(!stopped.error.is_empty());
    assert!(stopped.results.is_empty());
    println!("PASS: stopped scan session refuses subsequent probes");

    let parsed = core.parse_rule_set(
        br#"{"version":4,"rules":[{"ip_cidr":["192.0.2.0/24","2001:db8::/32"]}]}"#,
    )?;
    assert!(parsed.error.is_empty(), "rules: {}", parsed.error);
    assert_eq!(parsed.cidrs, ["192.0.2.0/24", "2001:db8::/32"]);
    println!("PASS: native rule-set parser returns IPv4 and IPv6 CIDRs");
    core.shutdown()?;
    Ok(())
}

fn check_beta2_imports(core: &mut CoreSession) -> Result<(), Box<dyn Error>> {
    let key = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=";
    let links = [
        "trojan://password@edge.example:443?ech=public.example+https://1.1.1.1/dns-query".to_string(),
        format!("awg://{key}@192.0.2.1:51820?public_key={key}&address=10.0.0.2/32&workers=2&jc=4&jmin=50&jmax=100&h1=1234"),
        "vless://11111111-1111-1111-1111-111111111111@edge.example:443?security=TLS&type=xhttp&sni=identity.example&ech=public.example%2Bhttps%3A%2F%2F1.1.1.1%2Fdns-query&extra=%7B%22noGRPCHeader%22%3Atrue%7D".into(),
    ];
    for link in links {
        let report = throne_import::import_text(&link);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        let imported = report
            .profiles
            .into_iter()
            .next()
            .ok_or("no imported profile")?;
        let mut profile = Profile::new(3, 1, "beta2 smoke", imported.profile_type);
        profile.outbound_json = imported.outbound.to_db_json(imported.profile_type);
        profile.outbound = imported.outbound;
        let settings = AppSettings {
            tun_mode_enabled: true,
            fake_dns: true,
            ..Default::default()
        };
        let built = build_load_config(&profile, &settings, None)?;
        core.check_config_json(&built.core_config_json)?;
        for xray in &built.xray_full_configs {
            core.check_xray_config_json(xray)?;
        }
    }
    println!("PASS: imported ECH, Amnezia endpoint, XHTTP/Xray, and IPv6 FakeIP configs validate in the rebuilt core");
    Ok(())
}

fn check_beta3_contracts(core: &mut CoreSession) -> Result<(), Box<dyn Error>> {
    use serde_json::json;
    use throne_core_client::{build_load_config_ex, AutoSelectorBuild};
    use throne_domain::{AppState, RouteProfile, RouteRule};
    let mut state = AppState::empty();
    let a = state.add_group("first");
    let b = state.add_group("second");
    for gid in [a, b] {
        let id = state.add_profile(gid, "member", ProfileType::Socks);
        state.update_profile_outbound_json(
            id,
            json!({"type":"socks","server":"127.0.0.1","server_port":1080}).to_string(),
        )?;
    }
    let selector = state.add_auto_selector(a, a, "two groups")?;
    state.update_profile_outbound_json(
        selector,
        json!({"type":"autoselector","gids":[a,b]}).to_string(),
    )?;
    let (config, _, members) = state.resolve_auto_selector_members(selector)?;
    assert_eq!(members.len(), 2);
    let auto = AutoSelectorBuild { config, members };
    let mut route = RouteProfile::new(1, "fragment");
    route.rules.push(RouteRule {
        action: "route-options".into(),
        domain: vec!["example.com".into()],
        tls_fragment: true,
        tls_fragment_fallback_delay: "500ms".into(),
        ..Default::default()
    });
    let built = build_load_config_ex(
        state.profile(selector).unwrap(),
        &AppSettings::default(),
        Some(&route),
        Some(&auto),
    )?;
    core.check_config_json(&built.core_config_json)?;
    let mut invalid: serde_json::Value = serde_json::from_str(&built.core_config_json)?;
    let rule = invalid["route"]["rules"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["action"] == "route-options")
        .unwrap();
    rule["tls_record_fragment"] = json!(true);
    assert!(
        matches!(
            core.check_config_json(&invalid.to_string()),
            Err(CoreError::Config(_))
        ),
        "route-options cannot enable both fragment modes"
    );
    println!("PASS: multi-group selector and route-options TLS fragmentation validate; conflicting modes rejected");
    for link in [
        "vless://11111111-1111-1111-1111-111111111111@edge.example:443?security=reality&fp=unsafe&pbk=AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE&sni=identity.example",
        "vless://11111111-1111-1111-1111-111111111111@edge.example:443?security=tls&fp=unsafe&cs=BOGUS,TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
    ] {
        let report = throne_import::import_text(link);
        assert!(report.errors.is_empty());
        let imported = report.profiles.into_iter().next().ok_or("missing beta3 import")?;
        let mut p = Profile::new(10, 1, "beta3 TLS", imported.profile_type);
        p.outbound_json = imported.outbound.to_db_json(imported.profile_type);
        p.outbound = imported.outbound;
        let built = build_load_config(&p, &AppSettings::default(), None)?;
        core.check_config_json(&built.core_config_json)?;
    }
    println!("PASS: native REALITY unsafe fallback and filtered TLS cipher suites validate");
    let _capabilities = core.core_privileges()?;
    let packets = core.generate_quic_signature("example.com")?;
    assert!((1..=5).contains(&packets.len()));
    assert!(packets.iter().all(|p| !p.is_empty()));
    assert!(core.generate_quic_signature("127.0.0.1").is_err());
    println!(
        "PASS: privilege capabilities and offline QUIC signature RPCs respond; IP input rejected"
    );
    Ok(())
}
