//! HTTP fetch for subscription / remote route URLs.

use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchOptions {
    pub proxy_url: Option<String>,
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
        })
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
    let agent: ureq::Agent = config.build().into();

    let mut resp = agent
        .get(url)
        .header(
            "User-Agent",
            "Throne/1.2.3 (throne-rs; +https://github.com/throneproj/Throne)",
        )
        .header("Accept", "*/*")
        .call()
        .map_err(|e| match e {
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
}
