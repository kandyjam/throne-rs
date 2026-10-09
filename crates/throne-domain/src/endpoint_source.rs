//! Upstream profile/group endpoint overrides, kept separate from saved outbounds.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{json, Value};
use thiserror::Error;

use crate::{Group, IpList, Profile, ProfileType};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum EndpointSource {
    #[default]
    Inherit,
    Own,
    Address {
        address: String,
    },
    IpList {
        list: i64,
    },
}

impl Serialize for EndpointSource {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Inherit => json!({}),
            Self::Own => json!({"mode": "own"}),
            Self::Address { address } => json!({"mode": "address", "address": address}),
            Self::IpList { list } => json!({"mode": "list", "list": list}),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EndpointSource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Ok(match value.get("mode").and_then(Value::as_str) {
            Some("own") => Self::Own,
            Some("address") => value
                .get("address")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| Self::Address {
                    address: s.to_owned(),
                })
                .unwrap_or_default(),
            Some("list") => value
                .get("list")
                .and_then(Value::as_i64)
                .filter(|id| *id >= 0)
                .map(|list| Self::IpList { list })
                .unwrap_or_default(),
            _ => Self::Inherit,
        })
    }
}

pub fn effective_endpoint_source<'a>(
    profile: &'a Profile,
    group: Option<&'a Group>,
) -> &'a EndpointSource {
    if profile.endpoint == EndpointSource::Inherit {
        group.map(|g| &g.endpoint).unwrap_or(&profile.endpoint)
    } else {
        &profile.endpoint
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EndpointError {
    #[error("The IP list no longer exists")]
    MissingList,
    #[error("The IP list \"{0}\" is empty")]
    EmptyList(String),
    #[error("The IP list entries have not been loaded")]
    EntriesNotLoaded,
}

/// Like upstream, use the first list entry's network address and keep the
/// profile's existing port. Snapshot lists are private to scan resumptions.
pub fn resolve_endpoint_source(
    source: &EndpointSource,
    list: Option<&IpList>,
) -> Result<Option<String>, EndpointError> {
    match source {
        EndpointSource::Inherit | EndpointSource::Own => Ok(None),
        EndpointSource::Address { address } => Ok(Some(address.trim().to_owned())),
        EndpointSource::IpList { list: id } => {
            let list = list
                .filter(|l| l.id == *id && !l.is_hidden())
                .ok_or(EndpointError::MissingList)?;
            if !list.entries_loaded {
                return Err(EndpointError::EntriesNotLoaded);
            }
            list.entries
                .first()
                .map(|entry| Some(entry.cidr.split('/').next().unwrap_or_default().to_owned()))
                .ok_or_else(|| EndpointError::EmptyList(list.name.clone()))
        }
    }
}

/// The same eligibility rule is used before assigning, resolving or applying
/// an endpoint override. Serverless profiles must not trigger list lookups.
pub fn endpoint_override_blocker(profile: &Profile) -> Option<&'static str> {
    if matches!(
        profile.profile_type,
        ProfileType::Chain
            | ProfileType::Custom
            | ProfileType::ExtraCore
            | ProfileType::Tailscale
            | ProfileType::AutoSelector
            | ProfileType::Direct
    ) {
        return Some("This profile has no single server address to replace");
    }
    let value: Value =
        serde_json::from_str(&profile.export_outbound_json()).unwrap_or_else(|_| json!({}));
    if matches!(
        profile.profile_type,
        ProfileType::Hysteria | ProfileType::Hysteria2
    ) && value.get("realm_enabled").and_then(Value::as_bool) == Some(true)
        && (profile.profile_type == ProfileType::Hysteria2
            || value.get("protocol_version").and_then(Value::as_str) == Some("2"))
    {
        return Some("Hysteria2 realm profiles have no fixed server address to replace");
    }
    None
}

/// Build a transient outbound with an effective endpoint. The live profile,
/// TLS identity, transport Host and configured port remain unchanged.
pub fn materialize_profile_endpoint(
    profile: &Profile,
    group: Option<&Group>,
    list: Option<&IpList>,
) -> Result<Profile, EndpointError> {
    let mut clone = profile.clone();
    if endpoint_override_blocker(profile).is_some() {
        return Ok(clone);
    }
    let mut value: Value =
        serde_json::from_str(&profile.export_outbound_json()).unwrap_or_else(|_| json!({}));
    let Some(address) = resolve_endpoint_source(effective_endpoint_source(profile, group), list)?
    else {
        return Ok(clone);
    };
    let address = address.trim().trim_start_matches('[').trim_end_matches(']');
    if address.is_empty() {
        return Ok(clone);
    }
    let mut original = profile
        .outbound
        .server
        .as_deref()
        .filter(|s| !s.is_empty())
        .or_else(|| value.get("server").and_then(Value::as_str))
        .unwrap_or_default()
        .trim()
        .to_owned();
    if profile.profile_type == ProfileType::OpenVpn {
        if let Some(first) = value
            .get("servers")
            .and_then(Value::as_array)
            .and_then(|s| s.first())
            .cloned()
        {
            if original.is_empty() {
                original = first
                    .get("server")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_owned();
            }
            if let Some(port) = first
                .get("server_port")
                .and_then(Value::as_u64)
                .filter(|p| *p > 0 && *p <= 65535)
            {
                value["server_port"] = port.into();
                clone.outbound.server_port = Some(port as u16);
            }
            if let Some(network) = first
                .get("network")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                value["network"] = network.into();
            }
        }
        if let Some(object) = value.as_object_mut() {
            object.remove("servers");
            object.remove("remote_random");
        }
    }
    let original = original.trim_start_matches('[').trim_end_matches(']');
    if !original.is_empty() && original.parse::<std::net::IpAddr>().is_err() {
        if clone.outbound.sni.as_deref().is_none_or(str::is_empty) {
            clone.outbound.sni = Some(
                [
                    "/tls/server_name",
                    "/stream/tlsSettings/serverName",
                    "/stream/realitySettings/serverName",
                    "/streamSettings/tlsSettings/serverName",
                    "/streamSettings/realitySettings/serverName",
                ]
                .iter()
                .filter_map(|path| value.pointer(path).and_then(Value::as_str))
                .find(|s| !s.is_empty())
                .unwrap_or(original)
                .to_owned(),
            );
        }
        if let Some(tls) = value.get_mut("tls").and_then(Value::as_object_mut) {
            if tls
                .get("server_name")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                tls.insert("server_name".into(), original.into());
            }
        }
        if let Some(transport) = value.get_mut("transport").and_then(Value::as_object_mut) {
            if transport.get("type").and_then(Value::as_str) == Some("ws") {
                // sing-box's WebsocketOptions accepts Host only in headers.
                // Migrate a legacy bean host into headers without forwarding
                // that unrecognized key to the core.
                let legacy_host = transport.remove("host");
                let fallback = legacy_host
                    .as_ref()
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or(original);
                let headers = transport.entry("headers").or_insert_with(|| json!({}));
                if let Some(headers) = headers.as_object_mut() {
                    if let Some((_, host)) = headers
                        .iter_mut()
                        .find(|(key, _)| key.eq_ignore_ascii_case("host"))
                    {
                        if json_host_is_empty(host) {
                            *host = fallback.into();
                        }
                    } else {
                        headers.insert("Host".into(), fallback.into());
                    }
                }
            } else if matches!(
                transport.get("type").and_then(Value::as_str),
                Some("httpupgrade" | "http")
            ) && transport.get("host").is_none_or(json_host_is_empty)
            {
                let host = transport
                    .get("headers")
                    .and_then(Value::as_object)
                    .and_then(|headers| {
                        headers
                            .iter()
                            .find(|(key, _)| key.eq_ignore_ascii_case("host"))
                    })
                    .and_then(|(_, value)| value.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(original)
                    .to_owned();
                let value = if transport.get("type").and_then(Value::as_str) == Some("http") {
                    json!([host])
                } else {
                    json!(host)
                };
                transport.insert("host".into(), value);
            }
        }
        if matches!(
            clone.outbound.transport.as_deref(),
            Some("ws" | "httpupgrade" | "http" | "xhttp")
        ) && clone.outbound.host.as_deref().is_none_or(str::is_empty)
        {
            clone.outbound.host = Some(
                value
                    .pointer("/transport/host")
                    .and_then(|host| {
                        host.as_str().or_else(|| {
                            host.as_array()
                                .and_then(|a| a.first())
                                .and_then(Value::as_str)
                        })
                    })
                    .or_else(|| {
                        value
                            .pointer("/transport/headers")
                            .and_then(Value::as_object)
                            .and_then(|headers| {
                                headers
                                    .iter()
                                    .find(|(key, _)| key.eq_ignore_ascii_case("host"))
                            })
                            .and_then(|(_, value)| value.as_str())
                    })
                    .filter(|s| !s.is_empty())
                    .unwrap_or(original)
                    .to_owned(),
            );
        }
        for stream_key in ["stream", "streamSettings", "stream_settings"] {
            if let Some(stream) = value.get_mut(stream_key).and_then(Value::as_object_mut) {
                for tls_key in ["tls", "tlsSettings", "reality", "realitySettings"] {
                    if let Some(tls) = stream.get_mut(tls_key).and_then(Value::as_object_mut) {
                        if tls
                            .get("serverName")
                            .and_then(Value::as_str)
                            .is_none_or(str::is_empty)
                        {
                            tls.insert("serverName".into(), original.into());
                        }
                    }
                }
                for host_key in [
                    "ws",
                    "wsSettings",
                    "httpupgrade",
                    "httpupgradeSettings",
                    "xhttp",
                    "xhttpSettings",
                ] {
                    if let Some(transport) = stream.get_mut(host_key).and_then(Value::as_object_mut)
                    {
                        if transport
                            .get("host")
                            .and_then(Value::as_str)
                            .is_none_or(str::is_empty)
                        {
                            transport.insert("host".into(), original.into());
                        }
                    }
                }
            }
        }
    }
    value["server"] = address.into();
    clone.outbound.server = Some(address.to_owned());
    clone.outbound_json = value.to_string();
    clone.outbound.raw_json = Some(clone.outbound_json.clone());
    Ok(clone)
}

fn json_host_is_empty(value: &Value) -> bool {
    value.is_null()
        || value.as_str().is_some_and(str::is_empty)
        || value.as_array().is_some_and(Vec::is_empty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IpListEntry, IpListRole, ProfileType};

    #[test]
    fn endpoint_json_matches_upstream_and_invalid_sources_inherit() {
        assert_eq!(
            serde_json::to_value(EndpointSource::Inherit).unwrap(),
            json!({})
        );
        assert_eq!(
            serde_json::to_value(EndpointSource::IpList { list: 7 }).unwrap(),
            json!({"mode":"list","list":7})
        );
        for value in [
            json!({"mode":"address","address":" "}),
            json!({"mode":"list","list":-1}),
            json!({"mode":"future"}),
        ] {
            assert_eq!(
                serde_json::from_value::<EndpointSource>(value).unwrap(),
                EndpointSource::Inherit
            );
        }
    }

    #[test]
    fn own_profile_endpoint_overrides_group_and_list_keeps_first_network_address() {
        let mut profile = Profile::new(1, 1, "p", ProfileType::Socks);
        let mut group = Group::new(1, "g");
        group.endpoint = EndpointSource::IpList { list: 7 };
        assert_eq!(
            effective_endpoint_source(&profile, Some(&group)),
            &group.endpoint
        );
        profile.endpoint = EndpointSource::Own;
        assert_eq!(
            effective_endpoint_source(&profile, Some(&group)),
            &EndpointSource::Own
        );
        let mut list = IpList::new("endpoints");
        list.id = 7;
        list.entries.push(IpListEntry {
            cidr: "192.0.2.0/24".into(),
            port: 443,
            latency_ms: 5,
        });
        assert_eq!(
            resolve_endpoint_source(&group.endpoint, Some(&list)).unwrap(),
            Some("192.0.2.0".into())
        );
        list.role = IpListRole::ScanSnapshot;
        assert_eq!(
            resolve_endpoint_source(&group.endpoint, Some(&list)),
            Err(EndpointError::MissingList)
        );
    }

    #[test]
    fn materialized_endpoint_preserves_hostname_port_identity_and_saved_bean() {
        let mut profile = Profile::new(1, 1, "p", ProfileType::Vless);
        profile.endpoint = EndpointSource::Address {
            address: "192.0.2.9".into(),
        };
        profile.outbound_json = json!({"type":"vless","server":"origin.example","server_port":443,"uuid":"credential","tls":{"enabled":true},"transport":{"type":"ws","path":"/"}}).to_string();
        let materialized = materialize_profile_endpoint(&profile, None, None).unwrap();
        let value: Value = serde_json::from_str(&materialized.outbound_json).unwrap();
        assert_eq!(value["server"], "192.0.2.9");
        assert_eq!(value["server_port"], 443);
        assert_eq!(value["tls"]["server_name"], "origin.example");
        assert_eq!(value["transport"]["headers"]["Host"], "origin.example");
        assert!(value["transport"].get("host").is_none());
        assert_eq!(value["uuid"], "credential");
        assert_eq!(materialized.id, profile.id);
        assert!(profile
            .outbound_json
            .contains("\"server\":\"origin.example\""));
        profile.profile_type = ProfileType::Custom;
        assert_eq!(
            materialize_profile_endpoint(&profile, None, None)
                .unwrap()
                .outbound_json,
            profile.outbound_json
        );
    }

    #[test]
    fn endpoint_materialization_preserves_explicit_http_host_and_skips_realm() {
        let mut profile = Profile::new(1, 1, "p", ProfileType::Vless);
        profile.endpoint = EndpointSource::Address {
            address: "192.0.2.9".into(),
        };
        profile.outbound_json = json!({"type":"vless","server":"origin.example","tls":{"server_name":"tls.example"},"transport":{"type":"http","host":["cdn.example"]}}).to_string();
        let materialized = materialize_profile_endpoint(&profile, None, None).unwrap();
        let value: Value = serde_json::from_str(&materialized.outbound_json).unwrap();
        assert_eq!(value["transport"]["host"], json!(["cdn.example"]));
        assert_eq!(materialized.outbound.sni.as_deref(), Some("tls.example"));
        profile.profile_type = ProfileType::Hysteria2;
        profile.outbound_json = json!({"type":"hysteria","protocol_version":"2","realm_enabled":true,"realm_server_url":"https://realm.example"}).to_string();
        assert_eq!(
            materialize_profile_endpoint(&profile, None, None)
                .unwrap()
                .outbound_json,
            profile.outbound_json
        );
    }

    #[test]
    fn openvpn_override_removes_alternate_servers_and_keeps_first_remote_port() {
        let mut profile = Profile::new(1, 1, "vpn", ProfileType::OpenVpn);
        profile.endpoint = EndpointSource::Address {
            address: "192.0.2.9".into(),
        };
        profile.outbound_json = json!({"type":"openvpn","servers":[{"server":"vpn.example","server_port":1194,"network":"udp"}],"remote_random":true}).to_string();
        let clone = materialize_profile_endpoint(&profile, None, None).unwrap();
        let value: Value = serde_json::from_str(&clone.outbound_json).unwrap();
        assert_eq!(value["server"], "192.0.2.9");
        assert_eq!(value["server_port"], 1194);
        assert_eq!(value["network"], "udp");
        assert!(value.get("servers").is_none());
        assert!(value.get("remote_random").is_none());
    }

    #[test]
    fn websocket_endpoint_override_keeps_host_only_in_headers() {
        for transport in [
            json!({"type":"ws","headers":{"host":"cdn.example"}}),
            json!({"type":"ws","host":"cdn.example"}),
        ] {
            let mut profile = Profile::new(1, 1, "ws", ProfileType::Vless);
            profile.endpoint = EndpointSource::Address {
                address: "1.2.3.4".into(),
            };
            profile.outbound.transport = Some("ws".into());
            profile.outbound_json = json!({"type":"vless","server":"origin.example","tls":{"enabled":true},"transport":transport}).to_string();
            let clone = materialize_profile_endpoint(&profile, None, None).unwrap();
            let value: Value = serde_json::from_str(&clone.outbound_json).unwrap();
            assert!(value["transport"].get("host").is_none());
            let headers = value["transport"]["headers"].as_object().unwrap();
            assert_eq!(headers.len(), 1);
            assert_eq!(headers.values().next().unwrap(), "cdn.example");
            assert_eq!(clone.outbound.host.as_deref(), Some("cdn.example"));
        }
    }

    #[test]
    fn unsupported_endpoint_profiles_are_blocked_before_missing_list_resolution() {
        for kind in [
            ProfileType::Chain,
            ProfileType::Custom,
            ProfileType::ExtraCore,
            ProfileType::Tailscale,
            ProfileType::AutoSelector,
            ProfileType::Direct,
        ] {
            let mut profile = Profile::new(1, 1, "serverless", kind);
            profile.endpoint = EndpointSource::IpList { list: 999 };
            assert!(endpoint_override_blocker(&profile).is_some());
            assert!(materialize_profile_endpoint(&profile, None, None).is_ok());
        }
        let mut realm = Profile::new(1, 1, "realm", ProfileType::Hysteria);
        realm.outbound_json =
            json!({"type":"hysteria","protocol_version":"2","realm_enabled":true}).to_string();
        realm.endpoint = EndpointSource::IpList { list: 999 };
        assert!(endpoint_override_blocker(&realm).is_some());
        assert!(materialize_profile_endpoint(&realm, None, None).is_ok());
        let ordinary = Profile::new(1, 1, "ordinary", ProfileType::Vless);
        assert_eq!(endpoint_override_blocker(&ordinary), None);
    }

    #[test]
    fn subscription_refresh_preserves_local_endpoint_and_latency_reset_clears_stamp() {
        let mut state = crate::AppState::empty();
        let gid = state.add_group("subscription");
        state.set_active_group(gid).unwrap();
        let outbound = crate::ParsedOutbound {
            server: Some("origin.example".into()),
            server_port: Some(443),
            ..Default::default()
        };
        state.import_profiles(vec![(
            "before".into(),
            ProfileType::Vless,
            outbound.clone(),
            false,
        )]);
        let pid = state.all_profiles()[0].id;
        state
            .set_profile_endpoint(pid, EndpointSource::IpList { list: 4 })
            .unwrap();
        state.set_profile_latency(pid, 21);
        assert!(state.profile(pid).unwrap().latency_at > 0);
        state
            .apply_subscription_snapshot(
                gid,
                vec![("after".into(), ProfileType::Vless, outbound, false)],
                String::new(),
                123,
            )
            .unwrap();
        assert_eq!(
            state.profile(pid).unwrap().endpoint,
            EndpointSource::IpList { list: 4 }
        );
        state.set_profile_latency(pid, 0);
        assert_eq!(state.profile(pid).unwrap().latency_at, 0);
    }
}
