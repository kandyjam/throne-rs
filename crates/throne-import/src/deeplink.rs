//! `throne://` deep links — path form from upstream `e7eb0438`.
//!
//! - `throne://add/<base64url>`
//! - `throne://route/<base64url>`
//! - `throne://remoteRoute/<base64url>`
//! - `throne://addsub/<url>` (host may be lowercased by OS URL parsers)

use crate::decode::decode_b64_flexible;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deeplink {
    Add { payload: String },
    Route { payload: String },
    RemoteRoute { payload: String },
    AddSub { url: String },
}

pub fn parse_deeplink(input: &str) -> Option<Deeplink> {
    let s = input.trim();
    let lower = s.to_ascii_lowercase();
    if !lower.starts_with("throne://") {
        return None;
    }

    // Path-style: throne://add/<payload>
    if let Some(rest) = strip_prefix_ci(s, "throne://add/") {
        let payload = decode_b64_flexible(rest).unwrap_or_else(|| rest.to_string());
        return Some(Deeplink::Add { payload });
    }
    if let Some(rest) = strip_prefix_ci(s, "throne://route/") {
        let payload = decode_b64_flexible(rest).unwrap_or_else(|| rest.to_string());
        return Some(Deeplink::Route { payload });
    }
    if let Some(rest) = strip_prefix_ci(s, "throne://remoteroute/") {
        let payload = decode_b64_flexible(rest).unwrap_or_else(|| rest.to_string());
        return Some(Deeplink::RemoteRoute { payload });
    }
    if let Some(rest) = strip_prefix_ci(s, "throne://addsub/") {
        return Some(Deeplink::AddSub {
            url: rest.to_string(),
        });
    }

    // Host-style after OS normalization: throne://addsub/https://...
    // QUrl lowercases host → throne://addsub/...
    if let Ok(u) = url::Url::parse(s) {
        if u.scheme().eq_ignore_ascii_case("throne") {
            let host = u.host_str().unwrap_or("").to_ascii_lowercase();
            let path = u.path().trim_start_matches('/');
            let joined = if path.is_empty() {
                String::new()
            } else {
                path.to_string()
            };
            // query may hold leftover data
            let payload = if joined.is_empty() {
                u.query().unwrap_or("").to_string()
            } else if let Some(q) = u.query() {
                format!("{joined}?{q}")
            } else {
                joined
            };

            match host.as_str() {
                "add" => {
                    let body = decode_b64_flexible(&payload).unwrap_or(payload);
                    return Some(Deeplink::Add { payload: body });
                }
                "route" => {
                    let body = decode_b64_flexible(&payload).unwrap_or(payload);
                    return Some(Deeplink::Route { payload: body });
                }
                "remoteroute" => {
                    let body = decode_b64_flexible(&payload).unwrap_or(payload);
                    return Some(Deeplink::RemoteRoute { payload: body });
                }
                "addsub" => {
                    return Some(Deeplink::AddSub { url: payload });
                }
                _ => {}
            }
        }
    }

    None
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_addsub_path() {
        let d = parse_deeplink("throne://addsub/https://example.com/sub").unwrap();
        assert_eq!(
            d,
            Deeplink::AddSub {
                url: "https://example.com/sub".into()
            }
        );
    }
}
