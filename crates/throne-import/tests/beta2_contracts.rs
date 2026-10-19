use base64::Engine;
use serde_json::{json, Value};
use throne_domain::ProfileType;
use throne_import::import_text;

fn imported_json(link: &str) -> Value {
    let report = import_text(link);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.profiles.len(), 1);
    let p = &report.profiles[0];
    serde_json::from_str(&p.outbound.to_db_json(p.profile_type)).unwrap()
}

#[test]
fn ech_query_target_preserves_literal_plus_and_resolver() {
    let out = imported_json(
        "vless://id@edge.example:443?security=tls&ech=public.example+https://dns.example/dns-query",
    );
    assert_eq!(out["tls"]["ech"]["enabled"], true);
    assert_eq!(out["tls"]["ech"]["query_server_name"], "public.example");
    assert_eq!(
        out["tls"]["ech"]["resolver"],
        "https://dns.example/dns-query"
    );
}

#[test]
fn ech_static_configs_are_pem_wrapped_and_legacy_flags_take_precedence() {
    let out = imported_json("trojan://pw@edge.example:443?ech=A+B/==");
    assert_eq!(
        out["tls"]["ech"]["config"],
        json!([
            "-----BEGIN ECH CONFIGS-----",
            "A+B/==",
            "-----END ECH CONFIGS-----"
        ])
    );
    let out = imported_json("trojan://pw@edge.example:443?ech=1&ech_enabled=false");
    assert!(out["tls"]["ech"].is_null() || out["tls"]["ech"]["enabled"] == false);
}

#[test]
fn whole_certificate_pins_are_validated_and_not_confused_with_public_key_pins() {
    let hex = "ab".repeat(32);
    let out = imported_json(&format!(
        "trojan://pw@edge.example:443?pcs={hex},bad&allowInsecure=1"
    ));
    let expected = base64::engine::general_purpose::STANDARD.encode([0xab; 32]);
    assert_eq!(out["tls"]["certificate_sha256"], json!([expected]));
    assert!(out["tls"]["certificate_public_key_sha256"].is_null());
    assert_eq!(out["tls"]["insecure"], true);
}

#[test]
fn xhttp_strict_json_values_survive_and_use_the_xray_bridge() {
    let extra = r#"{"headers":{"X-Value":"True's A+B"},"noGRPCHeader":true}"#;
    let link = format!("vless://id@edge.example:443?security=tls&type=xhttp&path=%2Fx&mode=auto&extra={}&ech=public.example%2Bhttps%3A%2F%2Fdns.example%2Fdns-query#Node", urlencoding::encode(extra));
    let report = import_text(&link);
    let profile = &report.profiles[0];
    assert_eq!(profile.profile_type, ProfileType::Custom);
    let out: Value =
        serde_json::from_str(&profile.outbound.to_db_json(profile.profile_type)).unwrap();
    assert_eq!(out["subtype"], "xrayfullconfig");
    let config: Value = serde_json::from_str(out["config"].as_str().unwrap()).unwrap();
    let stream = &config["outbounds"][0]["streamSettings"];
    assert_eq!(
        stream["xhttpSettings"]["extra"]["headers"]["X-Value"],
        "True's A+B"
    );
    assert_eq!(
        stream["tlsSettings"]["echConfigList"],
        "public.example+https://dns.example/dns-query"
    );
}

const WG: &str = "[Interface]\nPrivateKey = private\nAddress = 10.0.0.2/32\n# Tokyo\n[Peer]\nPublicKey = public\nEndpoint = edge.example:51820\n";

#[test]
fn xhttp_form_and_python_repr_preserve_string_contents() {
    for (encoded, expected) in [
        (
            "%7B%22headers%22%3A+%7B%22X-Token%22%3A+%22A%2BB%22%7D%7D".to_string(),
            "A+B",
        ),
        (
            urlencoding::encode(
                "{'headers': {'X-Token': 'TrueToken False None A+B'}, 'noGRPCHeader': True}",
            )
            .into_owned(),
            "TrueToken False None A+B",
        ),
    ] {
        let out = imported_json(&format!(
            "vless://id@edge.example:443?type=xhttp&extra={encoded}"
        ));
        let config: Value = serde_json::from_str(out["config"].as_str().unwrap()).unwrap();
        assert_eq!(
            config["outbounds"][0]["streamSettings"]["xhttpSettings"]["extra"]["headers"]
                ["X-Token"],
            expected
        );
    }
}

#[test]
fn wireguard_single_peer_fallback_keeps_last_peer_fields_together() {
    let first = "[Interface]\nPrivateKey=private\nAddress=10.0.0.2/32\n[Peer]\nPublicKey=first\nPresharedKey=firstpsk\nPersistentKeepalive=10\nEndpoint=first.example:1111\n";
    for last_psk in ["PresharedKey=lastpsk\n", ""] {
        let out = imported_json(&format!(
            "{first}[Peer]\nPublicKey=last\n{last_psk}Endpoint=last.example:2222\n"
        ));
        let peer = &out["peers"][0];
        assert_eq!(peer["address"], "last.example");
        assert_eq!(peer["port"], 2222);
        assert_eq!(peer["public_key"], "last");
        if last_psk.is_empty() {
            assert!(peer["pre_shared_key"].is_null());
        } else {
            assert_eq!(peer["pre_shared_key"], "lastpsk");
        }
        assert!(peer["persistent_keepalive_interval"].is_null());
    }
}

#[test]
fn wireguard_json_import_recovers_peer_display_fields() {
    let raw = json!({"type":"wireguard","tag":"WG","private_key":"private","address":["10.0.0.2/32"],"peers":[{"address":"edge.example","port":51820,"public_key":"public","pre_shared_key":"secret"}]});
    let report = import_text(&raw.to_string());
    let outbound = &report.profiles[0].outbound;
    assert_eq!(outbound.server.as_deref(), Some("edge.example"));
    assert_eq!(outbound.server_port, Some(51820));
    assert_eq!(outbound.password.as_deref(), Some("private"));
    assert_eq!(outbound.username.as_deref(), Some("public"));
    assert_eq!(outbound.method.as_deref(), Some("secret"));
    assert_eq!(outbound.path.as_deref(), Some("10.0.0.2/32"));
}

#[test]
fn mixed_case_xhttp_security_preserves_tls_and_reality_options() {
    for (security, key) in [("TLS", "tlsSettings"), ("ReAliTy", "realitySettings")] {
        let out = imported_json(&format!("vless://id@edge.example:443?security={security}&type=xhttp&sni=identity.example&pbk=pub&ech=public.example"));
        let config: Value = serde_json::from_str(out["config"].as_str().unwrap()).unwrap();
        let stream = &config["outbounds"][0]["streamSettings"];
        assert_eq!(stream["security"], security.to_lowercase());
        assert_eq!(stream[key]["serverName"], "identity.example");
        if key == "tlsSettings" {
            assert_eq!(stream[key]["echConfigList"], "public.example");
        } else {
            assert_eq!(stream[key]["publicKey"], "pub");
        }
    }
}

#[test]
fn amnezia_options_and_preshared_key_survive_uri_and_ini_import() {
    let out = imported_json("awg://private@edge.example:51820?public_key=public&psk=secret&address=10.0.0.2&jc=4&h1=1234&random_trailers=true&persistent_keepalive=25");
    assert_eq!(out["amnezia_wg"]["jc"], 4);
    assert_eq!(out["amnezia_wg"]["h1"], "1234");
    assert_eq!(out["amnezia_wg"]["random_trailers"], true);
    assert_eq!(out["peers"][0]["pre_shared_key"], "secret");
    assert_eq!(out["peers"][0]["persistent_keepalive_interval"], 25);
    assert_eq!(out["address"][0], "10.0.0.2/32");
    let out = imported_json(&format!(
        "{WG}\nJc=5\nH1=100-200\nHeaderProtectionKey=abc\n"
    ));
    assert_eq!(out["amnezia_wg"]["jc"], 5);
    assert_eq!(out["amnezia_wg"]["h1"], "100-200");
    assert_eq!(out["amnezia_wg"]["header_protection_key"], "abc");
}

#[test]
fn wireguard_names_use_explicit_comments_then_adjacent_peer_comment() {
    assert_eq!(import_text(WG).profiles[0].name, "Tokyo");
    assert_eq!(
        import_text(&format!("# Name: Preferred\n{WG}")).profiles[0].name,
        "Preferred"
    );
    for comment in [
        "Peer 1",
        "WireGuard configuration",
        "DNS = 1.1.1.1",
        "----",
        "Tokyo\n",
    ] {
        assert_eq!(
            import_text(&WG.replace("Tokyo", comment)).profiles[0].name,
            "WireGuard"
        );
    }
    let link = "awg://private@edge.example:51820?public_key=public#Amnezia";
    assert_eq!(import_text(link).profiles[0].name, "Amnezia");
}

#[test]
fn vpn_names_prefer_fragment_and_disambiguate_wireguard_containers() {
    let body = json!({"description":"Office", "containers":[
        {"container":"amnezia-awg", "awg":{"last_config":{"config":WG}}},
        {"container":"amnezia-wg", "wg":{"last_config":WG}}
    ]});
    let payload = base64::engine::general_purpose::STANDARD.encode(body.to_string());
    let report = import_text(&format!("vpn://{payload}#My%20VPN"));
    assert_eq!(
        report
            .profiles
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["My VPN (amnezia-awg)", "My VPN (amnezia-wg)"]
    );
    let payload = base64::engine::general_purpose::STANDARD.encode(WG);
    assert_eq!(
        import_text(&format!("vpn://{payload}#Wrapper")).profiles[0].name,
        "Wrapper"
    );
}
