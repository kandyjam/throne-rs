//! JSON subscription formats from upstream `RawUpdater`:
//! sing-box outbounds, Xray protocol objects, SIP008, custom full config.

use serde_json::Value;
use throne_domain::{ParsedOutbound, ProfileType};

use crate::ImportedProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SingBoxKind {
    FullOrOutboundsObject,
    OutboundObject,
    OutboundArray,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum XrayKind {
    OutboundObject,
    OutboundsWrapper,
    OutboundArray,
    ConfigArray,
}

pub fn try_import_json(text: &str) -> Option<Vec<ImportedProfile>> {
    let doc: Value = serde_json::from_str(text.trim()).ok()?;

    // Xray first (protocol tag) — mirrors getXraySubType priority.
    if let Some(kind) = xray_kind(&doc) {
        return Some(import_xray(&doc, kind, text));
    }

    if let Some(kind) = singbox_kind(&doc) {
        return Some(import_singbox(&doc, kind, text));
    }

    // SIP008: { "version": …, "servers": [ … ] }
    if doc.get("version").is_some() && doc.get("servers").and_then(|s| s.as_array()).is_some() {
        return Some(import_sip008(&doc));
    }

    None
}

fn singbox_kind(doc: &Value) -> Option<SingBoxKind> {
    if let Some(obj) = doc.as_object() {
        if obj.contains_key("outbounds") || obj.contains_key("endpoints") {
            return Some(SingBoxKind::FullOrOutboundsObject);
        }
        if obj.contains_key("type") {
            return Some(SingBoxKind::OutboundObject);
        }
        return None;
    }
    if let Some(arr) = doc.as_array() {
        if let Some(first) = arr.first().and_then(|v| v.as_object()) {
            if first.contains_key("type") {
                return Some(SingBoxKind::OutboundArray);
            }
        }
    }
    None
}

fn xray_kind(doc: &Value) -> Option<XrayKind> {
    if let Some(obj) = doc.as_object() {
        if let Some(arr) = obj.get("outbounds").and_then(|v| v.as_array()) {
            if arr
                .iter()
                .any(|i| i.as_object().is_some_and(|o| o.contains_key("protocol")))
            {
                return Some(XrayKind::OutboundsWrapper);
            }
        }
        if obj.contains_key("protocol") {
            return Some(XrayKind::OutboundObject);
        }
        return None;
    }
    if let Some(arr) = doc.as_array() {
        if let Some(first) = arr.first().and_then(|v| v.as_object()) {
            if first.contains_key("protocol") {
                return Some(XrayKind::OutboundArray);
            }
            if let Some(outs) = first.get("outbounds").and_then(|v| v.as_array()) {
                if outs
                    .iter()
                    .any(|i| i.as_object().is_some_and(|o| o.contains_key("protocol")))
                {
                    return Some(XrayKind::ConfigArray);
                }
            }
        }
    }
    None
}

fn import_singbox(doc: &Value, kind: SingBoxKind, raw: &str) -> Vec<ImportedProfile> {
    match kind {
        SingBoxKind::FullOrOutboundsObject => {
            // Prefer expanding outbounds; keep non-proxy tags skipped.
            if let Some(arr) = doc
                .get("outbounds")
                .or_else(|| doc.get("endpoints"))
                .and_then(|v| v.as_array())
            {
                let mut out = Vec::new();
                for item in arr {
                    if let Some(p) = outbound_from_singbox(item) {
                        out.push(p);
                    }
                }
                if !out.is_empty() {
                    return out;
                }
            }
            // Full custom config fallback
            vec![custom_profile("Sing-box config", raw, true)]
        }
        SingBoxKind::OutboundObject => outbound_from_singbox(doc).into_iter().collect(),
        SingBoxKind::OutboundArray => doc
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(outbound_from_singbox)
            .collect(),
    }
}

fn import_xray(doc: &Value, kind: XrayKind, _raw: &str) -> Vec<ImportedProfile> {
    match kind {
        XrayKind::OutboundObject => xray_outbound(doc).into_iter().collect(),
        XrayKind::OutboundsWrapper => doc
            .get("outbounds")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(xray_outbound)
            .collect(),
        XrayKind::OutboundArray => doc
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(xray_outbound)
            .collect(),
        XrayKind::ConfigArray => {
            // Each element is a full Xray config → store as custom xray full config
            // (upstream subtype `xrayfullconfig`, eligible for Auto Selector since 1.2.4).
            doc.as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .filter_map(|(i, cfg)| {
                    let name = cfg
                        .get("remarks")
                        .or_else(|| cfg.get("ps"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("Xray config {}", i + 1));
                    let text = serde_json::to_string(cfg).ok()?;
                    Some(custom_xray_full_profile(&name, &text))
                })
                .collect()
        }
    }
}

fn import_sip008(doc: &Value) -> Vec<ImportedProfile> {
    let mut out = Vec::new();
    let Some(servers) = doc.get("servers").and_then(|v| v.as_array()) else {
        return out;
    };
    for s in servers {
        let Some(obj) = s.as_object() else { continue };
        let server = obj
            .get("server")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let port = obj
            .get("server_port")
            .or_else(|| obj.get("port"))
            .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
            .unwrap_or(8388) as u16;
        let method = obj
            .get("method")
            .or_else(|| obj.get("cipher"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let password = obj
            .get("password")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let name = obj
            .get("remarks")
            .or_else(|| obj.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("SIP008")
            .to_string();
        if server.is_empty() || method.is_empty() || password.is_empty() {
            continue;
        }
        let mut outbound = ParsedOutbound {
            server: Some(server),
            server_port: Some(port),
            method: Some(method),
            password: Some(password),
            plugin: obj
                .get("plugin")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            plugin_opts: obj
                .get("plugin_opts")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            tag: Some(name.clone()),
            ..Default::default()
        };
        outbound.raw_json = Some(outbound.to_db_json(ProfileType::Shadowsocks));
        out.push(ImportedProfile {
            name,
            profile_type: ProfileType::Shadowsocks,
            outbound,
            source: "sip008".into(),
        });
    }
    out
}

fn outbound_from_singbox(v: &Value) -> Option<ImportedProfile> {
    let obj = v.as_object()?;
    let ty = obj.get("type")?.as_str()?;
    // Skip helper outbounds
    if matches!(
        ty,
        "direct" | "block" | "dns" | "selector" | "urltest" | "pass"
    ) {
        return None;
    }
    let profile_type = ProfileType::from_upstream(ty).unwrap_or(ProfileType::Custom);
    let name = obj
        .get("tag")
        .and_then(|t| t.as_str())
        .unwrap_or(ty)
        .to_string();

    if profile_type == ProfileType::Custom || ty == "custom" {
        let raw = serde_json::to_string(v).ok()?;
        return Some(custom_profile(&name, &raw, false));
    }

    let mut outbound = ParsedOutbound {
        tag: Some(name.clone()),
        server: obj
            .get("server")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        server_port: obj
            .get("server_port")
            .and_then(|x| x.as_u64().map(|n| n as u16)),
        uuid: obj
            .get("uuid")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        password: obj
            .get("password")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        username: obj
            .get("username")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        method: obj
            .get("method")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        flow: obj
            .get("flow")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        ..Default::default()
    };
    if let Some(tls) = obj.get("tls").and_then(|t| t.as_object()) {
        outbound.tls = tls.get("enabled").and_then(|v| v.as_bool()).or(Some(true));
        outbound.sni = tls
            .get("server_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        outbound.insecure = tls.get("insecure").and_then(|v| v.as_bool());
    }
    if let Some(tr) = obj.get("transport").and_then(|t| t.as_object()) {
        outbound.transport = tr
            .get("type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        outbound.path = tr
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        outbound.host = tr
            .get("headers")
            .and_then(|h| h.get("Host"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }
    // Prefer original JSON for DB fidelity
    outbound.raw_json = Some(serde_json::to_string(v).unwrap_or_else(|_| "{}".into()));

    Some(ImportedProfile {
        name,
        profile_type,
        outbound,
        source: "sing-box-json".into(),
    })
}

fn xray_outbound(v: &Value) -> Option<ImportedProfile> {
    let obj = v.as_object()?;
    let protocol = obj.get("protocol")?.as_str()?;
    if matches!(protocol, "freedom" | "blackhole" | "dns" | "dokodemo-door") {
        return None;
    }
    let name = obj
        .get("tag")
        .and_then(|t| t.as_str())
        .unwrap_or(protocol)
        .to_string();

    // Prefer simplified extraction for vless/vmess/trojan/shadowsocks
    let mut outbound = ParsedOutbound {
        tag: Some(name.clone()),
        ..Default::default()
    };

    if let Some(settings) = obj.get("settings") {
        // vnext style
        if let Some(vnext) = settings
            .get("vnext")
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
        {
            outbound.server = vnext
                .get("address")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            outbound.server_port = vnext.get("port").and_then(|x| x.as_u64().map(|n| n as u16));
            if let Some(user) = vnext
                .get("users")
                .and_then(|a| a.as_array())
                .and_then(|a| a.first())
            {
                outbound.uuid = user
                    .get("id")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string());
                outbound.flow = user
                    .get("flow")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string());
                outbound.security = user
                    .get("security")
                    .or_else(|| user.get("encryption"))
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string());
            }
        }
        // servers style (ss/trojan)
        if let Some(server) = settings
            .get("servers")
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
        {
            outbound.server = server
                .get("address")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            outbound.server_port = server
                .get("port")
                .and_then(|x| x.as_u64().map(|n| n as u16));
            outbound.password = server
                .get("password")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            outbound.method = server
                .get("method")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
        }
    }

    if let Some(stream) = obj.get("streamSettings").and_then(|s| s.as_object()) {
        outbound.transport = stream
            .get("network")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        let security = stream
            .get("security")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        outbound.tls = Some(security == "tls" || security == "reality");
        outbound.security = Some(security.to_string());
        if let Some(tls) = stream
            .get("tlsSettings")
            .or_else(|| stream.get("realitySettings"))
            .and_then(|t| t.as_object())
        {
            outbound.sni = tls
                .get("serverName")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            outbound.fp = tls
                .get("fingerprint")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            outbound.pbk = tls
                .get("publicKey")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            outbound.sid = tls
                .get("shortId")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
        }
    }

    outbound.raw_json = Some(serde_json::to_string(v).unwrap_or_else(|_| "{}".into()));

    let profile_type = match protocol {
        "vmess" => ProfileType::Vmess,
        "vless" => ProfileType::XrayVless,
        "trojan" => ProfileType::Trojan,
        "shadowsocks" => ProfileType::Shadowsocks,
        "socks" => ProfileType::Socks,
        "http" => ProfileType::Http,
        _ => ProfileType::Custom,
    };

    Some(ImportedProfile {
        name,
        profile_type,
        outbound,
        source: format!("xray:{protocol}"),
    })
}

fn custom_profile(name: &str, config: &str, full: bool) -> ImportedProfile {
    // Wire-compatible custom bean (upstream ExportToJson).
    let subtype = if full { "fullconfig" } else { "outbound" };
    let outbound_json = serde_json::json!({
        "type": "custom",
        "name": name,
        "subtype": subtype,
        "config": config,
    });
    ImportedProfile {
        name: name.to_string(),
        profile_type: ProfileType::Custom,
        outbound: ParsedOutbound {
            tag: Some(name.to_string()),
            security: if full {
                Some("full-config".into())
            } else {
                None
            },
            raw_json: Some(
                serde_json::to_string(&outbound_json).unwrap_or_else(|_| config.to_string()),
            ),
            ..Default::default()
        },
        source: if full {
            "custom-full".into()
        } else {
            "custom-outbound".into()
        },
    }
}

/// Upstream `CustomXrayFullConfig` (`subtype = xrayfullconfig`).
fn custom_xray_full_profile(name: &str, config: &str) -> ImportedProfile {
    let outbound_json = serde_json::json!({
        "type": "custom",
        "name": name,
        "subtype": "xrayfullconfig",
        "config": config,
    });
    ImportedProfile {
        name: name.to_string(),
        profile_type: ProfileType::Custom,
        outbound: ParsedOutbound {
            tag: Some(name.to_string()),
            security: Some("xray-full-config".into()),
            raw_json: Some(
                serde_json::to_string(&outbound_json).unwrap_or_else(|_| config.to_string()),
            ),
            ..Default::default()
        },
        source: "custom-xray-full".into(),
    }
}
