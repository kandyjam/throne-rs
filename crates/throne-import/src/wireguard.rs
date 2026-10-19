//! WireGuard/Amnezia fields shared by URI and INI import paths.
use crate::ImportedProfile;
use serde_json::{json, Value};

pub(crate) fn apply_options(profile: &mut ImportedProfile, options: &[(String, String)]) {
    let get = |key: &str| {
        options
            .iter()
            .rev()
            .find(|(k, _)| {
                k.replace('_', "")
                    .eq_ignore_ascii_case(&key.replace('_', ""))
            })
            .map(|(_, v)| v.as_str())
    };
    let o = &profile.outbound;
    let addresses: Vec<String> = o
        .path
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let s = s.trim();
            if s.contains('/') {
                s.to_string()
            } else {
                format!("{s}/{}", if s.contains(':') { 128 } else { 32 })
            }
        })
        .collect();
    let mut peer =
        json!({"address":o.server,"port":o.server_port.unwrap_or(51820),"public_key":o.username});
    if let Some(psk) = o
        .method
        .as_deref()
        .or_else(|| get("pre_shared_key"))
        .or_else(|| get("presharedkey"))
        .or_else(|| get("psk"))
    {
        peer["pre_shared_key"] = json!(psk);
    }
    if let Some(value) = get("persistent_keepalive_interval")
        .or_else(|| get("persistent_keepalive"))
        .or_else(|| get("keepalive"))
    {
        if let Ok(seconds) = value.parse::<u32>() {
            if seconds > 0 {
                peer["persistent_keepalive_interval"] = json!(seconds);
            }
        } else if value
            .split_once('-')
            .is_some_and(|(a, b)| a.parse::<u32>().is_ok() && b.parse::<u32>().is_ok())
        {
            peer["persistent_keepalive_interval"] = json!(value);
        }
    }
    if let Some(value) = get("reserved") {
        peer["reserved"] = json!(value
            .split([',', '-'])
            .filter_map(|s| s.parse::<u8>().ok())
            .collect::<Vec<_>>());
    }
    let mut raw = json!({"type":"wireguard","tag":profile.name,"private_key":o.password,"address":addresses,"peers":[peer]});
    for (source, key) in [("mtu", "mtu"), ("workers", "worker_count")] {
        if let Some(value) = get(source)
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|v| *v > 0)
        {
            raw[key] = json!(value);
        }
    }
    if get("use_system_interface") == Some("true") {
        raw["system"] = json!(true);
    }
    if let Some(value) = get("udp_timeout") {
        raw["udp_timeout"] = json!(value);
    }
    let mut amnezia = serde_json::Map::new();
    for key in ["jc", "jmin", "jmax", "s1", "s2", "s3", "s4"] {
        if let Some(value) = get(key)
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|v| *v > 0)
        {
            amnezia.insert(key.into(), json!(value));
        }
    }
    for key in [
        "h1",
        "h2",
        "h3",
        "h4",
        "i1",
        "i2",
        "i3",
        "i4",
        "i5",
        "header_protection_key",
        "content_padding_addition",
        "rekey_after_time",
        "rekey_timeout",
        "reject_after_time",
        "keepalive_timeout",
        "max_handshake_attempts",
    ] {
        if let Some(value) = get(key).filter(|s| !s.is_empty()) {
            amnezia.insert(key.into(), json!(value));
        }
    }
    for key in ["random_trailers", "disable_cookies"] {
        if get(key)
            .is_some_and(|s| matches!(s.to_ascii_lowercase().as_str(), "true" | "1" | "on" | "yes"))
        {
            amnezia.insert(key.into(), json!(true));
        }
    }
    if !amnezia.is_empty() {
        raw["amnezia_wg"] = Value::Object(amnezia);
    }
    profile.outbound.raw_json = Some(raw.to_string());
}
