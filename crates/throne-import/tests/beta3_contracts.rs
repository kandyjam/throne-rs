use serde_json::{json, Value};
use throne_import::{import_text, to_share_object};

#[test]
fn cipher_suites_and_unsafe_fingerprint_survive_link_import() {
    let report = import_text("vless://11111111-1111-1111-1111-111111111111@example.com:443?security=tls&fp=unsafe&tls_cipher_suites=OLD&cs=TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256%3A%20TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384#tls");
    assert_eq!(report.profiles.len(), 1);
    let p = &report.profiles[0];
    let out: Value = serde_json::from_str(&p.outbound.to_db_json(p.profile_type)).unwrap();
    assert_eq!(
        out["tls"]["cipher_suites"],
        json!([
            "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
            "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384"
        ])
    );
    assert_ne!(out["tls"]["utls"]["enabled"], true);
}

#[test]
fn xhttp_tls_keeps_cipher_suites_and_insecure_and_reality_ignores_unsafe() {
    for security in ["tls", "reality"] {
        let report = import_text(&format!("vless://11111111-1111-1111-1111-111111111111@example.com:443?type=xhttp&security={security}&fp=unsafe&cs=A,B&allowInsecure=1&pbk=key#x"));
        assert_eq!(report.profiles.len(), 1);
        let p = &report.profiles[0];
        let out: Value = serde_json::from_str(&p.outbound.to_db_json(p.profile_type)).unwrap();
        let raw: Value = serde_json::from_str(out["config"].as_str().unwrap()).unwrap();
        let stream = &raw["outbounds"][0]["streamSettings"];
        if security == "tls" {
            assert_eq!(stream["tlsSettings"]["cipherSuites"], "A:B");
            assert_eq!(stream["tlsSettings"]["allowInsecure"], true);
        } else {
            assert_ne!(stream["realitySettings"]["fingerprint"], "unsafe");
        }
    }
}

#[test]
fn route_share_roundtrip_keeps_tls_fragment_options() {
    let report = import_text(&json!({"kind":"throne-route-profile","v":1,"name":"fragment","rules":[{"action":"route-options","domain":["example.com"],"tls_fragment":true,"tls_fragment_fallback_delay":"500ms","tls_record_fragment":false}]}).to_string());
    assert_eq!(report.routes.len(), 1);
    let exported = to_share_object(&report.routes[0]);
    assert_eq!(exported["rules"][0]["tls_fragment"], true);
    assert_eq!(exported["rules"][0]["tls_fragment_fallback_delay"], "500ms");
}

#[test]
fn invalid_clash_ports_do_not_wrap_into_valid_endpoints() {
    for port in ["65537", "-65535", "65536", "0", "1.5"] {
        let report = import_text(&format!(
            "proxies:\n - {{name: invalid, type: socks5, server: example.com, port: {port}}}"
        ));
        assert!(report.profiles.is_empty(), "accepted invalid port {port}");
    }
}

#[test]
fn native_reality_unsafe_uses_required_random_utls() {
    let report = import_text("vless://11111111-1111-1111-1111-111111111111@example.com:443?security=reality&fp=unsafe&pbk=AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE#reality");
    let p = &report.profiles[0];
    let out: Value = serde_json::from_str(&p.outbound.to_db_json(p.profile_type)).unwrap();
    assert_eq!(
        out["tls"]["utls"],
        json!({"enabled":true,"fingerprint":"random"})
    );
}

#[test]
fn unsupported_native_suites_are_dropped_with_visible_import_notes() {
    let report = import_text("vless://11111111-1111-1111-1111-111111111111@example.com:443?security=tls&cs=BOGUS,TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,TLS_RSA_WITH_RC4_128_SHA");
    let p = &report.profiles[0];
    let out: Value = serde_json::from_str(&p.outbound.to_db_json(p.profile_type)).unwrap();
    assert_eq!(
        out["tls"]["cipher_suites"],
        json!(["TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256"])
    );
    assert_eq!(report.notes.len(), 2);
    assert!(report.notes[0].contains("BOGUS"));
    assert!(report.errors.is_empty());
}

#[test]
fn partly_invalid_clash_feed_reports_errors_to_prevent_snapshot_deletion() {
    let report = import_text("proxies:\n - {name: valid, type: socks5, server: example.com, port: 1080}\n - {name: invalid, type: socks5, server: example.com, port: 65537}");
    assert_eq!(report.profiles.len(), 1);
    assert_eq!(report.skipped, 1);
    assert!(!report.errors.is_empty());
}

#[test]
fn xray_json_preserves_cipher_suites_in_full_config_bridge() {
    let report = import_text(
        r#"{"outbounds":[{"protocol":"vless","tag":"cipher","settings":{"vnext":[{"address":"example.com","port":443,"users":[{"id":"11111111-1111-1111-1111-111111111111"}]}]},"streamSettings":{"security":"tls","tlsSettings":{"cipherSuites":"TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256"}}}]}"#,
    );
    assert_eq!(
        report.profiles[0].profile_type,
        throne_domain::ProfileType::Custom
    );
    assert!(report.profiles[0]
        .outbound
        .raw_json
        .as_deref()
        .unwrap()
        .contains("cipherSuites"));
}
