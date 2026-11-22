//! Profile share links for the node menu.
//!
//! `Protocol` is the ordinary URI (`vless://`, `trojan://`, …) when the stored
//! bean has enough fields. `Throne` is always `throne://add/` of that bean, the
//! upstream deep link (`ExportJsonLink`).

use base64::Engine;
use serde_json::{json, Value};

/// Which link the Share menu copies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShareFormat {
    /// Protocol URI, or the Throne link when no URI can be built.
    Protocol,
    /// `throne://add/<base64url>` of the stored outbound bean.
    Throne,
}

/// One share line for a stored profile.
pub fn share_link(name: &str, outbound_json: &str, format: ShareFormat) -> String {
    let bean = bean_json(name, outbound_json);
    match format {
        ShareFormat::Throne => throne_add_link(&bean),
        ShareFormat::Protocol => {
            protocol_link(name, &bean).unwrap_or_else(|| throne_add_link(&bean))
        }
    }
}

fn bean_json(name: &str, outbound_json: &str) -> Value {
    let mut bean = serde_json::from_str::<Value>(outbound_json).unwrap_or_else(|_| json!({}));
    if let Some(obj) = bean.as_object_mut() {
        if !name.trim().is_empty() {
            obj.entry("tag").or_insert_with(|| json!(name));
        }
    }
    bean
}

fn throne_add_link(bean: &Value) -> String {
    let compact = serde_json::to_string(bean).unwrap_or_else(|_| "{}".into());
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compact.as_bytes());
    format!("throne://add/{b64}")
}

fn protocol_link(name: &str, bean: &Value) -> Option<String> {
    let kind = text(bean, "type").or_else(|| text(bean, "protocol"))?;
    let server = text(bean, "server")?;
    let port = bean.get("server_port").and_then(Value::as_u64)? as u16;
    if server.is_empty() || port == 0 {
        return None;
    }
    match kind.to_ascii_lowercase().as_str() {
        "vless" => Some(vless_link(name, bean, &server, port)),
        "trojan" => {
            let password = text(bean, "password")?;
            Some(user_link(
                "trojan",
                &password,
                &server,
                port,
                &tls_query(bean),
                name,
            ))
        }
        "shadowsocks" | "ss" => ss_link(name, bean, &server, port),
        "socks" | "socks5" => {
            let user = text(bean, "username").unwrap_or_default();
            let pass = text(bean, "password").unwrap_or_default();
            let userinfo = if pass.is_empty() {
                user.to_string()
            } else {
                format!("{user}:{pass}")
            };
            (!userinfo.is_empty()).then(|| user_link("socks", &userinfo, &server, port, "", name))
        }
        "http" => {
            let user = text(bean, "username").unwrap_or_default();
            let pass = text(bean, "password").unwrap_or_default();
            if user.is_empty() {
                return None;
            }
            let userinfo = if pass.is_empty() {
                user.to_string()
            } else {
                format!("{user}:{pass}")
            };
            Some(user_link("http", &userinfo, &server, port, "", name))
        }
        "hysteria2" | "hy2" => {
            let password = text(bean, "password").unwrap_or_default();
            let mut query = tls_query(bean);
            if let Some(obfs) = bean.get("obfs") {
                let kind = text(obfs, "type").unwrap_or("salamander");
                push_query(&mut query, "obfs", kind);
                if let Some(password) = text(obfs, "password") {
                    push_query(&mut query, "obfs-password", password);
                }
            }
            Some(user_link(
                "hysteria2",
                &password,
                &server,
                port,
                &query,
                name,
            ))
        }
        _ => None,
    }
}

fn vless_link(name: &str, bean: &Value, server: &str, port: u16) -> String {
    let uuid = text(bean, "uuid").unwrap_or_default();
    let mut query = String::from("encryption=none");
    if let Some(flow) = text(bean, "flow") {
        push_query(&mut query, "flow", flow);
    }
    if let Some(encoding) = text(bean, "packet_encoding") {
        push_query(&mut query, "packetEncoding", encoding);
    }
    append_transport(&mut query, bean);
    let tls = tls_query(bean);
    if !tls.is_empty() {
        if !query.is_empty() {
            query.push('&');
        }
        query.push_str(&tls);
    }
    user_link("vless", &uuid, server, port, &query, name)
}

fn ss_link(name: &str, bean: &Value, server: &str, port: u16) -> Option<String> {
    let method = text(bean, "method")?;
    let password = text(bean, "password").unwrap_or_default();
    let userinfo = format!("{method}:{password}");
    Some(user_link("ss", &userinfo, server, port, "", name))
}

fn tls_query(bean: &Value) -> String {
    let Some(tls) = bean.get("tls").filter(|tls| tls["enabled"] != false) else {
        return String::new();
    };
    if tls.as_object().is_none() {
        return String::new();
    }
    let reality = tls
        .get("reality")
        .is_some_and(|reality| reality["enabled"] == true || text(reality, "public_key").is_some());
    let mut query = String::new();
    push_query(
        &mut query,
        "security",
        if reality { "reality" } else { "tls" },
    );
    if let Some(sni) = text(tls, "server_name") {
        push_query(&mut query, "sni", sni);
    }
    if let Some(fp) = text(&tls["utls"], "fingerprint") {
        push_query(&mut query, "fp", fp);
    }
    if reality {
        if let Some(pbk) = text(&tls["reality"], "public_key") {
            push_query(&mut query, "pbk", pbk);
        }
        if let Some(sid) = text(&tls["reality"], "short_id") {
            push_query(&mut query, "sid", sid);
        }
    }
    query
}

fn append_transport(query: &mut String, bean: &Value) {
    let Some(transport) = bean.get("transport") else {
        return;
    };
    let Some(kind) = text(transport, "type") else {
        return;
    };
    push_query(query, "type", kind);
    if let Some(path) = text(transport, "path") {
        push_query(query, "path", path);
    }
    if let Some(host) = text(&transport["headers"], "Host") {
        push_query(query, "host", host);
    }
}

fn user_link(scheme: &str, user: &str, host: &str, port: u16, query: &str, name: &str) -> String {
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    let mut link = format!("{scheme}://{}@{host}:{port}", urlencoding::encode(user));
    if !query.is_empty() {
        link.push('?');
        link.push_str(query);
    }
    if !name.trim().is_empty() {
        link.push('#');
        link.push_str(&urlencoding::encode(name));
    }
    link
}

fn push_query(query: &mut String, key: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    if !query.is_empty() {
        query.push('&');
    }
    query.push_str(&urlencoding::encode(key));
    query.push('=');
    query.push_str(&urlencoding::encode(value));
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import_text;

    #[test]
    fn vless_share_round_trips() {
        let json = r#"{"type":"vless","server":"edge.example","server_port":443,"uuid":"11111111-1111-1111-1111-111111111111","tls":{"enabled":true,"server_name":"edge.example","utls":{"fingerprint":"chrome"}}}"#;
        let link = share_link("Node", json, ShareFormat::Protocol);
        assert!(link.starts_with("vless://"), "{link}");
        let imported = import_text(&link);
        assert_eq!(imported.ok_count(), 1, "{:?}", imported.errors);
        assert_eq!(imported.profiles[0].name, "Node");
        assert_eq!(
            imported.profiles[0].outbound.server.as_deref(),
            Some("edge.example")
        );
    }

    #[test]
    fn throne_share_round_trips_the_bean() {
        let json =
            r#"{"type":"trojan","server":"t.example","server_port":443,"password":"secret"}"#;
        let link = share_link("T", json, ShareFormat::Throne);
        assert!(link.starts_with("throne://add/"), "{link}");
        let imported = import_text(&link);
        assert_eq!(imported.ok_count(), 1, "{:?}", imported.errors);
        assert_eq!(
            imported.profiles[0].outbound.password.as_deref(),
            Some("secret")
        );
    }

    #[test]
    fn unknown_bean_falls_back_to_the_throne_link() {
        let json = r#"{"type":"wireguard","server":"10.0.0.1","server_port":51820}"#;
        let link = share_link("wg", json, ShareFormat::Protocol);
        assert!(link.starts_with("throne://add/"), "{link}");
    }
}
