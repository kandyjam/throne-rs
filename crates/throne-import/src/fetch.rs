//! HTTP fetch for subscription / remote route URLs.

use std::time::Duration;

use throne_domain::AppSettings;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchOptions {
    pub proxy_url: Option<String>,
    /// Override User-Agent (None/empty → `Throne/<version>` default).
    pub user_agent: Option<String>,
    /// Skip TLS certificate verification (upstream `net_insecure`).
    pub insecure: bool,
    /// Extra request headers (e.g. HWID device headers).
    pub extra_headers: Vec<(String, String)>,
}

impl FetchOptions {
    pub fn with_http_proxy(host: &str, port: i32) -> Result<Self, String> {
        if !(1..=u16::MAX as i32).contains(&port) {
            return Err(format!("invalid proxy port: {port}"));
        }
        let host = match host.trim() {
            "" | "::" => "127.0.0.1",
            value => value,
        };
        Ok(Self {
            proxy_url: Some(format!("http://{host}:{port}")),
            ..Self::default()
        })
    }

    /// Build fetch options from Basic Settings subscription network flags.
    ///
    /// Proxy is used when `net_use_proxy` **or** system proxy mode is on, matching
    /// upstream `HTTPRequestHelper::HttpGet`.
    pub fn from_settings(
        settings: &AppSettings,
        core_running: bool,
    ) -> Result<Self, String> {
        let use_proxy = settings.net_use_proxy || settings.system_proxy_enabled;
        let mut options = if use_proxy {
            if !core_running {
                return Err("Request with proxy but no profile started.".into());
            }
            Self::with_http_proxy(&settings.inbound_address, settings.inbound_socks_port)?
        } else {
            Self::default()
        };
        options.user_agent = Some(settings.effective_user_agent());
        options.insecure = settings.net_insecure;
        if settings.sub_send_hwid {
            options.extra_headers = hwid_headers(&settings.sub_custom_hwid_params);
        }
        Ok(options)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchResponse {
    pub body: String,
    pub headers: Vec<(String, String)>,
}

impl FetchResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Device identity used for subscription HWID headers (upstream DeviceDetailsHelper).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceDetails {
    pub hwid: String,
    pub os: String,
    pub os_version: String,
    pub model: String,
}

/// Best-effort local device details for `x-hwid` / related headers.
pub fn device_details() -> DeviceDetails {
    let os = std::env::consts::OS.to_string();
    let os_version = std::env::consts::ARCH.to_string();
    let model = hostname_fallback();
    let hwid = stable_hwid_hint(&os, &model);
    DeviceDetails {
        hwid,
        os,
        os_version,
        model,
    }
}

fn hostname_fallback() -> String {
    std::env::var("HOST")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".into())
}

fn stable_hwid_hint(os: &str, model: &str) -> String {
    // Prefer machine-id style files when present; otherwise a stable composite.
    for path in [
        "/etc/machine-id",
        "/var/lib/dbus/machine-id",
        "/etc/hostname",
    ] {
        if let Ok(raw) = std::fs::read_to_string(path) {
            let t = raw.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }
    format!("{os}-{model}")
}

/// Parse custom HWID params and merge with [`device_details`].
///
/// Format (upstream tooltip): `hwid=value,os=value,osVersion=value,model=value`
pub fn hwid_headers(custom_params: &str) -> Vec<(String, String)> {
    let details = device_details();
    let mut hwid = details.hwid;
    let mut os = details.os;
    let mut os_version = details.os_version;
    let mut model = details.model;

    for pair in custom_params.split(',') {
        let trimmed = pair.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "hwid" => hwid = value.to_string(),
            "os" => os = value.to_string(),
            "osversion" => os_version = value.to_string(),
            "model" => model = value.to_string(),
            _ => {}
        }
    }

    let mut headers = Vec::new();
    if !hwid.is_empty() {
        headers.push(("x-hwid".into(), hwid));
    }
    if !os.is_empty() {
        headers.push(("x-device-os".into(), os));
    }
    if !os_version.is_empty() {
        headers.push(("x-ver-os".into(), os_version));
    }
    if !model.is_empty() {
        headers.push(("x-device-model".into(), model));
    }
    headers
}

/// Fetch a subscription body over HTTP(S).
///
/// Uses a desktop-like User-Agent so some CDNs don't return empty 403 bodies.
pub fn fetch_url(url: &str) -> Result<String, String> {
    fetch_url_with_timeout(url, Duration::from_secs(30))
}

pub fn fetch_url_with_timeout(url: &str, timeout: Duration) -> Result<String, String> {
    fetch_url_with_options(url, timeout, &FetchOptions::default()).map(|response| response.body)
}

pub fn fetch_url_with_options(
    url: &str,
    timeout: Duration,
    options: &FetchOptions,
) -> Result<FetchResponse, String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("not an HTTP(S) URL: {url}"));
    }

    // ureq 3: configure via Agent::config_builder (AgentBuilder was removed).
    let mut config = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(timeout));
    if let Some(proxy_url) = options.proxy_url.as_deref() {
        let proxy = ureq::Proxy::new(proxy_url)
            .map_err(|error| format!("invalid HTTP proxy {proxy_url}: {error}"))?;
        config = config.proxy(Some(proxy));
    }
    if options.insecure {
        config = config.tls_config(
            ureq::tls::TlsConfig::builder()
                .disable_verification(true)
                .build(),
        );
    }
    let agent: ureq::Agent = config.build().into();

    let ua = options
        .user_agent
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(throne_domain::user_agent);

    let mut request = agent
        .get(url)
        .header("User-Agent", &ua)
        .header("Accept", "*/*");
    for (name, value) in &options.extra_headers {
        request = request.header(name, value);
    }

    let mut resp = request.call().map_err(|e| match e {
        ureq::Error::StatusCode(code) => format!("HTTP {code} from {url}"),
        other => format!("HTTP request failed: {other}"),
    })?;

    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status} from {url}"));
    }

    let headers = resp
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_string(),
                value.to_str().unwrap_or("").to_string(),
            )
        })
        .collect();

    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("read body failed: {e}"))?;
    if body.trim().is_empty() {
        return Err("empty subscription body".into());
    }
    Ok(FetchResponse { body, headers })
}

#[cfg(test)]
mod tests {
    use super::*;
    use throne_domain::AppSettings;

    #[test]
    fn response_header_lookup_is_case_insensitive() {
        let response = FetchResponse {
            body: "vless://example".into(),
            headers: vec![("subscription-userinfo".into(), "total=100".into())],
        };
        assert_eq!(response.header("Subscription-UserInfo"), Some("total=100"));
    }

    #[test]
    fn proxy_url_uses_normalized_mixed_inbound() {
        let options = FetchOptions::with_http_proxy("::", 2080).unwrap();
        assert_eq!(
            options.proxy_url.as_deref(),
            Some("http://127.0.0.1:2080")
        );
    }

    #[test]
    fn rejects_non_http() {
        assert!(fetch_url("ftp://x").is_err());
        assert!(fetch_url("not-a-url").is_err());
    }

    #[test]
    fn from_settings_uses_net_use_proxy_without_system_proxy() {
        let mut settings = AppSettings::default();
        settings.net_use_proxy = true;
        settings.inbound_socks_port = 1080;
        let err = FetchOptions::from_settings(&settings, false).unwrap_err();
        assert!(err.contains("no profile started"));
        let options = FetchOptions::from_settings(&settings, true).unwrap();
        assert_eq!(options.proxy_url.as_deref(), Some("http://127.0.0.1:1080"));
        assert_eq!(
            options.user_agent.as_deref(),
            Some(settings.effective_user_agent().as_str())
        );
    }

    #[test]
    fn hwid_headers_accept_custom_overrides() {
        let headers = hwid_headers("hwid=abc,os=linux,osVersion=1,model=box");
        let get = |name: &str| {
            headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("x-hwid"), Some("abc"));
        assert_eq!(get("x-device-os"), Some("linux"));
        assert_eq!(get("x-ver-os"), Some("1"));
        assert_eq!(get("x-device-model"), Some("box"));
    }

    #[test]
    fn from_settings_attaches_hwid_when_enabled() {
        let mut settings = AppSettings::default();
        settings.sub_send_hwid = true;
        settings.sub_custom_hwid_params = "hwid=test-id".into();
        let options = FetchOptions::from_settings(&settings, false).unwrap();
        assert!(
            options
                .extra_headers
                .iter()
                .any(|(k, v)| k == "x-hwid" && v == "test-id")
        );
    }
}
