use throne_core_client::{CoreConfig, CoreSession, ResolveDomainsRequest, ResolveDomainsResponse};

#[test]
fn empty_resolve_request_does_not_start_a_core_process() {
    let mut session = CoreSession::new(CoreConfig::default());
    assert_eq!(
        session
            .resolve_domains(&ResolveDomainsRequest::default())
            .unwrap(),
        ResolveDomainsResponse::default(),
    );
    assert!(!session.is_connected());
    assert_eq!(session.running_profile_id(), None);
}

#[test]
fn disabled_mixed_inbound_rejects_system_proxy_before_starting_core() {
    use throne_core_client::CoreError;
    use throne_domain::{AppSettings, Profile, ProfileType};
    let profile = Profile::new(1, 1, "test", ProfileType::Socks);
    for (saved_proxy, apply_proxy) in [(true, false), (false, true), (true, true)] {
        let mut session = CoreSession::new(CoreConfig::default());
        let settings = AppSettings {
            disable_mixed_inbound: true,
            system_proxy_enabled: saved_proxy,
            // Guard must precede privilege setup as well as IPC.
            tun_mode_enabled: true,
            ..Default::default()
        };
        let error = session
            .start_profile_ex(&profile, &settings, None, None, apply_proxy)
            .unwrap_err();
        assert!(
            matches!(error, CoreError::Config(message) if message.contains("System proxy requires the mixed"))
        );
        assert!(!session.is_connected());
        assert_eq!(session.running_profile_id(), None);
        assert!(session.take_core_logs().is_empty());
    }
}
