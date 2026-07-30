//! HTTP fetch for subscription / remote route URLs.

use std::time::Duration;

/// Fetch a subscription body over HTTP(S).
///
/// Uses a desktop-like User-Agent so some CDNs don't return empty 403 bodies.
pub fn fetch_url(url: &str) -> Result<String, String> {
    fetch_url_with_timeout(url, Duration::from_secs(30))
}

pub fn fetch_url_with_timeout(url: &str, timeout: Duration) -> Result<String, String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("not an HTTP(S) URL: {url}"));
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(timeout)
        .build();
    let resp = agent
        .get(url)
        .set(
            "User-Agent",
            "Throne/1.2.2 (throne-rs; +https://github.com/throneproj/Throne)",
        )
        .set("Accept", "*/*")
        .call()
        .map_err(|e| format!("HTTP request failed: {e}"))?;
    let status = resp.status();
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status} from {url}"));
    }
    let body = resp
        .into_string()
        .map_err(|e| format!("read body failed: {e}"))?;
    if body.trim().is_empty() {
        return Err("empty subscription body".into());
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http() {
        assert!(fetch_url("ftp://x").is_err());
        assert!(fetch_url("not-a-url").is_err());
    }
}
