//! Lossless, JSON-backed profile editing. Forms patch changed fields only.

use serde_json::{Map, Value};

use crate::{ParsedOutbound, Profile, ProfileType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileEditSection {
    General,
    Transport,
    Tls,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileEditKind {
    Text,
    Secret,
    Boolean,
    Number,
    StringList,
}

#[derive(Clone, Debug)]
pub struct ProfileEditField {
    pub key: String,
    pub label: &'static str,
    pub section: ProfileEditSection,
    pub kind: ProfileEditKind,
    pub value: String,
}

/// Owns the original profile so cancelling or failed validation cannot mutate it.
#[derive(Clone, Debug)]
pub struct ProfileEditDraft {
    pub name: String,
    pub json: String,
    pub fields: Vec<ProfileEditField>,
    original: Profile,
    baseline: Value,
    baseline_fields: Vec<ProfileEditField>,
}

impl ProfileEditDraft {
    pub fn from_profile(profile: &Profile) -> Result<Self, String> {
        // Never replace malformed user JSON with a synthesized default.
        let raw = if profile.outbound_json.trim().is_empty() {
            profile.export_outbound_json()
        } else {
            let value = parse_object(&profile.outbound_json)?;
            if value.get("raw_json").is_some() {
                profile.export_outbound_json()
            } else {
                profile.outbound_json.clone()
            }
        };
        let value = parse_object(&raw)?;
        Self::from_value(profile.clone(), profile.name.clone(), value)
    }

    fn from_value(original: Profile, name: String, value: Value) -> Result<Self, String> {
        let fields = fields_for(original.profile_type, &value);
        Ok(Self {
            name,
            json: serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?,
            baseline_fields: fields.clone(),
            fields,
            original,
            baseline: value,
        })
    }

    pub fn is_structured(&self) -> bool {
        !self.fields.is_empty()
    }

    /// Rebase form fields after editing raw JSON. No form values are overlaid.
    /// Identity is checked here; semantic validation happens on build/save.
    pub fn with_json(&self, json: String) -> Result<Self, String> {
        let value = parse_object(&json)?;
        validate_identity(&self.baseline, &value)?;
        Self::from_value(self.original.clone(), self.name.clone(), value)
    }

    /// Detect subscription/import edits made while this draft was open.
    /// Traffic, latency and other live metadata do not make the draft stale.
    pub fn matches_source(&self, current: &Profile) -> bool {
        self.original.id == current.id
            && self.original.name == current.name
            && self.original.group_id == current.group_id
            && self.original.profile_type == current.profile_type
            && Self::from_profile(&self.original)
                .ok()
                .zip(Self::from_profile(current).ok())
                .is_some_and(|(original, current)| original.baseline == current.baseline)
    }

    pub fn build(&self) -> Result<Profile, String> {
        let profile = self.serialize_draft()?;
        if profile.name.is_empty() {
            return Err("Profile name cannot be empty".into());
        }
        let value = parse_object(&profile.outbound_json)?;
        validate_native(self.original.profile_type, &value)?;
        Ok(profile)
    }

    /// Serialize representable edits for switching tabs, even when the draft
    /// still needs semantic repairs. This must not be used for persistence.
    pub fn serialize_draft(&self) -> Result<Profile, String> {
        let name = self.name.trim();
        let mut value = parse_object(&self.json)?;
        validate_identity(&self.baseline, &value)?;
        // Input entities retain their opening kinds until a tab rebase. Resolve
        // the final network first so an edited Host uses the new schema even
        // when the user changes both fields in one form session.
        let network =
            self.fields
                .iter()
                .find(|field| {
                    field.key == "/transport/type"
                        && self.baseline_fields.iter().any(|baseline| {
                            baseline.key == field.key && baseline.value != field.value
                        })
                })
                .map(|field| field.value.trim())
                .unwrap_or_else(|| {
                    value
                        .pointer("/transport/type")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                })
                .to_owned();
        for field in &self.fields {
            let baseline = self
                .baseline_fields
                .iter()
                .find(|b| b.key == field.key)
                .ok_or_else(|| "Unknown editor field".to_string())?;
            if field.kind != baseline.kind || field.value == baseline.value {
                continue;
            }
            if value.pointer(&field.key) != self.baseline.pointer(&field.key) {
                return Err(format!(
                    "{} changed in both the form and JSON; switch tabs to synchronize",
                    field.label
                ));
            }
            let replacement = if field.key == "/transport/host"
                && matches!(network.as_str(), "http" | "httpupgrade")
            {
                let mut host = field.clone();
                host.kind = if network == "http" {
                    ProfileEditKind::StringList
                } else {
                    ProfileEditKind::Text
                };
                field_value(&host)?
            } else {
                field_value(field)?
            };
            set_pointer(&mut value, &field.key, replacement)?;
        }
        // Plain TCP has no sing-box transport object. Only normalize on an
        // explicit type edit, not while merely opening or renaming a profile.
        let transport_changed = self.fields.iter().any(|f| {
            f.key == "/transport/type"
                && self
                    .baseline_fields
                    .iter()
                    .any(|b| b.key == f.key && b.value != f.value)
        });
        if transport_changed {
            migrate_transport(&mut value);
        }
        let json = serde_json::to_string(&value).map_err(|e| e.to_string())?;
        let mut profile = self.original.clone();
        profile.name = name.to_owned();
        profile.outbound = hydrate_outbound(&value, &json);
        profile.outbound_json = json;
        profile.insecure = profile.outbound.insecure.unwrap_or(false);
        Ok(profile)
    }
}

fn parse_object(raw: &str) -> Result<Value, String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|e| format!("Invalid outbound JSON: {e}"))?;
    if !value.is_object() {
        return Err("Outbound JSON must be an object".into());
    }
    Ok(value)
}

fn validate_identity(original: &Value, value: &Value) -> Result<(), String> {
    for key in ["type", "protocol", "subtype"] {
        if original.get(key) != value.get(key) {
            return Err(format!("Profile {key} cannot be changed in the editor"));
        }
    }
    Ok(())
}

fn native_protocol(ty: ProfileType, v: &Value) -> bool {
    use ProfileType::*;
    let supported = matches!(
        ty,
        Socks
            | Http
            | Shadowsocks
            | Vmess
            | Vless
            | Trojan
            | Hysteria
            | Hysteria2
            | Tuic
            | AnyTls
            | Naive
            | ShadowTls
            | Ssh
    );
    supported
        && v.get("type").and_then(Value::as_str).is_some_and(|t| {
            ProfileType::from_upstream(t) == Some(ty) || (ty == Hysteria && t == "hysteria2")
        })
        && !["clash", "subtype", "outbounds", "protocol"]
            .iter()
            .any(|key| v.get(key).is_some())
        && v.get("realm_enabled").and_then(Value::as_bool) != Some(true)
}

fn native_type(ty: ProfileType, v: &Value) -> bool {
    native_protocol(ty, v)
        && ["tls", "transport"]
            .iter()
            .all(|key| v.get(key).is_none_or(Value::is_object))
}

fn fields_for(ty: ProfileType, value: &Value) -> Vec<ProfileEditField> {
    use ProfileEditKind::*;
    use ProfileEditSection::*;
    use ProfileType::*;
    if !native_type(ty, value) {
        return Vec::new();
    }
    let mut fields = Vec::new();
    let mut add = |key: &str, label, section, kind| {
        // Unexpected field shapes remain available in raw JSON, never flattened.
        let v = value.pointer(key);
        let compatible = match (kind, v) {
            (_, None) => true,
            (Text | Secret, Some(v)) => v.is_string(),
            (Boolean, Some(v)) => v.is_boolean(),
            (Number, Some(v)) => v.is_u64(),
            (StringList, Some(v)) => {
                v.is_string() || v.as_array().is_some_and(|a| a.iter().all(Value::is_string))
            }
        };
        if !compatible {
            return;
        }
        let display = match (kind, v) {
            (Boolean, None) => (key == "/tls/enabled" && value.get("tls").is_some()).to_string(),
            (_, None) => String::new(),
            (Boolean | Number, Some(v)) => v.to_string(),
            (StringList, Some(Value::Array(a))) => a
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
            (_, Some(v)) => v.as_str().unwrap_or_default().to_owned(),
        };
        fields.push(ProfileEditField {
            key: key.into(),
            label,
            section,
            kind,
            value: display,
        });
    };
    add("/server", "Server", General, Text);
    add("/server_port", "Port", General, Number);
    if matches!(ty, Vless | Vmess | Tuic) {
        add("/uuid", "UUID", General, Secret);
    }
    if matches!(ty, Socks | Http | Naive | Ssh) {
        add("/username", "Username", General, Text);
    }
    if matches!(
        ty,
        Socks | Http | Shadowsocks | Trojan | Hysteria2 | Tuic | AnyTls | Naive | ShadowTls | Ssh
    ) || (ty == Hysteria && value["type"] == "hysteria2")
    {
        add("/password", "Password", General, Secret);
    }
    if ty == Hysteria && value["type"] == "hysteria" {
        add("/auth_str", "Authentication", General, Secret);
    }
    if ty == Shadowsocks {
        add("/method", "Cipher", General, Text);
        add("/plugin", "Plugin", General, Text);
        add("/plugin_opts", "Plugin options", General, Text);
    }
    if ty == Vless {
        add("/flow", "Flow", General, Text);
        add("/packet_encoding", "Packet encoding", General, Text);
    }
    if ty == Vmess {
        add("/security", "Encryption", General, Text);
        add("/alter_id", "Alter ID", General, Number);
        add("/packet_encoding", "Packet encoding", General, Text);
    }
    if matches!(ty, Hysteria | Hysteria2) {
        add("/up_mbps", "Upload Mbps", General, Number);
        add("/down_mbps", "Download Mbps", General, Number);
    }
    if ty == Hysteria2 || (ty == Hysteria && value["type"] == "hysteria2") {
        add("/obfs/type", "Obfuscation", General, Text);
        add("/obfs/password", "Obfuscation password", General, Secret);
    }
    if ty == Tuic {
        add("/congestion_control", "Congestion control", General, Text);
        add("/udp_relay_mode", "UDP relay mode", General, Text);
    }
    if matches!(ty, Vless | Vmess | Trojan) {
        add("/transport/type", "Network (blank = TCP)", Transport, Text);
        add("/transport/path", "Path", Transport, Text);
        add("/transport/headers/Host", "WebSocket Host", Transport, Text);
        if value.pointer("/transport/type").and_then(Value::as_str) == Some("http") {
            add("/transport/host", "HTTP hosts", Transport, StringList);
        } else {
            add("/transport/host", "HTTPUpgrade host", Transport, Text);
        }
        add("/transport/service_name", "gRPC service", Transport, Text);
    }
    if !matches!(ty, Socks | Shadowsocks | Ssh) {
        add("/tls/enabled", "Enable TLS", Tls, Boolean);
        add("/tls/server_name", "SNI", Tls, Text);
        add("/tls/alpn", "ALPN", Tls, StringList);
        add("/tls/insecure", "Allow insecure TLS", Tls, Boolean);
        add("/tls/utls/enabled", "Enable uTLS", Tls, Boolean);
        add("/tls/utls/fingerprint", "Fingerprint", Tls, Text);
        add("/tls/reality/enabled", "Enable REALITY", Tls, Boolean);
        add("/tls/reality/public_key", "REALITY public key", Tls, Text);
        add("/tls/reality/short_id", "REALITY short ID", Tls, Text);
    }
    fields
}

fn field_value(field: &ProfileEditField) -> Result<Option<Value>, String> {
    use ProfileEditKind::*;
    let text = if field.kind == Secret {
        field.value.as_str()
    } else {
        field.value.trim()
    };
    if text.is_empty() {
        return Ok(None);
    }
    let bad = || format!("Invalid {}", field.label);
    Ok(Some(match field.kind {
        Text | Secret => Value::String(text.to_owned()),
        Boolean => Value::Bool(text.parse::<bool>().map_err(|_| bad())?),
        Number => {
            let n = text.parse::<u64>().map_err(|_| bad())?;
            Value::from(n)
        }
        StringList => Value::Array(
            text.split([',', '\n'])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| Value::String(s.into()))
                .collect(),
        ),
    }))
}

fn set_pointer(value: &mut Value, pointer: &str, replacement: Option<Value>) -> Result<(), String> {
    let parts: Vec<_> = pointer.trim_start_matches('/').split('/').collect();
    let mut parent = value;
    for part in &parts[..parts.len() - 1] {
        let object = parent
            .as_object_mut()
            .ok_or_else(|| format!("{pointer} has a non-object parent; edit it in JSON"))?;
        if replacement.is_none() && !object.contains_key(*part) {
            return Ok(());
        }
        parent = object
            .entry((*part).to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    let object = parent
        .as_object_mut()
        .ok_or_else(|| format!("{pointer} has a non-object parent; edit it in JSON"))?;
    let key = parts.last().unwrap();
    if let Some(replacement) = replacement {
        object.insert((*key).into(), replacement);
    } else {
        object.remove(*key);
    }
    Ok(())
}

// Field names match the pinned core's option/v2ray_transport.go.
const TRANSPORT_KEYS: &[&str] = &[
    "host",
    "path",
    "method",
    "headers",
    "idle_timeout",
    "ping_timeout",
    "max_early_data",
    "early_data_header_name",
    "service_name",
    "permit_without_stream",
];

fn transport_keys(network: &str) -> &'static [&'static str] {
    match network {
        "http" => &[
            "host",
            "path",
            "method",
            "headers",
            "idle_timeout",
            "ping_timeout",
        ],
        "ws" => &[
            "path",
            "headers",
            "max_early_data",
            "early_data_header_name",
        ],
        "httpupgrade" => &["host", "path", "headers"],
        "grpc" => &[
            "service_name",
            "idle_timeout",
            "ping_timeout",
            "permit_without_stream",
        ],
        _ => &[],
    }
}

fn migrate_transport(value: &mut Value) {
    let network = value
        .pointer("/transport/type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if matches!(network.as_str(), "" | "tcp" | "raw" | "none") {
        value.as_object_mut().unwrap().remove("transport");
        return;
    }
    if !matches!(
        network.as_str(),
        "http" | "ws" | "httpupgrade" | "grpc" | "quic"
    ) {
        return;
    }
    let Some(transport) = value.get_mut("transport").and_then(Value::as_object_mut) else {
        return;
    };
    let allowed = transport_keys(&network);
    // An explicit network change retires incompatible known options; opaque
    // future options stay available for inspection or manual JSON repair.
    for key in TRANSPORT_KEYS {
        if !allowed.contains(key) {
            transport.remove(*key);
        }
    }
    if network == "http" {
        if let Some(host) = transport
            .get("host")
            .and_then(Value::as_str)
            .map(str::to_owned)
        {
            transport.insert("host".into(), Value::Array(vec![Value::String(host)]));
        }
    } else if network == "httpupgrade" {
        if let Some(hosts) = transport.get("host").and_then(Value::as_array) {
            // Do not arbitrarily choose one host from a multi-host HTTP list.
            if hosts.len() == 1 && hosts[0].is_string() {
                transport.insert("host".into(), hosts[0].clone());
            }
        }
    }
}

fn validate_transport(value: &Value) -> Result<(), String> {
    let Some(transport) = value.get("transport").and_then(Value::as_object) else {
        return Ok(());
    };
    let network = transport.get("type").and_then(Value::as_str).unwrap_or("");
    let allowed = transport_keys(network);
    for key in TRANSPORT_KEYS {
        if transport.contains_key(*key) && !allowed.contains(key) {
            return Err(format!("transport.{key} is not supported by {network}; repair it in JSON or change the network"));
        }
    }
    if let Some(host) = transport.get("host") {
        let valid = match network {
            "httpupgrade" => host
                .as_str()
                .is_some_and(|host| !host.contains(',') && !host.chars().any(char::is_whitespace)),
            "http" => {
                host.is_string()
                    || host
                        .as_array()
                        .is_some_and(|a| a.iter().all(Value::is_string))
            }
            _ => false,
        };
        if !valid {
            return Err(format!("transport.host has the wrong type for {network}"));
        }
    }
    for key in ["path", "method", "service_name", "early_data_header_name"] {
        if transport.get(key).is_some_and(|v| !v.is_string()) {
            return Err(format!("transport.{key} must be text"));
        }
    }
    if transport.get("headers").is_some_and(|v| {
        v.as_object().is_none_or(|o| {
            o.values().any(|v| {
                !v.is_string() && !v.as_array().is_some_and(|a| a.iter().all(Value::is_string))
            })
        })
    }) {
        return Err("Transport headers must be an object of text values or text lists".into());
    }
    Ok(())
}

fn nonempty(value: &Value, pointer: &str) -> bool {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty())
}

fn validate_native(ty: ProfileType, value: &Value) -> Result<(), String> {
    use ProfileType::*;
    if !native_protocol(ty, value) {
        return Ok(());
    }
    for key in [
        "/tls",
        "/tls/utls",
        "/tls/reality",
        "/transport",
        "/multiplex",
    ] {
        if value.pointer(key).is_some_and(|v| !v.is_object()) {
            return Err(format!("{key} must be an object"));
        }
    }
    for key in [
        "/tls/enabled",
        "/tls/insecure",
        "/tls/utls/enabled",
        "/tls/reality/enabled",
        "/multiplex/enabled",
    ] {
        if value.pointer(key).is_some_and(|v| !v.is_boolean()) {
            return Err(format!("{key} must be true or false"));
        }
    }
    for key in [
        "/uuid",
        "/flow",
        "/tls/server_name",
        "/tls/reality/public_key",
        "/tls/reality/short_id",
        "/transport/type",
    ] {
        if value.pointer(key).is_some_and(|v| !v.is_string()) {
            return Err(format!("{key} must be text"));
        }
    }
    let host = value["server"].as_str().unwrap_or_default();
    let bare = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    if bare.is_empty()
        || bare.chars().any(char::is_whitespace)
        || bare.contains(['/', '?', '#', '@'])
        || (bare.contains(':') && bare.parse::<std::net::Ipv6Addr>().is_err())
    {
        return Err("Server must be a hostname or IP address without a scheme or port".into());
    }
    if value["server_port"]
        .as_u64()
        .is_none_or(|port| !(1..=65535).contains(&port))
    {
        return Err("Port must be an integer between 1 and 65535".into());
    }
    if matches!(ty, Vless | Vmess | Tuic) && !nonempty(value, "/uuid") {
        return Err("UUID is required".into());
    }
    let password_required = matches!(
        ty,
        Shadowsocks | Trojan | Hysteria2 | Tuic | AnyTls | Naive | ShadowTls
    ) || (ty == Hysteria && value["type"] == "hysteria2");
    if password_required && !nonempty(value, "/password") {
        return Err("Password is required".into());
    }
    if ty == Shadowsocks && !nonempty(value, "/method") {
        return Err("Cipher is required".into());
    }
    for key in ["up_mbps", "down_mbps", "alter_id"] {
        if value
            .get(key)
            .is_some_and(|v| v.as_u64().is_none_or(|n| n > u32::MAX as u64))
        {
            return Err(format!("{key} must be a non-negative 32-bit integer"));
        }
    }
    let tls = value
        .get("tls")
        .is_some_and(|t| t.get("enabled").and_then(Value::as_bool) != Some(false));
    let network = value
        .pointer("/transport/type")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !matches!(
        network,
        "" | "tcp" | "ws" | "http" | "httpupgrade" | "grpc" | "quic"
    ) {
        return Err("Unsupported native transport; use an Xray profile for XHTTP".into());
    }
    validate_transport(value)?;
    if ty == Vless {
        let flow = value["flow"].as_str().unwrap_or("");
        if !matches!(flow, "" | "xtls-rprx-vision") {
            return Err("VLESS flow must be blank or xtls-rprx-vision".into());
        }
        if flow == "xtls-rprx-vision" && (!tls || !matches!(network, "" | "tcp")) {
            return Err("XTLS Vision requires TLS or REALITY directly over TCP".into());
        }
        if value.pointer("/multiplex/enabled").and_then(Value::as_bool) == Some(true)
            && flow == "xtls-rprx-vision"
        {
            return Err("XTLS Vision cannot be combined with multiplexing".into());
        }
    }
    if value
        .pointer("/tls/reality/enabled")
        .and_then(Value::as_bool)
        == Some(true)
    {
        if !tls {
            return Err("REALITY requires TLS enabled".into());
        }
        if value.pointer("/tls/utls/enabled").and_then(Value::as_bool) != Some(true) {
            return Err("REALITY requires uTLS enabled".into());
        }
        if !nonempty(value, "/tls/server_name") {
            return Err("REALITY requires SNI".into());
        }
        let key = value
            .pointer("/tls/reality/public_key")
            .and_then(Value::as_str)
            .unwrap_or("");
        if key.len() != 43
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("REALITY public key must be 43 unpadded base64url characters".into());
        }
        let sid = value
            .pointer("/tls/reality/short_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        if sid.len() > 16
            || !sid.len().is_multiple_of(2)
            || !sid.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(
                "REALITY short ID must be even-length hexadecimal, at most 16 characters".into(),
            );
        }
    }
    Ok(())
}

/// Refresh all editable/display fields from authoritative JSON. Never retain
/// stale endpoint/auth data after an edit or after an optional field is cleared.
pub(crate) fn hydrate_outbound(value: &Value, raw: &str) -> ParsedOutbound {
    let text = |pointer: &str| {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let boolean = |pointer: &str| value.pointer(pointer).and_then(Value::as_bool);
    let list = |pointer: &str| {
        value.pointer(pointer).map(|v| {
            v.as_str().map(str::to_owned).unwrap_or_else(|| {
                v.as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_default()
            })
        })
    };
    let mut parsed = ParsedOutbound {
        tag: text("/tag"),
        server: text("/server"),
        server_port: value["server_port"]
            .as_u64()
            .and_then(|p| u16::try_from(p).ok())
            .filter(|p| *p > 0),
        uuid: text("/uuid"),
        password: text("/password").or_else(|| text("/auth_str")),
        username: text("/username"),
        method: text("/method"),
        flow: text("/flow"),
        security: text("/security"),
        alter_id: value["alter_id"]
            .as_i64()
            .and_then(|i| i32::try_from(i).ok()),
        transport: text("/transport/type"),
        host: text("/transport/headers/Host").or_else(|| list("/transport/host")),
        path: text("/transport/path"),
        service_name: text("/transport/service_name"),
        tls: boolean("/tls/enabled")
            .or_else(|| value.get("tls").filter(|v| v.is_object()).map(|_| true)),
        sni: text("/tls/server_name"),
        alpn: list("/tls/alpn"),
        fp: text("/tls/utls/fingerprint"),
        pbk: text("/tls/reality/public_key"),
        sid: text("/tls/reality/short_id"),
        spx: text("/tls/reality/spider_x"),
        insecure: boolean("/tls/insecure"),
        plugin: text("/plugin"),
        plugin_opts: text("/plugin_opts"),
        obfs: text("/obfs/password").or_else(|| text("/obfs")),
        up_mbps: value["up_mbps"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok()),
        down_mbps: value["down_mbps"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok()),
        congestion_control: text("/congestion_control"),
        udp_relay_mode: text("/udp_relay_mode"),
        packet_encoding: text("/packet_encoding"),
        raw_json: Some(raw.into()),
        finalmask: value.get("finalmask").map(Value::to_string),
    };
    parsed.hydrate_wireguard_fields(value);
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn profile(ty: ProfileType, value: Value) -> Profile {
        let mut profile = Profile::new(7, 2, "original", ty);
        profile.outbound_json = value.to_string();
        profile.traffic_downlink = 1234;
        profile.latency_ms = 42;
        profile.endpoint = crate::EndpointSource::Own;
        profile
    }

    fn vless() -> Profile {
        profile(
            ProfileType::Vless,
            json!({
                "type":"vless", "tag":"existing-tag", "server":"node.example", "server_port":38584,
                "uuid":"server-assigned-id", "flow":"xtls-rprx-vision", "packet_encoding":"xudp",
                "tls":{"enabled":true, "server_name":"tls.example", "alpn":["h2","http/1.1"],
                    "utls":{"enabled":true,"fingerprint":"chrome"},
                    "reality":{"enabled":true,"public_key":"A".repeat(43),"short_id":"abcd"},
                    "ech":{"enabled":true,"config":["opaque"]},"certificate_public_key_sha256":["pin"]},
                "multiplex":{"enabled":false,"padding":true},
                "future_extension":{"nested":[1,2,3]}, "detour":"chain-hop"
            }),
        )
    }

    fn set(draft: &mut ProfileEditDraft, key: &str, value: &str) {
        draft
            .fields
            .iter_mut()
            .find(|f| f.key == key)
            .unwrap()
            .value = value.into();
    }

    #[test]
    fn port_edit_preserves_auth_tls_unknown_fields_and_metadata() {
        let original = vless();
        let mut expected: Value = serde_json::from_str(&original.outbound_json).unwrap();
        expected["server_port"] = json!(443);
        let mut draft = ProfileEditDraft::from_profile(&original).unwrap();
        set(&mut draft, "/server_port", "443");
        draft.name = "  changed name  ".into();
        let edited = draft.build().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&edited.outbound_json).unwrap(),
            expected
        );
        assert_eq!(edited.name, "changed name");
        assert_eq!(edited.outbound.server_port, Some(443));
        assert_eq!(edited.display_address(), "node.example:443");
        assert_eq!(edited.traffic_downlink, original.traffic_downlink);
        assert_eq!(edited.latency_ms, original.latency_ms);
        assert_eq!(edited.endpoint, original.endpoint);
        assert_eq!(edited.id, original.id);
        assert_eq!(edited.group_id, original.group_id);
        assert_eq!(original.outbound_json, vless().outbound_json);
    }

    #[test]
    fn unchanged_form_is_lossless_even_with_scalar_alpn_and_unknown_tls_fields() {
        let mut original = vless();
        let mut json: Value = serde_json::from_str(&original.outbound_json).unwrap();
        json["tls"]["alpn"] = json!("h2");
        original.outbound_json = json.to_string();
        let draft = ProfileEditDraft::from_profile(&original).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&draft.build().unwrap().outbound_json).unwrap(),
            json
        );
    }

    #[test]
    fn clearing_optional_field_removes_it_and_refreshes_stale_parsed_cache() {
        let mut original = vless();
        original.outbound.flow = Some("stale-flow".into());
        original.outbound.server_port = Some(1);
        original.outbound.raw_json = Some("stale".into());
        let mut draft = ProfileEditDraft::from_profile(&original).unwrap();
        set(&mut draft, "/flow", "");
        let edited = draft.build().unwrap();
        let json: Value = serde_json::from_str(&edited.outbound_json).unwrap();
        assert!(json.get("flow").is_none());
        assert_eq!(edited.outbound.flow, None);
        assert_eq!(edited.outbound.server_port, Some(38584));
        assert_eq!(
            edited.outbound.raw_json.as_deref(),
            Some(edited.outbound_json.as_str())
        );
    }

    #[test]
    fn invalid_port_never_wraps_or_mutates_original() {
        let original = vless();
        for invalid in ["0", "65536", "-1", "1.5", "abc", "18446744073709551616", ""] {
            let mut draft = ProfileEditDraft::from_profile(&original).unwrap();
            set(&mut draft, "/server_port", invalid);
            assert!(draft.build().is_err(), "accepted {invalid}");
        }
        assert_eq!(original.outbound_json, vless().outbound_json);
    }

    #[test]
    fn raw_json_roundtrip_rebases_form_without_reverting_raw_edits() {
        let mut draft = ProfileEditDraft::from_profile(&vless()).unwrap();
        draft.name = "raw edit".into();
        let mut json: Value = serde_json::from_str(&draft.json).unwrap();
        json["server_port"] = json!(443);
        json["future_extension"]["new"] = json!(true);
        let mut draft = draft.with_json(json.to_string()).unwrap();
        assert_eq!(draft.name, "raw edit");
        assert_eq!(
            draft
                .fields
                .iter()
                .find(|f| f.key == "/server_port")
                .unwrap()
                .value,
            "443"
        );
        set(&mut draft, "/tls/server_name", "new.example");
        json["tls"]["server_name"] = json!("new.example");
        assert_eq!(
            serde_json::from_str::<Value>(&draft.build().unwrap().outbound_json).unwrap(),
            json
        );
    }

    #[test]
    fn raw_json_rejects_type_change_nonobject_and_form_conflict() {
        let mut draft = ProfileEditDraft::from_profile(&vless()).unwrap();
        assert!(draft.with_json("[]".into()).is_err());
        let mut json: Value = serde_json::from_str(&draft.json).unwrap();
        json["type"] = json!("trojan");
        assert!(draft.with_json(json.to_string()).is_err());
        json["type"] = json!("vless");
        json["server_port"] = json!(443);
        draft.json = json.to_string();
        set(&mut draft, "/server_port", "8443");
        assert!(draft.build().unwrap_err().contains("both"));
    }

    #[test]
    fn raw_only_profiles_preserve_custom_and_wireguard_peers() {
        for (ty, json) in [
            (
                ProfileType::Custom,
                json!({"subtype":"xrayfullconfig","config":"{\"outbounds\":[]}","future":true}),
            ),
            (
                ProfileType::XrayVless,
                json!({"protocol":"vless","settings":{"vnext":[{"address":"node.example","port":443,"users":[{"id":"opaque"}]}]},"streamSettings":{"network":"xhttp"}}),
            ),
            (
                ProfileType::Wireguard,
                json!({"type":"wireguard","private_key":"private","peers":[{"address":"first.example","port":100,"public_key":"key1"},{"address":"second.example","port":200,"public_key":"key2"}],"amnezia":{"i1":"secret"}}),
            ),
        ] {
            let original = profile(ty, json.clone());
            let draft = ProfileEditDraft::from_profile(&original).unwrap();
            assert!(!draft.is_structured());
            let edited = draft.build().unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&edited.outbound_json).unwrap(),
                json
            );
            if ty == ProfileType::Wireguard {
                assert_eq!(edited.outbound.server.as_deref(), Some("first.example"));
                assert_eq!(edited.outbound.server_port, Some(100));
            }
        }
    }

    #[test]
    fn vision_and_reality_constraints_are_validated_before_save() {
        for (key, value) in [
            ("/tls/enabled", "false"),
            ("/transport/type", "ws"),
            ("/tls/reality/public_key", "invalid"),
            ("/tls/reality/short_id", "abc"),
            ("/tls/reality/short_id", "0123456789abcdef00"),
            ("/tls/server_name", ""),
            ("/uuid", ""),
        ] {
            let mut draft = ProfileEditDraft::from_profile(&vless()).unwrap();
            set(&mut draft, key, value);
            assert!(draft.build().is_err(), "accepted {key}={value}");
        }
        let mut draft = ProfileEditDraft::from_profile(&vless()).unwrap();
        set(&mut draft, "/tls/reality/short_id", "");
        assert!(
            draft.build().is_ok(),
            "empty short ID is supported by REALITY"
        );
    }

    #[test]
    fn malformed_original_json_is_not_silently_replaced() {
        let mut original = vless();
        original.outbound_json = "broken{".into();
        assert!(ProfileEditDraft::from_profile(&original).is_err());
    }

    #[test]
    fn form_tcp_switch_removes_old_transport_only_when_explicitly_requested() {
        let mut original = vless();
        let mut json: Value = serde_json::from_str(&original.outbound_json).unwrap();
        json.as_object_mut().unwrap().remove("flow");
        json["transport"] = json!({"type":"ws","path":"/path","headers":{"Host":"cdn.example"}});
        original.outbound_json = json.to_string();
        let mut draft = ProfileEditDraft::from_profile(&original).unwrap();
        set(&mut draft, "/transport/type", "tcp");
        let edited = draft.build().unwrap();
        assert!(serde_json::from_str::<Value>(&edited.outbound_json)
            .unwrap()
            .get("transport")
            .is_none());
    }

    #[test]
    fn raw_json_cannot_bypass_endpoint_or_tls_type_validation() {
        let draft = ProfileEditDraft::from_profile(&vless()).unwrap();
        for (pointer, invalid) in [
            ("/server_port", json!(65536)),
            ("/tls", json!(false)),
            ("/tls/enabled", json!("false")),
            ("/tls/reality/short_id", json!(123)),
        ] {
            let mut value: Value = serde_json::from_str(&draft.json).unwrap();
            *value.pointer_mut(pointer).unwrap() = invalid;
            assert!(
                draft.with_json(value.to_string()).unwrap().build().is_err(),
                "accepted {pointer}"
            );
        }
    }

    #[test]
    fn state_save_retains_traffic_arriving_while_dialog_is_open() {
        let mut state = crate::AppState::empty();
        let gid = state.add_group("group");
        let id = state.add_profile(gid, "node", ProfileType::Vless);
        state
            .update_profile_outbound_json(id, vless().outbound_json)
            .unwrap();
        let mut draft = ProfileEditDraft::from_profile(state.profile(id).unwrap()).unwrap();
        set(&mut draft, "/server_port", "443");
        draft.name = "new name".into();
        let edited = draft.build().unwrap();
        state.set_profile_traffic(id, 5678, 2345);
        state.set_profile_latency(id, 50);
        state.replace_edited_profile(edited).unwrap();
        let saved = state.profile(id).unwrap();
        assert_eq!(saved.name, "new name");
        assert_eq!(saved.outbound.server_port, Some(443));
        assert_eq!(saved.traffic_downlink, 5678);
        assert_eq!(saved.traffic_uplink, 2345);
        assert_eq!(saved.latency_ms, 50);
    }

    #[test]
    fn state_rejects_invalid_or_retyped_edits_atomically() {
        let mut state = crate::AppState::empty();
        let gid = state.add_group("group");
        let id = state.add_profile(gid, "node", ProfileType::Vless);
        state
            .update_profile_outbound_json(id, vless().outbound_json)
            .unwrap();
        let original = state.profile(id).unwrap().clone();
        let mut edited = original.clone();
        edited.name = "should not save".into();
        let mut value: Value = serde_json::from_str(&edited.outbound_json).unwrap();
        value["server_port"] = json!(0);
        edited.outbound_json = value.to_string();
        assert!(state.replace_edited_profile(edited).is_err());
        let mut edited = original.clone();
        edited.profile_type = ProfileType::Trojan;
        assert!(state.replace_edited_profile(edited).is_err());
        assert_eq!(state.profile(id).unwrap().name, original.name);
        assert_eq!(
            state.profile(id).unwrap().outbound_json,
            original.outbound_json
        );
    }

    #[test]
    fn raw_full_config_export_preserves_document_after_edit() {
        let value = json!({"log":{"level":"warning"},"outbounds":[{"protocol":"vless","tag":"proxy","settings":{"vnext":[{"address":"node.example","port":443}]}}],"dns":{"servers":["localhost"]}});
        let original = profile(ProfileType::Custom, value.clone());
        let mut draft = ProfileEditDraft::from_profile(&original).unwrap();
        let mut expected = value;
        expected["outbounds"][0]["settings"]["vnext"][0]["port"] = json!(8443);
        draft.json = expected.to_string();
        let edited = draft.build().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&edited.export_outbound_json()).unwrap(),
            expected
        );
    }

    fn with_transport(transport: Value) -> Profile {
        let mut original = vless();
        let mut value: Value = serde_json::from_str(&original.outbound_json).unwrap();
        value.as_object_mut().unwrap().remove("flow");
        value["transport"] = transport;
        original.outbound_json = value.to_string();
        original
    }

    #[test]
    fn httpupgrade_host_edit_is_scalar_and_http_host_edit_is_list() {
        for (network, kind, expected) in [
            ("httpupgrade", ProfileEditKind::Text, json!("new.example")),
            ("http", ProfileEditKind::StringList, json!(["new.example"])),
        ] {
            let original =
                with_transport(json!({"type":network,"host":"old.example","path":"/transport"}));
            let mut draft = ProfileEditDraft::from_profile(&original).unwrap();
            assert_eq!(
                draft
                    .fields
                    .iter()
                    .find(|f| f.key == "/transport/host")
                    .unwrap()
                    .kind,
                kind
            );
            set(&mut draft, "/transport/host", "new.example");
            let built = draft.build().unwrap();
            let value: Value = serde_json::from_str(&built.outbound_json).unwrap();
            assert_eq!(value["transport"]["host"], expected);
        }
    }

    #[test]
    fn websocket_to_grpc_migration_removes_incompatible_known_keys() {
        let original = with_transport(
            json!({"type":"ws","path":"/ws","headers":{"Host":"cdn.example"},"max_early_data":2048,"early_data_header_name":"Sec-WebSocket-Protocol","future_transport_option":true}),
        );
        let mut draft = ProfileEditDraft::from_profile(&original).unwrap();
        set(&mut draft, "/transport/type", "grpc");
        set(&mut draft, "/transport/service_name", "my-service");
        let built = draft.build().unwrap();
        let value: Value = serde_json::from_str(&built.outbound_json).unwrap();
        assert_eq!(
            value["transport"],
            json!({"type":"grpc","service_name":"my-service","future_transport_option":true})
        );
    }

    #[test]
    fn raw_incompatible_transport_is_rejected_without_silently_discarding_options() {
        let original =
            with_transport(json!({"type":"grpc","path":"/ws","headers":{"Host":"cdn.example"}}));
        let draft = ProfileEditDraft::from_profile(&original).unwrap();
        assert!(draft.build().is_err());
        assert_eq!(
            serde_json::from_str::<Value>(&draft.serialize_draft().unwrap().outbound_json).unwrap(),
            serde_json::from_str::<Value>(&original.outbound_json).unwrap()
        );
    }

    #[test]
    fn reality_save_requires_enabled_utls_but_invalid_draft_can_switch_tabs() {
        let original = vless();
        let mut value: Value = serde_json::from_str(&original.outbound_json).unwrap();
        value["tls"]["utls"]["enabled"] = json!(false);
        let mut draft = ProfileEditDraft::from_profile(&original)
            .unwrap()
            .with_json(value.to_string())
            .unwrap();
        draft.name.clear();
        assert!(draft.serialize_draft().is_ok());
        assert!(draft.build().is_err());
        draft.name = "repaired".into();
        assert!(draft.build().unwrap_err().contains("uTLS"));
        set(&mut draft, "/tls/utls/enabled", "true");
        assert!(draft.build().is_ok());
        value["tls"].as_object_mut().unwrap().remove("utls");
        let draft = draft.with_json(value.to_string()).unwrap();
        assert!(draft.build().unwrap_err().contains("uTLS"));
    }

    #[test]
    fn invalid_semantic_draft_serializes_for_raw_repair_without_committing() {
        let mut draft = ProfileEditDraft::from_profile(&vless()).unwrap();
        set(&mut draft, "/server_port", "65536");
        draft.name.clear();
        let intermediate = draft.serialize_draft().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&intermediate.outbound_json).unwrap()["server_port"],
            json!(65536)
        );
        assert!(draft.build().is_err());
        let mut json: Value = serde_json::from_str(&intermediate.outbound_json).unwrap();
        json["server_port"] = json!(443);
        let mut repaired = draft.with_json(json.to_string()).unwrap();
        repaired.name = "repaired".into();
        assert_eq!(repaired.build().unwrap().outbound.server_port, Some(443));
    }

    #[test]
    fn source_match_keeps_opening_snapshot_across_rebases_and_ignores_live_stats() {
        let original = vless();
        let mut draft = ProfileEditDraft::from_profile(&original).unwrap();
        set(&mut draft, "/server_port", "443");
        let serialized = draft.serialize_draft().unwrap();
        let draft = draft.with_json(serialized.outbound_json).unwrap();
        assert!(draft.matches_source(&original));
        let mut current = original.clone();
        current.traffic_downlink += 99;
        current.latency_ms = 20;
        current.outbound_json = serde_json::to_string_pretty(
            &serde_json::from_str::<Value>(&current.outbound_json).unwrap(),
        )
        .unwrap();
        assert!(draft.matches_source(&current));
        current.name = "subscription changed name".into();
        assert!(!draft.matches_source(&current));
        current = original.clone();
        let mut value: Value = serde_json::from_str(&current.outbound_json).unwrap();
        value["server_port"] = json!(8443);
        current.outbound_json = value.to_string();
        assert!(!draft.matches_source(&current));
    }

    #[test]
    fn same_session_httpupgrade_to_http_parses_edited_host_as_list() {
        let original =
            with_transport(json!({"type":"httpupgrade","host":"original.example","path":"/path"}));
        for hosts in ["a.example, b.example", "a.example\nb.example"] {
            let mut draft = ProfileEditDraft::from_profile(&original).unwrap();
            set(&mut draft, "/transport/type", "http");
            set(&mut draft, "/transport/host", hosts);
            // Form order must not determine the resulting transport schema.
            draft.fields.reverse();
            let built = draft.build().unwrap();
            let value: Value = serde_json::from_str(&built.outbound_json).unwrap();
            assert_eq!(
                value["transport"]["host"],
                json!(["a.example", "b.example"])
            );
        }
    }

    #[test]
    fn same_session_http_to_httpupgrade_accepts_only_one_edited_host() {
        let original =
            with_transport(json!({"type":"http","host":["a.example","b.example"],"path":"/path"}));
        let mut draft = ProfileEditDraft::from_profile(&original).unwrap();
        set(&mut draft, "/transport/type", "httpupgrade");
        set(&mut draft, "/transport/host", "single.example");
        let built = draft.build().unwrap();
        let value: Value = serde_json::from_str(&built.outbound_json).unwrap();
        assert_eq!(value["transport"]["host"], json!("single.example"));
        for invalid in ["a.example, b.example", "a.example\nb.example"] {
            set(&mut draft, "/transport/host", invalid);
            assert!(draft.build().is_err());
        }
    }

    #[test]
    fn unedited_http_host_keeps_original_scalar_or_list_representation() {
        for host in [json!("a.example"), json!(["a.example", "b.example"])] {
            let original = with_transport(json!({"type":"http","host":host,"path":"/path"}));
            let mut draft = ProfileEditDraft::from_profile(&original).unwrap();
            set(&mut draft, "/server_port", "443");
            let built = draft.build().unwrap();
            let value: Value = serde_json::from_str(&built.outbound_json).unwrap();
            assert_eq!(value["transport"]["host"], host);
        }
    }
}
