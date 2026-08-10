//! Minimal protobuf2 wire codec for libcore RPC messages.
//!
//! Avoids a full prost codegen dependency; only the fields we need.

use std::collections::HashMap;

use crate::CoreError;

/// Encode `LoadConfigReq` (subset of libcore.proto).
///
/// Important: upstream Go core dereferences optional bool pointers with `*in.NeedXray`
/// etc. (not getters). Unset fields are nil and **panic**, which drops the IPC
/// socket mid-call. Always encode every bool we might touch on the Start path.
pub fn encode_load_config_req(
    core_config: &str,
    disable_stats: bool,
    need_xray: bool,
    xray_config: &str,
    tun_ipv4_cidr: &str,
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(core_config.len() + xray_config.len() + tun_ipv4_cidr.len() + 80);
    write_string(&mut buf, 1, core_config);
    write_varint_field(&mut buf, 2, u64::from(disable_stats));
    write_varint_field(&mut buf, 3, 0); // need_extra_process
    write_varint_field(&mut buf, 8, 0); // extra_no_out
    write_varint_field(&mut buf, 9, u64::from(need_xray));
    if !xray_config.is_empty() {
        write_string(&mut buf, 10, xray_config);
    }
    // field 11 — Darwin core sets system DNS to tunIP+1 when non-empty
    if !tun_ipv4_cidr.is_empty() {
        write_string(&mut buf, 11, tun_ipv4_cidr);
    }
    buf
}

/// Decode `IsPrivilegedResponse.has_privilege` (field 1, bool).
pub fn decode_is_privileged_resp(data: &[u8]) -> Result<bool, CoreError> {
    let mut i = 0;
    let mut has = false;
    while i < data.len() {
        let (key, ni) = read_varint(data, i)?;
        i = ni;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match (field, wire) {
            (1, 0) => {
                let (v, ni) = read_varint(data, i)?;
                has = v != 0;
                i = ni;
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok(has)
}

pub fn encode_empty_req() -> Vec<u8> {
    Vec::new()
}

/// Encode `TestReq` for URL latency testing.
///
/// All bool/int fields the Go side dereferences with `*` must be present.
pub fn encode_test_req(
    config: &str,
    outbound_tags: &[String],
    url: &str,
    test_current: bool,
    use_default_outbound: bool,
    max_concurrency: i32,
    test_timeout_ms: i32,
) -> Vec<u8> {
    let mut buf = Vec::new();
    if !config.is_empty() {
        write_string(&mut buf, 1, config);
    }
    for tag in outbound_tags {
        write_string(&mut buf, 2, tag);
    }
    write_varint_field(&mut buf, 3, u64::from(use_default_outbound));
    write_string(&mut buf, 4, url);
    write_varint_field(&mut buf, 5, u64::from(test_current));
    write_varint_field(&mut buf, 6, max_concurrency as u64);
    write_varint_field(&mut buf, 7, test_timeout_ms as u64);
    write_varint_field(&mut buf, 8, 0); // need_xray
    buf
}

/// One URL-test result (`URLTestResp`).
#[derive(Debug, Clone, Default)]
pub struct UrlTestResult {
    pub outbound_tag: String,
    pub latency_ms: i32,
    pub error: String,
}

/// Decode `TestResp` / `QueryURLTestResponse` (`repeated URLTestResp results = 1`).
pub fn decode_test_resp(data: &[u8]) -> Result<Vec<UrlTestResult>, CoreError> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i)?;
        i = ni;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match (field, wire) {
            (1, 2) => {
                let (len, ni) = read_varint(data, i)?;
                i = ni;
                let len = len as usize;
                if i + len > data.len() {
                    return Err(CoreError::Rpc("truncated TestResp".into()));
                }
                out.push(decode_url_test_item(&data[i..i + len])?);
                i += len;
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok(out)
}

fn decode_url_test_item(data: &[u8]) -> Result<UrlTestResult, CoreError> {
    let mut r = UrlTestResult::default();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i)?;
        i = ni;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match (field, wire) {
            (1, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.outbound_tag = s;
                i = ni;
            }
            (2, 0) => {
                let (v, ni) = read_varint(data, i)?;
                r.latency_ms = v as i32;
                i = ni;
            }
            (3, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.error = s;
                i = ni;
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok(r)
}

/// Decode `QueryStatsResp` maps `ups` / `downs`.
pub fn decode_query_stats_resp(data: &[u8]) -> Result<(HashMap<String, i64>, HashMap<String, i64>), CoreError> {
    let mut ups = HashMap::new();
    let mut downs = HashMap::new();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i)?;
        i = ni;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match (field, wire) {
            (1, 2) | (2, 2) => {
                let (len, ni) = read_varint(data, i)?;
                i = ni;
                let len = len as usize;
                if i + len > data.len() {
                    return Err(CoreError::Rpc("truncated map entry".into()));
                }
                let (k, v) = decode_string_i64_entry(&data[i..i + len])?;
                i += len;
                if field == 1 {
                    ups.insert(k, v);
                } else {
                    downs.insert(k, v);
                }
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok((ups, downs))
}

fn decode_string_i64_entry(data: &[u8]) -> Result<(String, i64), CoreError> {
    let mut key = String::new();
    let mut val = 0i64;
    let mut i = 0;
    while i < data.len() {
        let (tag, ni) = read_varint(data, i)?;
        i = ni;
        let field = (tag >> 3) as u32;
        let wire = (tag & 0x7) as u8;
        match (field, wire) {
            (1, 2) => {
                let (s, ni) = read_string(data, i)?;
                key = s;
                i = ni;
            }
            (2, 0) => {
                let (v, ni) = read_varint(data, i)?;
                val = v as i64;
                i = ni;
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok((key, val))
}

/// One connection row from `QueryConnectionsResp`.
#[derive(Debug, Clone, Default)]
pub struct ConnectionRow {
    pub id: String,
    pub created_at: i64,
    pub upload: i64,
    pub download: i64,
    pub outbound: String,
    pub network: String,
    pub dest: String,
    pub protocol: String,
    pub domain: String,
    pub process: String,
    pub closed_at: i64,
}

/// Decode `QueryConnectionsResp` (`active` field 1, `closed` field 2).
pub fn decode_query_connections_resp(
    data: &[u8],
) -> Result<(Vec<ConnectionRow>, Vec<ConnectionRow>), CoreError> {
    let mut active = Vec::new();
    let mut closed = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i)?;
        i = ni;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match (field, wire) {
            (1, 2) | (2, 2) => {
                let (len, ni) = read_varint(data, i)?;
                i = ni;
                let len = len as usize;
                if i + len > data.len() {
                    return Err(CoreError::Rpc("truncated ConnectionMetaData".into()));
                }
                let row = decode_connection_meta(&data[i..i + len])?;
                i += len;
                if field == 1 {
                    active.push(row);
                } else {
                    closed.push(row);
                }
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok((active, closed))
}

fn decode_connection_meta(data: &[u8]) -> Result<ConnectionRow, CoreError> {
    let mut r = ConnectionRow::default();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i)?;
        i = ni;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match (field, wire) {
            (1, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.id = s;
                i = ni;
            }
            (2, 0) => {
                let (v, ni) = read_varint(data, i)?;
                r.created_at = v as i64;
                i = ni;
            }
            (3, 0) => {
                let (v, ni) = read_varint(data, i)?;
                r.upload = v as i64;
                i = ni;
            }
            (4, 0) => {
                let (v, ni) = read_varint(data, i)?;
                r.download = v as i64;
                i = ni;
            }
            (5, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.outbound = s;
                i = ni;
            }
            (6, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.network = s;
                i = ni;
            }
            (7, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.dest = s;
                i = ni;
            }
            (8, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.protocol = s;
                i = ni;
            }
            (9, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.domain = s;
                i = ni;
            }
            (10, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.process = s;
                i = ni;
            }
            (13, 0) => {
                let (v, ni) = read_varint(data, i)?;
                r.closed_at = v as i64;
                i = ni;
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok(r)
}

/// Encode `IPTestRequest`. All bool/int fields Go dereferences must be set.
pub fn encode_ip_test_req(
    config: &str,
    outbound_tags: &[String],
    use_default_outbound: bool,
    max_concurrency: i32,
    test_timeout_ms: i32,
) -> Vec<u8> {
    let mut buf = Vec::new();
    if !config.is_empty() {
        write_string(&mut buf, 1, config);
    }
    for tag in outbound_tags {
        write_string(&mut buf, 2, tag);
    }
    write_varint_field(&mut buf, 3, u64::from(use_default_outbound));
    write_varint_field(&mut buf, 4, max_concurrency as u64);
    write_varint_field(&mut buf, 5, test_timeout_ms as u64);
    write_varint_field(&mut buf, 6, 0); // need_xray
    buf
}

#[derive(Debug, Clone, Default)]
pub struct IpTestResult {
    pub outbound_tag: String,
    pub ip: String,
    pub country_code: String,
    pub error: String,
}

/// Decode `IPTestResp` (`repeated IPTestRes results = 1`).
pub fn decode_ip_test_resp(data: &[u8]) -> Result<Vec<IpTestResult>, CoreError> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i)?;
        i = ni;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match (field, wire) {
            (1, 2) => {
                let (len, ni) = read_varint(data, i)?;
                i = ni;
                let len = len as usize;
                if i + len > data.len() {
                    return Err(CoreError::Rpc("truncated IPTestResp".into()));
                }
                out.push(decode_ip_test_item(&data[i..i + len])?);
                i += len;
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok(out)
}

fn decode_ip_test_item(data: &[u8]) -> Result<IpTestResult, CoreError> {
    let mut r = IpTestResult::default();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i)?;
        i = ni;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match (field, wire) {
            (1, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.outbound_tag = s;
                i = ni;
            }
            (2, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.ip = s;
                i = ni;
            }
            (3, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.country_code = s;
                i = ni;
            }
            (4, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.error = s;
                i = ni;
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok(r)
}

/// Encode `SpeedTestRequest` — simple download on current or config tags.
pub fn encode_speed_test_req(
    config: &str,
    outbound_tags: &[String],
    test_current: bool,
    use_default_outbound: bool,
    simple_download: bool,
    timeout_ms: i32,
) -> Vec<u8> {
    let mut buf = Vec::new();
    if !config.is_empty() {
        write_string(&mut buf, 1, config);
    }
    for tag in outbound_tags {
        write_string(&mut buf, 2, tag);
    }
    write_varint_field(&mut buf, 3, u64::from(test_current));
    write_varint_field(&mut buf, 4, u64::from(use_default_outbound));
    write_varint_field(&mut buf, 5, 0); // test_download
    write_varint_field(&mut buf, 6, 0); // test_upload
    write_varint_field(&mut buf, 7, u64::from(simple_download));
    write_varint_field(&mut buf, 9, timeout_ms as u64);
    write_varint_field(&mut buf, 10, 0); // only_country
    write_varint_field(&mut buf, 11, 0); // country_concurrency
    write_varint_field(&mut buf, 12, 0); // need_xray
    buf
}

#[derive(Debug, Clone, Default)]
pub struct SpeedTestResult {
    pub dl_speed: String,
    pub ul_speed: String,
    pub latency: i32,
    pub outbound_tag: String,
    pub error: String,
    pub server_country: String,
}

/// Decode `SpeedTestResponse` (`repeated SpeedTestResult results = 1`).
pub fn decode_speed_test_resp(data: &[u8]) -> Result<Vec<SpeedTestResult>, CoreError> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i)?;
        i = ni;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match (field, wire) {
            (1, 2) => {
                let (len, ni) = read_varint(data, i)?;
                i = ni;
                let len = len as usize;
                if i + len > data.len() {
                    return Err(CoreError::Rpc("truncated SpeedTestResponse".into()));
                }
                out.push(decode_speed_test_item(&data[i..i + len])?);
                i += len;
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok(out)
}

fn decode_speed_test_item(data: &[u8]) -> Result<SpeedTestResult, CoreError> {
    let mut r = SpeedTestResult::default();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i)?;
        i = ni;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match (field, wire) {
            (1, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.dl_speed = s;
                i = ni;
            }
            (2, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.ul_speed = s;
                i = ni;
            }
            (3, 0) => {
                let (v, ni) = read_varint(data, i)?;
                r.latency = v as i32;
                i = ni;
            }
            (4, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.outbound_tag = s;
                i = ni;
            }
            (5, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.error = s;
                i = ni;
            }
            (7, 2) => {
                let (s, ni) = read_string(data, i)?;
                r.server_country = s;
                i = ni;
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok(r)
}

/// One member of a running auto-selector group (subset of `AutoSelectorMember`).
#[derive(Debug, Clone, Default)]
pub struct AutoSelectorMemberStatus {
    pub tag: String,
    pub rank: i32,
    pub state: String,
    pub selected: bool,
    pub qualified: bool,
    pub active: bool,
    pub average_ms: i32,
    pub failures: i32,
    pub last_error: String,
}

/// Snapshot of one running auto-selector group (`AutoSelectorStatus`).
#[derive(Debug, Clone, Default)]
pub struct AutoSelectorGroupStatus {
    pub tag: String,
    pub phase: String,
    pub selected: String,
    pub pinned: String,
    pub balance: bool,
    pub balance_mode: String,
    pub suspended: bool,
    pub members_total: i32,
    pub members_alive: i32,
    pub members_qualified: i32,
    pub last_switch_reason: String,
    pub members: Vec<AutoSelectorMemberStatus>,
}

/// Encode `AutoSelectorActionRequest` (`tag`, `action`, `member`).
pub fn encode_auto_selector_action(tag: &str, action: &str, member: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    if !tag.is_empty() {
        write_string(&mut buf, 1, tag);
    }
    if !action.is_empty() {
        write_string(&mut buf, 2, action);
    }
    if !member.is_empty() {
        write_string(&mut buf, 3, member);
    }
    buf
}

/// Decode `QueryAutoSelectorsResponse` (`repeated AutoSelectorStatus groups = 1`).
pub fn decode_query_auto_selectors_resp(
    data: &[u8],
) -> Result<Vec<AutoSelectorGroupStatus>, CoreError> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i)?;
        i = ni;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match (field, wire) {
            (1, 2) => {
                let (len, ni) = read_varint(data, i)?;
                i = ni;
                let len = len as usize;
                if i + len > data.len() {
                    return Err(CoreError::Rpc("truncated QueryAutoSelectorsResponse".into()));
                }
                out.push(decode_auto_selector_status(&data[i..i + len])?);
                i += len;
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok(out)
}

fn decode_auto_selector_status(data: &[u8]) -> Result<AutoSelectorGroupStatus, CoreError> {
    let mut g = AutoSelectorGroupStatus::default();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i)?;
        i = ni;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match (field, wire) {
            (1, 2) => {
                let (s, ni) = read_string(data, i)?;
                g.tag = s;
                i = ni;
            }
            (2, 2) => {
                let (s, ni) = read_string(data, i)?;
                g.phase = s;
                i = ni;
            }
            (3, 2) => {
                let (s, ni) = read_string(data, i)?;
                g.selected = s;
                i = ni;
            }
            (4, 2) => {
                let (s, ni) = read_string(data, i)?;
                // selected_udp — ignore for now
                let _ = s;
                i = ni;
            }
            (5, 0) => {
                let (v, ni) = read_varint(data, i)?;
                g.balance = v != 0;
                i = ni;
            }
            (6, 2) => {
                let (s, ni) = read_string(data, i)?;
                g.balance_mode = s;
                i = ni;
            }
            (7, 0) => {
                let (v, ni) = read_varint(data, i)?;
                g.suspended = v != 0;
                i = ni;
            }
            (9, 0) => {
                let (v, ni) = read_varint(data, i)?;
                g.members_total = v as i32;
                i = ni;
            }
            (11, 0) => {
                let (v, ni) = read_varint(data, i)?;
                g.members_alive = v as i32;
                i = ni;
            }
            (12, 0) => {
                let (v, ni) = read_varint(data, i)?;
                g.members_qualified = v as i32;
                i = ni;
            }
            (19, 2) => {
                let (s, ni) = read_string(data, i)?;
                g.last_switch_reason = s;
                i = ni;
            }
            (20, 2) => {
                let (len, ni) = read_varint(data, i)?;
                i = ni;
                let len = len as usize;
                if i + len > data.len() {
                    return Err(CoreError::Rpc("truncated AutoSelectorMember".into()));
                }
                g.members
                    .push(decode_auto_selector_member(&data[i..i + len])?);
                i += len;
            }
            (21, 2) => {
                let (s, ni) = read_string(data, i)?;
                g.pinned = s;
                i = ni;
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok(g)
}

fn decode_auto_selector_member(data: &[u8]) -> Result<AutoSelectorMemberStatus, CoreError> {
    let mut m = AutoSelectorMemberStatus::default();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i)?;
        i = ni;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match (field, wire) {
            (1, 2) => {
                let (s, ni) = read_string(data, i)?;
                m.tag = s;
                i = ni;
            }
            (2, 0) => {
                let (v, ni) = read_varint(data, i)?;
                m.rank = v as i32;
                i = ni;
            }
            (3, 2) => {
                let (s, ni) = read_string(data, i)?;
                m.state = s;
                i = ni;
            }
            (4, 0) => {
                let (v, ni) = read_varint(data, i)?;
                m.selected = v != 0;
                i = ni;
            }
            (6, 0) => {
                let (v, ni) = read_varint(data, i)?;
                m.qualified = v != 0;
                i = ni;
            }
            (7, 0) => {
                let (v, ni) = read_varint(data, i)?;
                m.active = v != 0;
                i = ni;
            }
            (8, 0) => {
                let (v, ni) = read_varint(data, i)?;
                m.average_ms = v as i32;
                i = ni;
            }
            (13, 0) => {
                let (v, ni) = read_varint(data, i)?;
                m.failures = v as i32;
                i = ni;
            }
            (20, 2) => {
                let (s, ni) = read_string(data, i)?;
                m.last_error = s;
                i = ni;
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok(m)
}

/// Decode `ErrorResp.error` (field 1). Empty string means OK.
pub fn decode_error_resp(data: &[u8]) -> Result<String, CoreError> {
    let mut i = 0;
    let mut error = String::new();
    while i < data.len() {
        let (key, ni) = read_varint(data, i)?;
        i = ni;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u8;
        match (field, wire) {
            (1, 2) => {
                let (s, ni) = read_string(data, i)?;
                error = s;
                i = ni;
            }
            _ => i = skip_field(data, i, wire)?,
        }
    }
    Ok(error)
}

fn write_string(buf: &mut Vec<u8>, field: u32, s: &str) {
    write_varint(buf, ((field as u64) << 3) | 2);
    write_varint(buf, s.len() as u64);
    buf.extend_from_slice(s.as_bytes());
}

fn write_varint_field(buf: &mut Vec<u8>, field: u32, value: u64) {
    write_varint(buf, ((field as u64) << 3) | 0);
    write_varint(buf, value);
}

fn write_varint(buf: &mut Vec<u8>, mut v: u64) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        buf.push(b);
        if v == 0 {
            break;
        }
    }
}

fn read_varint(data: &[u8], mut i: usize) -> Result<(u64, usize), CoreError> {
    let mut result = 0u64;
    let mut shift = 0;
    loop {
        if i >= data.len() {
            return Err(CoreError::Rpc("truncated varint".into()));
        }
        let b = data[i];
        i += 1;
        result |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Ok((result, i));
        }
        shift += 7;
        if shift > 63 {
            return Err(CoreError::Rpc("varint overflow".into()));
        }
    }
}

fn read_string(data: &[u8], i: usize) -> Result<(String, usize), CoreError> {
    let (len, ni) = read_varint(data, i)?;
    let len = len as usize;
    if ni + len > data.len() {
        return Err(CoreError::Rpc("truncated string".into()));
    }
    let s = String::from_utf8_lossy(&data[ni..ni + len]).into_owned();
    Ok((s, ni + len))
}

fn skip_field(data: &[u8], i: usize, wire: u8) -> Result<usize, CoreError> {
    match wire {
        0 => {
            let (_, ni) = read_varint(data, i)?;
            Ok(ni)
        }
        1 => {
            if i + 8 > data.len() {
                return Err(CoreError::Rpc("truncated fixed64".into()));
            }
            Ok(i + 8)
        }
        2 => {
            let (len, ni) = read_varint(data, i)?;
            let end = ni + len as usize;
            if end > data.len() {
                return Err(CoreError::Rpc("truncated length-delimited".into()));
            }
            Ok(end)
        }
        5 => {
            if i + 4 > data.len() {
                return Err(CoreError::Rpc("truncated fixed32".into()));
            }
            Ok(i + 4)
        }
        _ => Err(CoreError::Rpc(format!("unsupported wire type {wire}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_error_resp_empty() {
        assert_eq!(decode_error_resp(&[]).unwrap(), "");
    }

    #[test]
    fn encode_load_has_core_config() {
        let b = encode_load_config_req(r#"{"log":{}}"#, false, false, "", "");
        assert!(!b.is_empty());
        assert_eq!(b[0], 0x0a);
    }

    #[test]
    fn encode_load_always_includes_bool_fields() {
        let b = encode_load_config_req("{}", false, false, "", "");
        assert!(
            b.contains(&0x18),
            "need_extra_process must be encoded: {b:02x?}"
        );
        assert!(b.contains(&0x48), "need_xray must be encoded: {b:02x?}");
    }

    #[test]
    fn encode_load_includes_tun_ipv4_cidr() {
        let b = encode_load_config_req("{}", false, false, "", "172.19.0.1/24");
        // field 11 string tag = (11 << 3) | 2 = 0x5a
        assert!(
            b.contains(&0x5a),
            "tun_ipv4_cidr field tag missing: {b:02x?}"
        );
        let s = String::from_utf8_lossy(&b);
        assert!(s.contains("172.19.0.1/24"), "{s}");
    }

    #[test]
    fn encode_test_req_has_required_bools() {
        let b = encode_test_req(
            r#"{"outbounds":[]}"#,
            &["proxy".into()],
            "https://www.gstatic.com/generate_204",
            false,
            false,
            8,
            5000,
        );
        // field 5 test_current tag = (5<<3)|0 = 0x28
        assert!(b.contains(&0x28), "test_current missing: {b:02x?}");
        // field 3 use_default tag = 0x18
        assert!(b.contains(&0x18), "use_default missing: {b:02x?}");
    }

    #[test]
    fn decode_url_test_item_fields() {
        // hand-build URLTestResp: tag="proxy", latency=42
        let mut item = Vec::new();
        write_string(&mut item, 1, "proxy");
        write_varint_field(&mut item, 2, 42);
        let mut outer = Vec::new();
        write_varint(&mut outer, (1 << 3) | 2);
        write_varint(&mut outer, item.len() as u64);
        outer.extend_from_slice(&item);
        let r = decode_test_resp(&outer).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].outbound_tag, "proxy");
        assert_eq!(r[0].latency_ms, 42);
    }
}
