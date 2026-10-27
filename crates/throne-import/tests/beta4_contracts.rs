use serde_json::{json, Value};
use throne_domain::AppSettings;
use throne_import::{import_text, FetchOptions};

#[test]
fn xdrive_json_preserves_encryption_storage_and_mux_in_full_bridge() {
    let outbound = json!({"tag":"drive","protocol":"vless","settings":{"vnext":[{"address":"example.com","port":443,"users":[{"id":"11111111-1111-1111-1111-111111111111","encryption":"mlkem768x25519plus.example"}]}]},"streamSettings":{"network":"xdrive","xdriveSettings":{"service":"webdav","remoteFolder":"folder","secrets":["opaque-secret"],"template":{"url":"https://example.com"},"segmentBytes":4096,"xmux":{"maxConcurrency":8}}},"mux":{"enabled":true,"concurrency":8}});
    let report = import_text(&outbound.to_string());
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.profiles.len(), 1);
    let p = &report.profiles[0];
    let bean: Value = serde_json::from_str(&p.outbound.to_db_json(p.profile_type)).unwrap();
    assert_eq!(bean["subtype"], "xrayfullconfig");
    let config: Value = serde_json::from_str(bean["config"].as_str().unwrap()).unwrap();
    assert_eq!(config["outbounds"][0], outbound);
}

#[test]
fn xdrive_share_link_is_rejected_instead_of_silently_building_tcp() {
    let report =
        import_text("vless://11111111-1111-1111-1111-111111111111@example.com:443?type=xdrive");
    assert!(report.profiles.is_empty());
    assert!(!report.errors.is_empty());
}

#[test]
fn disabled_mixed_inbound_uses_active_tun_or_reports_unavailable_proxy() {
    let mut settings = AppSettings {
        net_use_proxy: true,
        disable_mixed_inbound: true,
        ..Default::default()
    };
    assert!(FetchOptions::from_settings(&settings, false).is_err());
    assert!(FetchOptions::from_settings(&settings, true).is_err());
    settings.tun_mode_enabled = true;
    assert!(FetchOptions::from_settings(&settings, true)
        .unwrap()
        .proxy_url
        .is_none());
    settings.disable_mixed_inbound = false;
    assert!(FetchOptions::from_settings(&settings, true)
        .unwrap()
        .proxy_url
        .is_some());
}

#[test]
fn case_insensitive_xray_transport_and_explicit_trust_choices_use_lossless_bridge() {
    for stream in [
        json!({"network":"XDrive","xdriveSettings":{"service":"webdav","segmentBytes":4096}}),
        json!({"network":"tcp","security":"tls","tlsSettings":{"UseSystemCA":false}}),
    ] {
        let outbound = json!({"protocol":"vless","settings":{"vnext":[{"address":"example.com","port":443,"users":[{"id":"11111111-1111-1111-1111-111111111111"}]}]},"streamSettings":stream});
        let report = import_text(&outbound.to_string());
        assert_eq!(report.profiles.len(), 1);
        let p = &report.profiles[0];
        let bean: Value = serde_json::from_str(&p.outbound.to_db_json(p.profile_type)).unwrap();
        assert_eq!(bean["subtype"], "xrayfullconfig");
        let config: Value = serde_json::from_str(bean["config"].as_str().unwrap()).unwrap();
        assert_eq!(config["outbounds"][0], outbound);
    }
}

#[test]
fn xray_method_and_case_aliases_preserve_full_config_and_precedence() {
    for stream_fields in [
        json!({"streamSettings":{"network":"tcp","method":"xdrive","xdriveSettings":{"service":"webdav"}}}),
        json!({"StreamSettings":{"Network":"tcp","Method":"XHTTP","xhttpSettings":{"path":"/upload"}}}),
        json!({"STREAMSETTINGS":{"NETWORK":"SplitHTTP"}}),
        // Retain both values so Xray, not import normalization, applies method precedence.
        json!({"streamSettings":{"network":"xdrive","method":"tcp"}}),
        json!({"StreamSettings":{"method":"xdrive"},"streamSettings":{"network":"tcp","method":null}}),
        json!({"StreamSettings":{"network":"xdrive"},"streamSettings":null}),
        json!({"StreamSettings":null,"streamSettings":{"Method":"xdrive","network":null}}),
    ] {
        assert_lossless_xray_stream_fields(stream_fields);
    }
}

#[test]
fn xray_tls_case_aliases_and_null_choices_preserve_full_config() {
    for stream_fields in [
        json!({"StreamSettings":{"TLSSettings":{"UseSystemCA":false}}}),
        json!({"streamSettings":{"TLSSETTINGS":{"USESYSTEMCA":null}}}),
        json!({"StreamSettings":{"tlsSettings":{"CipherSuites":"TLS_AES_128_GCM_SHA256"}}}),
        json!({"streamSettings":{"TlsSettings":{"ECHCONFIGLIST":"opaque-ech-config"}}}),
        json!({"streamSettings":{"tlsSettings":{"CipherSuites":null,"EchConfigList":null}}}),
        json!({"streamSettings":{"TLSSettings":{"useSystemCA":false},"tlsSettings":null}}),
        json!({"StreamSettings":{"TLSSettings":{"useSystemCA":false}},"streamSettings":{"tlsSettings":{"useSystemCA":null}}}),
    ] {
        assert_lossless_xray_stream_fields(stream_fields);
    }
}

fn assert_lossless_xray_stream_fields(stream_fields: Value) {
    let mut outbound = json!({
        "protocol":"vless",
        "settings":{"vnext":[{"address":"example.com","port":443,"users":[{"id":"11111111-1111-1111-1111-111111111111"}]}]}
    });
    outbound
        .as_object_mut()
        .unwrap()
        .extend(stream_fields.as_object().unwrap().clone());
    let report = import_text(&outbound.to_string());
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.profiles.len(), 1, "{outbound}");
    let profile = &report.profiles[0];
    let bean: Value =
        serde_json::from_str(&profile.outbound.to_db_json(profile.profile_type)).unwrap();
    assert_eq!(bean["subtype"], "xrayfullconfig", "{outbound}");
    let config: Value = serde_json::from_str(bean["config"].as_str().unwrap()).unwrap();
    assert_eq!(config["outbounds"][0], outbound);
}
