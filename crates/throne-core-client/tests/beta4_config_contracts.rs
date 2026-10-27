use serde_json::{json, Value};
use throne_core_client::build_load_config;
use throne_domain::{AppSettings, Profile, ProfileType, RouteDns, RouteProfile, RouteRule};
fn profile(mux: Value) -> Profile {
    let mut p = Profile::new(1, 1, "test", ProfileType::Vless);
    let mut ob = json!({"type":"vless","server":"proxy.example","server_port":443,"uuid":"11111111-1111-1111-1111-111111111111"});
    if !mux.is_null() {
        ob["multiplex"] = mux;
    }
    p.outbound_json = ob.to_string();
    p
}
fn config(p: &Profile, s: &AppSettings, r: Option<&RouteProfile>) -> Value {
    serde_json::from_str(&build_load_config(p, s, r).unwrap().core_config_json).unwrap()
}
#[test]
fn custom_route_dns_overrides_global_raw_without_mutating_global() {
    let s = AppSettings {
        use_dns_object: true,
        dns_object: r#"{"servers":[{"type":"local","tag":"global"}],"final":"global"}"#.into(),
        ..Default::default()
    };
    let mut r = RouteProfile::new(1, "own");
    r.custom_dns = true;
    r.dns = Some(RouteDns {
        direct_dns: "9.9.9.9".into(),
        remote_dns: "https://1.1.1.1/dns-query".into(),
        dns_final_out: "direct".into(),
        direct_dns_disable_ipv6: true,
        ..Default::default()
    });
    let v = config(&profile(Value::Null), &s, Some(&r));
    assert!(v["dns"]["servers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["tag"] == "dns-direct" && x["server"] == "9.9.9.9"));
    assert_eq!(
        v["dns"]["rules"].as_array().unwrap().last().unwrap()["server"],
        "dns-direct"
    );
    assert_eq!(
        v["dns"]["rules"].as_array().unwrap().last().unwrap()["strategy"],
        "ipv4_only"
    );
    assert!(s.use_dns_object);
    r.custom_dns = false;
    assert_eq!(
        config(&profile(Value::Null), &s, Some(&r))["dns"]["final"],
        "global"
    );
}
#[test]
fn disabled_rules_never_emit_route_dns_or_fetch_their_rulesets() {
    let mut r = RouteProfile::new(1, "rules");
    r.custom_dns = true;
    r.dns = Some(RouteDns::default());
    r.rules.push(RouteRule {
        disabled: true,
        name: "off".into(),
        domain: vec!["off.example".into()],
        rule_set: vec!["geosite-off".into()],
        outbound_id: -2,
        ..Default::default()
    });
    r.rules.push(RouteRule {
        domain: vec!["on.example".into()],
        outbound_id: -2,
        ..Default::default()
    });
    let v = config(&profile(Value::Null), &AppSettings::default(), Some(&r));
    assert!(!v.to_string().contains("off.example"));
    assert!(!v.to_string().contains("geosite-off"));
    assert!(v["dns"]["rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["domain"] == json!(["on.example"]) && x["server"] == "dns-direct"));
}
#[test]
fn multiplex_on_uses_preset_custom_can_disable_padding_and_default_off_stays_off() {
    let s = AppSettings {
        mux_protocol: "yamux".into(),
        mux_mode: "max_connections".into(),
        mux_max_connections: 3,
        mux_min_streams: 2,
        mux_padding: true,
        ..Default::default()
    };
    let v = config(&profile(json!({"enabled":true})), &s, None);
    let mux = &v["outbounds"][0]["multiplex"];
    assert_eq!(mux["protocol"], "yamux");
    assert_eq!(mux["max_connections"], 3);
    assert!(mux.get("max_streams").is_none());
    let v = config(
        &profile(json!({"enabled":true,"protocol":"smux","max_streams":5,"padding":false})),
        &s,
        None,
    );
    let mux = &v["outbounds"][0]["multiplex"];
    assert_eq!(mux["max_streams"], 5);
    assert!(mux.get("max_connections").is_none());
    assert_ne!(mux["padding"], true);
    assert!(config(
        &profile(json!({"unspecified":true,"protocol":"smux"})),
        &s,
        None
    )["outbounds"][0]
        .get("multiplex")
        .is_none());
}
#[test]
fn disabled_mixed_inbound_is_not_recreated_by_config_generation() {
    let s = AppSettings {
        disable_mixed_inbound: true,
        ..Default::default()
    };
    assert!(config(&profile(Value::Null), &s, None)["inbounds"]
        .as_array()
        .unwrap()
        .is_empty());
}
#[cfg(target_os = "macos")]
#[test]
fn darwin_tun_uses_loopback_system_dns_and_first_hijack_rule() {
    let s = AppSettings {
        tun_mode_enabled: true,
        ..Default::default()
    };
    let v = config(&profile(Value::Null), &s, None);
    let inbound = v["inbounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["tag"] == "dns-system")
        .unwrap();
    assert_eq!(inbound["listen"], "127.0.0.53");
    assert_eq!(inbound["listen_port"], 53);
    assert_eq!(inbound["reuse_addr"], true);
    assert_eq!(
        v["route"]["rules"][0],
        json!({"inbound":"dns-system","action":"hijack-dns"})
    );
    assert!(
        !config(&profile(Value::Null), &AppSettings::default(), None)
            .to_string()
            .contains("dns-system")
    );
}

#[test]
fn fakeip_precedes_route_dns_matches_but_not_proxy_bootstrap() {
    let mut route = RouteProfile::new(1, "fake");
    route.custom_dns = true;
    route.dns = Some(RouteDns {
        fake_dns: true,
        ..Default::default()
    });
    route.rules.push(RouteRule {
        domain: vec!["direct.example".into()],
        outbound_id: -2,
        ..Default::default()
    });
    let v = config(&profile(Value::Null), &AppSettings::default(), Some(&route));
    let rules = v["dns"]["rules"].as_array().unwrap();
    let fake = rules
        .iter()
        .position(|r| r["server"] == "dns-fake")
        .unwrap();
    let direct = rules
        .iter()
        .position(|r| r["domain"] == json!(["direct.example"]))
        .unwrap();
    let bootstrap = rules
        .iter()
        .position(|r| r["domain"] == json!(["proxy.example"]))
        .unwrap();
    assert!(bootstrap < fake && fake < direct);
}

#[test]
fn resolver_probe_uses_route_dns_without_listeners_or_proxy_dependencies() {
    let settings = AppSettings {
        tun_mode_enabled: true,
        fake_dns: true,
        direct_dns: "8.8.8.8".into(),
        ..Default::default()
    };
    let mut route = RouteProfile::new(1, "own");
    route.custom_dns = true;
    route.dns = Some(RouteDns {
        direct_dns: "HTTPS://Dns.Example:8443/CaseSensitive".into(),
        direct_dns_disable_ipv6: true,
        ..Default::default()
    });
    let v: Value = serde_json::from_str(
        &throne_core_client::build_resolve_config(&settings, Some(&route)).unwrap(),
    )
    .unwrap();
    let direct = v["dns"]["servers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["tag"] == "dns-direct")
        .unwrap();
    assert_eq!(direct["type"], "https");
    assert_eq!(direct["server"], "Dns.Example");
    assert_eq!(direct["server_port"], 8443);
    assert_eq!(direct["path"], "/CaseSensitive");
    assert!(v
        .get("inbounds")
        .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty)));
    assert!(!v.to_string().contains("dns-fake"));
    assert!(!v["outbounds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|o| o["tag"] == "proxy"));
}

fn xray_config(streams: Value, opt_in: bool) -> Value {
    let mut outbound = json!({"protocol":"freedom","tag":"proxy"});
    for (key, value) in streams.as_object().unwrap() {
        outbound[key] = value.clone();
    }
    let full = json!({"inbounds":[{"protocol":"socks","listen":"127.0.0.1","port":1080}],"outbounds":[outbound]});
    let mut p = Profile::new(7, 1, "Xray", ProfileType::Custom);
    p.outbound_json =
        json!({"type":"custom","subtype":"xrayfullconfig","config":full.to_string()}).to_string();
    let built = build_load_config(
        &p,
        &AppSettings {
            xray_use_system_ca: opt_in,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    serde_json::from_str(&built.xray_full_configs[0]).unwrap()
}

#[test]
fn xray_system_ca_keeps_explicit_false_across_decoder_spellings_and_extra_precedence() {
    let streams = json!({"streamSettings":{"security":"tls","tlsSettings":{"UseSystemCA":false},"network":"xhttp","xhttpSettings":{"downloadSettings":{"security":"tls"},"extra":{"DownloadSettings":{"security":"tls"}}}}});
    let v = xray_config(streams.clone(), true);
    let stream = &v["outbounds"][0]["streamSettings"];
    assert_eq!(stream["tlsSettings"]["UseSystemCA"], false);
    assert!(stream["tlsSettings"].get("useSystemCA").is_none());
    assert!(stream["xhttpSettings"]["downloadSettings"]
        .get("tlsSettings")
        .is_none());
    assert_eq!(
        stream["xhttpSettings"]["extra"]["DownloadSettings"]["tlsSettings"]["useSystemCA"],
        true
    );
    assert_eq!(
        xray_config(streams, false)["outbounds"][0]["streamSettings"]["tlsSettings"]["UseSystemCA"],
        false
    );
}

#[test]
fn xray_system_ca_obeys_pointer_reset_and_skips_reality() {
    let v = xray_config(
        json!({"StreamSettings":{"security":"tls","tlsSettings":{"useSystemCA":false}},"streamSettings":null}),
        true,
    );
    assert!(v["outbounds"][0].get("StreamSettings").is_none());
    let v = xray_config(
        json!({"streamSettings":{"security":"reality","tlsSettings":{"serverName":"ignored.example"}}}),
        true,
    );
    assert!(v["outbounds"][0]["streamSettings"]["tlsSettings"]
        .get("useSystemCA")
        .is_none());
    let v = xray_config(json!({"StreamSettings":{"Security":"TLS"}}), true);
    assert_eq!(
        v["outbounds"][0]["StreamSettings"]["tlsSettings"]["useSystemCA"],
        true
    );
}

#[test]
fn raw_dns_ech_bootstrap_references_existing_resolvers_and_keeps_user_tags() {
    for custom in [false, true] {
        let mut p = Profile::new(1, 1, "ECH", ProfileType::Trojan);
        let mut ech = json!({"enabled":true,"query_server_name":"public.example"});
        if custom {
            ech["resolver"] = json!("https://1.1.1.1/dns-query");
        }
        p.outbound_json=json!({"type":"trojan","server":"edge.example","server_port":443,"password":"pw","tls":{"enabled":true,"ech":ech}}).to_string();
        let s=AppSettings{use_dns_object:true,dns_object:json!({"servers":[{"type":"local","tag":"global"},{"type":"local","tag":"dns-ech-0"}],"final":"global"}).to_string(),..Default::default()};
        let v = config(&p, &s, None);
        let servers = v["dns"]["servers"].as_array().unwrap();
        let tags: std::collections::HashSet<_> =
            servers.iter().map(|s| s["tag"].as_str().unwrap()).collect();
        assert_eq!(tags.len(), servers.len());
        assert_eq!(servers[1], json!({"type":"local","tag":"dns-ech-0"}));
        let rule = &v["dns"]["rules"][0];
        assert!(tags.contains(rule["server"].as_str().unwrap()));
        if custom {
            let server = servers.iter().find(|s| s["tag"] == rule["server"]).unwrap();
            assert_eq!(server["domain_resolver"], "global");
            assert_ne!(server["tag"], server["domain_resolver"]);
        } else {
            assert_eq!(rule["server"], "global");
        }
    }
}

#[test]
fn accepted_local_and_auto_dhcp_dns_shorthand_emit_correct_transports() {
    for (address, expected) in [
        (
            "local://en0",
            json!({"type":"local","tag":"dns-direct","domain_resolver":"dns-local"}),
        ),
        (
            "DHCP://AUTO",
            json!({"type":"dhcp","interface":"","tag":"dns-direct","domain_resolver":"dns-local"}),
        ),
    ] {
        let s = AppSettings {
            direct_dns: address.into(),
            ..Default::default()
        };
        let v: Value =
            serde_json::from_str(&throne_core_client::build_resolve_config(&s, None).unwrap())
                .unwrap();
        let direct = v["dns"]["servers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["tag"] == "dns-direct")
            .unwrap();
        assert_eq!(direct, &expected);
    }
}

#[test]
fn ech_never_uses_proxy_dependent_raw_dns_even_when_it_is_the_default() {
    for custom in [false, true] {
        let mut p = Profile::new(1, 1, "ECH", ProfileType::Trojan);
        let mut ech = json!({"enabled":true,"query_server_name":"public.example"});
        if custom {
            ech["resolver"] = json!("https://resolver.example/dns-query");
        }
        p.outbound_json=json!({"type":"trojan","server":"edge.example","server_port":443,"password":"pw","tls":{"enabled":true,"ech":ech}}).to_string();
        let mut s=AppSettings{use_dns_object:true,dns_object:json!({"servers":[{"type":"local","tag":"bootstrap"},{"type":"https","tag":"remote","server":"1.1.1.1","detour":"proxy","domain_resolver":"bootstrap"}],"final":"remote","rules":[{"domain":["public.example"],"query_type":["HTTPS"],"server":"bootstrap"}]}).to_string(),..Default::default()};
        let v = config(&p, &s, None);
        let tag = &v["dns"]["rules"][0]["server"];
        if custom {
            let server = v["dns"]["servers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["tag"] == *tag)
                .unwrap();
            assert_eq!(server["domain_resolver"], "bootstrap");
        } else {
            assert_eq!(tag, "bootstrap");
        }
        s.dns_object=json!({"servers":[{"type":"https","tag":"remote","server":"1.1.1.1","detour":"proxy"}],"final":"remote"}).to_string();
        assert!(build_load_config(&p, &s, None)
            .unwrap_err()
            .to_string()
            .contains("independent of the proxy"));
    }
}
