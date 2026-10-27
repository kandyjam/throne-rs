use super::*;
#[test]
fn edited_fields_merge_without_overwriting_unrelated_or_concurrent_settings() {
    let initial = AppSettings::default();
    let mut edited = initial.clone();
    edited.mux_padding = true;
    edited.mux_protocol = "yamux".into();
    let mut current = initial.clone();
    current.remote_dns = "https://resolver.example/dns-query".into();
    current.mux_concurrency = 19;
    apply_changed(&initial, &edited, &mut current);
    assert!(current.mux_padding);
    assert_eq!(current.mux_protocol, "yamux");
    assert_eq!(current.mux_concurrency, 19);
    assert_eq!(current.remote_dns, "https://resolver.example/dns-query");
}
#[test]
fn numeric_inputs_reject_negative_overflow_and_require_brutal_bandwidth() {
    let values = ["8", "4", "4", "0", "0"].map(str::to_string);
    assert!(read_numbers(AppSettings::default(), &values).is_ok());
    for invalid in ["-1", "x", "2147483648", "1.5"] {
        let mut bad = values.clone();
        bad[0] = invalid.into();
        assert!(read_numbers(AppSettings::default(), &bad).is_err());
    }
    assert!(read_numbers(
        AppSettings {
            mux_brutal: true,
            ..Default::default()
        },
        &values
    )
    .is_err());
    assert!(read_numbers(
        AppSettings {
            mux_protocol: "unknown".into(),
            ..Default::default()
        },
        &values
    )
    .is_err());
    assert!(read_numbers(
        AppSettings {
            mux_mode: "unknown".into(),
            ..Default::default()
        },
        &values
    )
    .is_err());
}
