//! Scanner RPC contracts from upstream `core/gen/libcore.proto` (1.4.0-beta.1).

use std::time::Duration;

use crate::{proto_wire, CoreError, CoreSession, UrlTestResult};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanEntry {
    pub cidr: String,
    pub port: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanTargetSpec {
    pub entries: Vec<ScanEntry>,
    pub default_ports: Vec<i32>,
    pub max_hosts_per_entry: i64,
    pub shuffle: bool,
    pub seed: u64,
    pub port_mode: String,
}

impl Default for ScanTargetSpec {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            default_ports: Vec::new(),
            max_hosts_per_entry: 0,
            shuffle: true,
            seed: 0,
            port_mode: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanIcmpOptions {
    pub enabled: bool,
    pub timeout_ms: i32,
    pub count: i32,
}

impl Default for ScanIcmpOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_ms: 1000,
            count: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanTcpOptions {
    pub enabled: bool,
    pub timeout_ms: i32,
    pub attempts: i32,
    pub fallback_port: i32,
}

impl Default for ScanTcpOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_ms: 2000,
            attempts: 1,
            fallback_port: 443,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanHttpOptions {
    pub enabled: bool,
    pub tls: bool,
    pub server_name: String,
    pub host: String,
    pub path: String,
    pub method: String,
    pub http_version: String,
    pub alpn: Vec<String>,
    pub min_version: String,
    pub max_version: String,
    pub fingerprint: String,
    pub insecure: bool,
    pub disable_sni: bool,
    pub fragment: bool,
    pub fragment_fallback_delay_ms: i32,
    pub record_fragment: bool,
    pub mixed_case_sni: bool,
    pub timeout_ms: i32,
    pub fallback_port: i32,
}

impl Default for ScanHttpOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            tls: true,
            server_name: String::new(),
            host: String::new(),
            path: "/".into(),
            method: "GET".into(),
            http_version: "1.1".into(),
            alpn: Vec::new(),
            min_version: String::new(),
            max_version: String::new(),
            fingerprint: String::new(),
            insecure: false,
            disable_sni: false,
            fragment: false,
            fragment_fallback_delay_ms: 0,
            record_fragment: false,
            mixed_case_sni: false,
            timeout_ms: 3000,
            fallback_port: 443,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanProbeRequest {
    pub session_id: String,
    pub spec: ScanTargetSpec,
    pub cursor: u64,
    /// Zero counts targets without sending any network probes.
    pub max_targets: i32,
    pub icmp: ScanIcmpOptions,
    pub tcp: ScanTcpOptions,
    pub http: ScanHttpOptions,
    pub concurrency: i32,
    pub spawn_interval_ms: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanProbeResult {
    pub address: String,
    pub port: i32,
    pub passed: bool,
    pub failed_phase: String,
    pub error: String,
    pub icmp_ms: i32,
    pub tcp_ms: i32,
    pub tls_ms: i32,
    pub http_ms: i32,
    pub http_status: i32,
    pub probe_port: i32,
    pub local_failure: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanProbeResponse {
    pub error: String,
    pub total: u64,
    pub next_cursor: u64,
    pub aborted: bool,
    pub results: Vec<ScanProbeResult>,
    pub sampled_entries: i32,
    pub invalid_entries: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanEvent {
    pub seq: i64,
    pub kind: String,
    pub phase: String,
    pub target: String,
    pub latency_ms: i32,
    pub error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryScanResponse {
    pub events: Vec<ScanEvent>,
    pub last_seq: i64,
    pub probed: i64,
    pub probe_passed: i64,
    pub url_tested: i64,
    pub url_passed: i64,
}

#[derive(Debug, Clone)]
pub struct ScanUrlTestRequest {
    pub session_id: String,
    pub config: String,
    pub outbound_tags: Vec<String>,
    pub url: String,
    pub use_default_outbound: bool,
    pub max_concurrency: i32,
    pub test_timeout_ms: i32,
    pub need_xray: bool,
    pub xray_config: String,
    pub xray_full_configs: Vec<String>,
    pub xray_outbound_dns_strategy: String,
    pub target_labels: Vec<String>,
    pub warm_latency: bool,
}

impl Default for ScanUrlTestRequest {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            config: String::new(),
            outbound_tags: Vec::new(),
            url: String::new(),
            use_default_outbound: false,
            max_concurrency: 0,
            test_timeout_ms: 0,
            need_xray: false,
            xray_config: String::new(),
            xray_full_configs: Vec::new(),
            xray_outbound_dns_strategy: String::new(),
            target_labels: Vec::new(),
            warm_latency: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanCheckNetworkResponse {
    pub up: bool,
    pub error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParseRuleSetResponse {
    pub error: String,
    pub cidrs: Vec<String>,
    pub skipped_rules: i32,
    pub version: i32,
}

impl CoreSession {
    /// Count or probe the requested range. Use bounded batches to remain responsive.
    /// Core-level errors stay in the response so partial results are preserved.
    pub fn scan_probe(
        &mut self,
        request: &ScanProbeRequest,
    ) -> Result<ScanProbeResponse, CoreError> {
        self.ensure_connected()?;
        let data = self.call("ScanProbe", &encode_probe(request), probe_timeout(request))?;
        decode_probe(&data)
    }

    pub fn query_scan(
        &mut self,
        session_id: &str,
        after_seq: i64,
    ) -> Result<QueryScanResponse, CoreError> {
        self.ensure_connected()?;
        let data = self.call(
            "QueryScan",
            &encode_query(session_id, after_seq),
            Duration::from_secs(5),
        )?;
        decode_query(&data)
    }

    /// A stopped session ID cannot be reused for further probes.
    pub fn stop_scan(&mut self, session_id: &str) -> Result<(), CoreError> {
        self.ensure_connected()?;
        self.call(
            "StopScan",
            &encode_session(session_id),
            Duration::from_secs(5),
        )?;
        Ok(())
    }

    pub fn scan_url_test(
        &mut self,
        request: &ScanUrlTestRequest,
    ) -> Result<Vec<UrlTestResult>, CoreError> {
        self.ensure_connected()?;
        let count = request.outbound_tags.len().max(1) as u64;
        let timeout = Duration::from_millis(
            30_000 + count.saturating_mul(request.test_timeout_ms.max(10_000) as u64),
        );
        let data = self.call("ScanURLTest", &encode_url_test(request), timeout)?;
        proto_wire::decode_test_resp(&data)
    }

    pub fn scan_check_network(
        &mut self,
        targets: &[String],
        timeout_ms: i32,
    ) -> Result<ScanCheckNetworkResponse, CoreError> {
        self.ensure_connected()?;
        let mut payload = Vec::new();
        for target in targets {
            string(&mut payload, 1, target);
        }
        number(&mut payload, 2, timeout_ms as u64);
        let data = self.call(
            "ScanCheckNetwork",
            &payload,
            Duration::from_millis(5_000 + timeout_ms.max(3_000) as u64),
        )?;
        let mut result = ScanCheckNetworkResponse::default();
        fields(&data, |field, value| {
            match (field, value) {
                (1, Value::Number(v)) => result.up = v != 0,
                (2, Value::Bytes(v)) => result.error = text(v)?,
                _ => {}
            }
            Ok(())
        })?;
        Ok(result)
    }

    /// Extract CIDRs from JSON or binary SRS rule sets without probing the network.
    pub fn parse_rule_set(&mut self, content: &[u8]) -> Result<ParseRuleSetResponse, CoreError> {
        self.ensure_connected()?;
        let mut payload = Vec::new();
        bytes(&mut payload, 1, content);
        let data = self.call("ParseRuleSet", &payload, Duration::from_secs(30))?;
        decode_rule_set(&data)
    }
}

fn probe_timeout(request: &ScanProbeRequest) -> Duration {
    let targets = request.max_targets.max(0) as u64;
    let mut per_target = 0u64;
    if request.icmp.enabled {
        per_target += (request.icmp.timeout_ms.max(1000) as u64)
            .saturating_mul(request.icmp.count.max(1) as u64);
    }
    if request.tcp.enabled {
        per_target += (request.tcp.timeout_ms.max(2000) as u64)
            .saturating_mul(request.tcp.attempts.max(1) as u64);
    }
    if request.http.enabled {
        per_target += 2 * request.http.timeout_ms.max(3000) as u64;
    }
    let delay = request.spawn_interval_ms.max(0) as u64;
    Duration::from_millis(
        30_000u64.saturating_add(targets.saturating_mul(per_target.saturating_add(delay))),
    )
}

fn encode_probe(request: &ScanProbeRequest) -> Vec<u8> {
    let mut out = encode_session(&request.session_id);
    let mut spec = Vec::new();
    for entry in &request.spec.entries {
        let mut item = Vec::new();
        string(&mut item, 1, &entry.cidr);
        number(&mut item, 2, entry.port as u64);
        bytes(&mut spec, 1, &item);
    }
    for port in &request.spec.default_ports {
        number(&mut spec, 2, *port as u64);
    }
    number(&mut spec, 3, request.spec.max_hosts_per_entry as u64);
    number(&mut spec, 4, request.spec.shuffle.into());
    number(&mut spec, 5, request.spec.seed);
    string(&mut spec, 6, &request.spec.port_mode);
    bytes(&mut out, 2, &spec);
    number(&mut out, 3, request.cursor);
    number(&mut out, 4, request.max_targets as u64);
    let mut icmp = Vec::new();
    number(&mut icmp, 1, request.icmp.enabled.into());
    number(&mut icmp, 2, request.icmp.timeout_ms as u64);
    number(&mut icmp, 3, request.icmp.count as u64);
    bytes(&mut out, 5, &icmp);
    let mut tcp = Vec::new();
    number(&mut tcp, 1, request.tcp.enabled.into());
    number(&mut tcp, 2, request.tcp.timeout_ms as u64);
    number(&mut tcp, 3, request.tcp.attempts as u64);
    number(&mut tcp, 4, request.tcp.fallback_port as u64);
    bytes(&mut out, 6, &tcp);
    let mut http = Vec::new();
    let h = &request.http;
    number(&mut http, 1, h.enabled.into());
    number(&mut http, 2, h.tls.into());
    for (field, value) in [
        (3, &h.server_name),
        (4, &h.host),
        (5, &h.path),
        (6, &h.method),
        (7, &h.http_version),
    ] {
        string(&mut http, field, value);
    }
    for value in &h.alpn {
        string(&mut http, 8, value);
    }
    for (field, value) in [
        (9, &h.min_version),
        (10, &h.max_version),
        (11, &h.fingerprint),
    ] {
        string(&mut http, field, value);
    }
    for (field, value) in [
        (12, h.insecure),
        (13, h.disable_sni),
        (14, h.fragment),
        (16, h.record_fragment),
        (17, h.mixed_case_sni),
    ] {
        number(&mut http, field, value.into());
    }
    number(&mut http, 15, h.fragment_fallback_delay_ms as u64);
    number(&mut http, 21, h.timeout_ms as u64);
    number(&mut http, 22, h.fallback_port as u64);
    bytes(&mut out, 7, &http);
    number(&mut out, 8, request.concurrency as u64);
    number(&mut out, 9, request.spawn_interval_ms as u64);
    out
}

fn encode_session(session_id: &str) -> Vec<u8> {
    let mut out = Vec::new();
    string(&mut out, 1, session_id);
    out
}

fn encode_query(session_id: &str, after_seq: i64) -> Vec<u8> {
    let mut out = encode_session(session_id);
    number(&mut out, 2, after_seq as u64);
    out
}

fn encode_url_test(request: &ScanUrlTestRequest) -> Vec<u8> {
    let mut out = encode_session(&request.session_id);
    let mut test = proto_wire::encode_test_req(
        &request.config,
        &request.outbound_tags,
        &request.url,
        false,
        request.use_default_outbound,
        request.max_concurrency,
        request.test_timeout_ms,
    );
    number(&mut test, 8, request.need_xray.into());
    string(&mut test, 9, &request.xray_config);
    for config in &request.xray_full_configs {
        string(&mut test, 10, config);
    }
    string(&mut test, 14, &request.xray_outbound_dns_strategy);
    bytes(&mut out, 2, &test);
    for label in &request.target_labels {
        string(&mut out, 3, label);
    }
    number(&mut out, 4, request.warm_latency.into());
    out
}

fn decode_probe(data: &[u8]) -> Result<ScanProbeResponse, CoreError> {
    let mut result = ScanProbeResponse::default();
    fields(data, |field, value| {
        match (field, value) {
            (1, Value::Bytes(v)) => result.error = text(v)?,
            (2, Value::Number(v)) => result.total = v,
            (3, Value::Number(v)) => result.next_cursor = v,
            (4, Value::Number(v)) => result.aborted = v != 0,
            (5, Value::Bytes(v)) => result.results.push(decode_result(v)?),
            (6, Value::Number(v)) => result.sampled_entries = v as i32,
            (7, Value::Number(v)) => result.invalid_entries = v as i32,
            _ => {}
        }
        Ok(())
    })?;
    Ok(result)
}

fn decode_result(data: &[u8]) -> Result<ScanProbeResult, CoreError> {
    let mut result = ScanProbeResult::default();
    fields(data, |field, value| {
        match (field, value) {
            (1, Value::Bytes(v)) => result.address = text(v)?,
            (2, Value::Number(v)) => result.port = v as i32,
            (3, Value::Number(v)) => result.passed = v != 0,
            (4, Value::Bytes(v)) => result.failed_phase = text(v)?,
            (5, Value::Bytes(v)) => result.error = text(v)?,
            (6, Value::Number(v)) => result.icmp_ms = v as i32,
            (7, Value::Number(v)) => result.tcp_ms = v as i32,
            (8, Value::Number(v)) => result.tls_ms = v as i32,
            (9, Value::Number(v)) => result.http_ms = v as i32,
            (10, Value::Number(v)) => result.http_status = v as i32,
            (11, Value::Number(v)) => result.probe_port = v as i32,
            (12, Value::Number(v)) => result.local_failure = v != 0,
            _ => {}
        }
        Ok(())
    })?;
    Ok(result)
}

fn decode_query(data: &[u8]) -> Result<QueryScanResponse, CoreError> {
    let mut result = QueryScanResponse::default();
    fields(data, |field, value| {
        match (field, value) {
            (1, Value::Bytes(v)) => {
                let mut event = ScanEvent::default();
                fields(v, |f, v| {
                    match (f, v) {
                        (1, Value::Number(v)) => event.seq = v as i64,
                        (2, Value::Bytes(v)) => event.kind = text(v)?,
                        (3, Value::Bytes(v)) => event.phase = text(v)?,
                        (4, Value::Bytes(v)) => event.target = text(v)?,
                        (5, Value::Number(v)) => event.latency_ms = v as i32,
                        (6, Value::Bytes(v)) => event.error = text(v)?,
                        _ => {}
                    }
                    Ok(())
                })?;
                result.events.push(event);
            }
            (2, Value::Number(v)) => result.last_seq = v as i64,
            (3, Value::Number(v)) => result.probed = v as i64,
            (4, Value::Number(v)) => result.probe_passed = v as i64,
            (5, Value::Number(v)) => result.url_tested = v as i64,
            (6, Value::Number(v)) => result.url_passed = v as i64,
            _ => {}
        }
        Ok(())
    })?;
    Ok(result)
}

fn decode_rule_set(data: &[u8]) -> Result<ParseRuleSetResponse, CoreError> {
    let mut result = ParseRuleSetResponse::default();
    fields(data, |field, value| {
        match (field, value) {
            (1, Value::Bytes(v)) => result.error = text(v)?,
            (2, Value::Bytes(v)) => result.cidrs.push(text(v)?),
            (3, Value::Number(v)) => result.skipped_rules = v as i32,
            (4, Value::Number(v)) => result.version = v as i32,
            _ => {}
        }
        Ok(())
    })?;
    Ok(result)
}

fn varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        out.push(value as u8 | 128);
        value >>= 7;
    }
    out.push(value as u8);
}
fn number(out: &mut Vec<u8>, field: u32, value: u64) {
    varint(out, u64::from(field) << 3);
    varint(out, value);
}
fn bytes(out: &mut Vec<u8>, field: u32, value: &[u8]) {
    varint(out, (u64::from(field) << 3) | 2);
    varint(out, value.len() as u64);
    out.extend_from_slice(value);
}
fn string(out: &mut Vec<u8>, field: u32, value: &str) {
    bytes(out, field, value.as_bytes());
}
fn text(value: &[u8]) -> Result<String, CoreError> {
    String::from_utf8(value.to_vec())
        .map_err(|_| CoreError::Rpc("invalid UTF-8 in scanner response".into()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Value<'a> {
    Number(u64),
    Bytes(&'a [u8]),
    Ignored,
}

fn read_varint(input: &mut &[u8]) -> Result<u64, CoreError> {
    let mut result = 0;
    for index in 0..10 {
        let Some((&byte, rest)) = input.split_first() else {
            return Err(CoreError::Rpc("truncated scanner varint".into()));
        };
        *input = rest;
        if index == 9 && byte > 1 {
            return Err(CoreError::Rpc("scanner varint overflow".into()));
        }
        result |= u64::from(byte & 127) << (index * 7);
        if byte & 128 == 0 {
            return Ok(result);
        }
    }
    Err(CoreError::Rpc("scanner varint overflow".into()))
}

fn take<'a>(input: &mut &'a [u8], len: u64) -> Result<&'a [u8], CoreError> {
    let length =
        usize::try_from(len).map_err(|_| CoreError::Rpc("scanner field length overflow".into()))?;
    if length > input.len() {
        return Err(CoreError::Rpc("truncated scanner field".into()));
    }
    let (value, rest) = input.split_at(length);
    *input = rest;
    Ok(value)
}

fn fields(
    mut input: &[u8],
    mut visit: impl FnMut(u32, Value<'_>) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    while !input.is_empty() {
        let key = read_varint(&mut input)?;
        if key >> 3 == 0 || key >> 3 > 0x1fff_ffff {
            return Err(CoreError::Rpc("invalid scanner field number".into()));
        }
        let value = match key & 7 {
            0 => Value::Number(read_varint(&mut input)?),
            1 => {
                take(&mut input, 8)?;
                Value::Ignored
            }
            2 => {
                let length = read_varint(&mut input)?;
                Value::Bytes(take(&mut input, length)?)
            }
            5 => {
                take(&mut input, 4)?;
                Value::Ignored
            }
            wire => {
                return Err(CoreError::Rpc(format!(
                    "unsupported scanner wire type {wire}"
                )));
            }
        };
        visit((key >> 3) as u32, value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(data: &[u8], number: u32) -> Value<'_> {
        let mut result = Value::Ignored;
        let mut input = data;
        while !input.is_empty() {
            let key = read_varint(&mut input).unwrap();
            let value = match key & 7 {
                0 => Value::Number(read_varint(&mut input).unwrap()),
                2 => {
                    let len = read_varint(&mut input).unwrap();
                    Value::Bytes(take(&mut input, len).unwrap())
                }
                _ => panic!("unexpected wire type"),
            };
            if key >> 3 == u64::from(number) {
                result = value;
            }
        }
        result
    }

    #[test]
    fn scan_defaults_preserve_upstream_count_only_and_phase_defaults() {
        let encoded = encode_probe(&ScanProbeRequest::default());
        assert_eq!(field(&encoded, 4), Value::Number(0));
        let Value::Bytes(spec) = field(&encoded, 2) else {
            panic!()
        };
        assert_eq!(field(spec, 4), Value::Number(1));
        let Value::Bytes(http) = field(&encoded, 7) else {
            panic!()
        };
        assert_eq!(field(http, 1), Value::Number(0));
        assert_eq!(field(http, 2), Value::Number(1));
        assert_eq!(field(http, 5), Value::Bytes(b"/"));
        assert_eq!(field(http, 6), Value::Bytes(b"GET"));
        assert_eq!(field(http, 21), Value::Number(3000));
        assert_eq!(field(http, 22), Value::Number(443));
        for reserved in 18..=20 {
            assert_eq!(field(http, reserved), Value::Ignored);
        }
    }

    #[test]
    fn scan_request_preserves_64_bit_cursor_seed_and_nested_entry() {
        let request = ScanProbeRequest {
            session_id: "test".into(),
            cursor: u64::MAX - 1,
            max_targets: 8,
            spec: ScanTargetSpec {
                entries: vec![ScanEntry {
                    cidr: "2001:db8::/64".into(),
                    port: 8443,
                }],
                seed: u64::MAX,
                shuffle: false,
                port_mode: "merge".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let encoded = encode_probe(&request);
        assert_eq!(field(&encoded, 1), Value::Bytes(b"test"));
        assert_eq!(field(&encoded, 3), Value::Number(u64::MAX - 1));
        let Value::Bytes(spec) = field(&encoded, 2) else {
            panic!()
        };
        assert_eq!(field(spec, 4), Value::Number(0));
        assert_eq!(field(spec, 5), Value::Number(u64::MAX));
        let Value::Bytes(entry) = field(spec, 1) else {
            panic!()
        };
        assert_eq!(field(entry, 1), Value::Bytes(b"2001:db8::/64"));
        assert_eq!(field(entry, 2), Value::Number(8443));
    }

    #[test]
    fn scan_response_retains_partial_results_and_local_failure() {
        let mut item = Vec::new();
        string(&mut item, 1, "2001:db8::1");
        number(&mut item, 11, 443);
        number(&mut item, 12, 1);
        number(&mut item, 6, (-1i32) as u64);
        let mut data = Vec::new();
        string(&mut data, 1, "egress lost");
        number(&mut data, 2, 1u64 << 40);
        number(&mut data, 3, 19);
        number(&mut data, 4, 1);
        bytes(&mut data, 5, &item);
        let response = decode_probe(&data).unwrap();
        assert_eq!(response.error, "egress lost");
        assert_eq!(response.total, 1u64 << 40);
        assert_eq!(response.next_cursor, 19);
        assert!(response.aborted);
        assert_eq!(response.results[0].probe_port, 443);
        assert!(response.results[0].local_failure);
        assert_eq!(response.results[0].icmp_ms, -1);
    }

    #[test]
    fn scanner_events_and_rulesets_decode_without_losing_counters() {
        let mut event = Vec::new();
        number(&mut event, 1, 12);
        string(&mut event, 2, "ok");
        string(&mut event, 3, "tcp");
        string(&mut event, 4, "[::1]:443");
        number(&mut event, 5, 24);
        let mut data = Vec::new();
        bytes(&mut data, 1, &event);
        for key in 2..=6 {
            number(&mut data, key, u64::from(key) * 100);
        }
        let response = decode_query(&data).unwrap();
        assert_eq!(response.events[0].seq, 12);
        assert_eq!(response.events[0].target, "[::1]:443");
        assert_eq!(
            (
                response.last_seq,
                response.probed,
                response.probe_passed,
                response.url_tested,
                response.url_passed
            ),
            (200, 300, 400, 500, 600)
        );
        let mut data = Vec::new();
        string(&mut data, 2, "192.0.2.0/24");
        string(&mut data, 2, "2001:db8::/32");
        number(&mut data, 3, 2);
        number(&mut data, 4, 4);
        let response = decode_rule_set(&data).unwrap();
        assert_eq!(response.cidrs, ["192.0.2.0/24", "2001:db8::/32"]);
        assert_eq!((response.skipped_rules, response.version), (2, 4));
    }

    #[test]
    fn scanner_decoder_rejects_overflow_and_truncation_without_panicking() {
        for data in [
            vec![0],
            vec![0x10, 0x80],
            vec![0x0a, 5, 1],
            vec![
                0x10, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 2,
            ],
            vec![
                0x0a, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 1,
            ],
        ] {
            assert!(decode_probe(&data).is_err(), "{data:?}");
        }
    }

    #[test]
    fn scan_url_test_preserves_warm_latency_and_xray_extensions() {
        let request = ScanUrlTestRequest {
            session_id: "scan".into(),
            warm_latency: false,
            need_xray: true,
            xray_full_configs: vec!["full".into()],
            xray_outbound_dns_strategy: "UseIP".into(),
            ..Default::default()
        };
        let encoded = encode_url_test(&request);
        assert_eq!(field(&encoded, 4), Value::Number(0));
        let Value::Bytes(test) = field(&encoded, 2) else {
            panic!()
        };
        assert_eq!(field(test, 8), Value::Number(1));
        assert_eq!(field(test, 10), Value::Bytes(b"full"));
        assert_eq!(field(test, 14), Value::Bytes(b"UseIP"));
    }
}
