use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD},
    Engine,
};

/// Try standard and URL-safe base64 (with/without padding), matching upstream
/// `DecodeB64IfValid` flexibility.
pub fn decode_b64_flexible(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    for engine in [&URL_SAFE_NO_PAD, &URL_SAFE, &STANDARD] {
        if let Ok(bytes) = engine.decode(s) {
            if let Ok(text) = String::from_utf8(bytes) {
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    // Some feeds embed whitespace/newlines inside b64
    let compact: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if compact != s {
        return decode_b64_flexible(&compact);
    }
    None
}

/// Same engines as [`decode_b64_flexible`] but keep raw bytes (vpn:// JSON).
pub fn decode_b64_bytes(input: &str) -> Option<Vec<u8>> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    for engine in [
        &URL_SAFE_NO_PAD,
        &URL_SAFE,
        &STANDARD,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
    ] {
        if let Ok(bytes) = engine.decode(s) {
            if !bytes.is_empty() {
                return Some(bytes);
            }
        }
    }
    let compact: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if compact != s {
        return decode_b64_bytes(&compact);
    }
    None
}

pub fn percent_decode(s: &str) -> String {
    urlencoding::decode(s)
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;

    #[test]
    fn roundtrip_standard() {
        let raw = "hello\nworld";
        let enc = STANDARD.encode(raw);
        assert_eq!(decode_b64_flexible(&enc).as_deref(), Some(raw));
    }
}
