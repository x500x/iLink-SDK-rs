//! Redaction helpers for safe diagnostic output.
//!
//! Mirrors `src/util/redact.ts`: tokens keep only a short prefix, JSON bodies
//! have known-sensitive fields masked, and URLs lose their query string.

/// Default number of leading token characters preserved by [`redact_token`].
pub const DEFAULT_TOKEN_PREFIX_LEN: usize = 6;

/// Default truncation length for [`truncate`] / [`redact_body`].
pub const DEFAULT_BODY_MAX_LEN: usize = 200;

/// Truncate `s`, appending a length indicator when trimmed.
///
/// Returns an owned `String` (empty for empty input) rather than borrowing, so
/// the result can be handed straight to a `format!` without lifetime issues.
pub fn truncate(s: &str, max: usize) -> String {
    if s.is_empty() {
        return String::new();
    }
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out.push_str(&format!("(len={})", s.chars().count()));
    out
}

/// Redact a token/secret: show at most `prefix_len` leading characters plus the
/// total length. Returns `"(none)"` when absent.
pub fn redact_token(token: Option<&str>, prefix_len: Option<usize>) -> String {
    match token {
        None | Some("") => "(none)".to_string(),
        Some(token) => {
            let prefix_len = prefix_len.unwrap_or(DEFAULT_TOKEN_PREFIX_LEN);
            let len = token.chars().count();
            if len <= prefix_len {
                format!("****(len={len})")
            } else {
                let prefix: String = token.chars().take(prefix_len).collect();
                format!("{prefix}…(len={len})")
            }
        }
    }
}

/// Field names whose values are masked in logged JSON bodies.
const SENSITIVE_FIELDS: &[&str] = &[
    "context_token",
    "bot_token",
    "token",
    "authorization",
    "Authorization",
];

/// Mask a JSON body's sensitive values, then truncate for logging.
///
/// `{"context_token":"abc"}` becomes `{"context_token":"<redacted>"}`.
pub fn redact_body(body: &str, max_len: Option<usize>) -> String {
    let max_len = max_len.unwrap_or(DEFAULT_BODY_MAX_LEN);
    if body.is_empty() {
        return "(empty)".to_string();
    }
    let redacted = redact_json_fields(body);
    truncate(&redacted, max_len)
}

/// Replace the values of sensitive JSON keys with `<redacted>`.
///
/// This is a small hand-rolled scanner rather than full JSON parsing: it walks
/// the raw text and only rewrites `"key" : "value"` pairs, so it never
/// accidentally rewrites text that merely *looks* like it contains a key
/// inside a longer string value.
pub fn redact_json_fields(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    while i < bytes.len() {
        // Copy everything until the next quote.
        if bytes[i] != b'"' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }

        // We are at the start of a JSON string. Parse it.
        let (token, next) = read_json_string(input, i);
        out.push_str(&token);
        i = next;

        // Skip whitespace; require ':' right after to treat the string as a key.
        let mut j = i;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b':' {
            continue;
        }
        let key = unescape_json_string(&token);
        if !SENSITIVE_FIELDS.iter().any(|f| *f == key) {
            continue;
        }

        // Sensitive key. Copy the ':' and whitespace, then replace the
        // following string literal value (if present) with "<redacted>".
        out.push_str(&input[i..j + 1]);
        i = j + 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            out.push(bytes[i] as char);
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'"' {
            let (_value, next) = read_json_string(input, i);
            out.push_str("\"<redacted>\"");
            i = next;
        }
    }

    out
}

/// Read a JSON string literal starting at `start` (which must be `"`).
///
/// Returns the raw token including the surrounding quotes and the index just
/// past it. An unterminated string reads to the end of input.
fn read_json_string(input: &str, start: usize) -> (String, usize) {
    let bytes = input.as_bytes();
    let mut i = start + 1;
    let mut escaped = false;
    while i < bytes.len() {
        let c = bytes[i];
        i += 1;
        if escaped {
            escaped = false;
        } else if c == b'\\' {
            escaped = true;
        } else if c == b'"' {
            break;
        }
    }
    (input[start..i].to_string(), i)
}

/// Best-effort unescape of a JSON string token (surrounding quotes included).
fn unescape_json_string(token: &str) -> String {
    if token.len() < 2 {
        return String::new();
    }
    let inner = &token[1..token.len() - 1];
    serde_json::from_str::<String>(&format!("\"{inner}\"")).unwrap_or_else(|_| inner.to_string())
}

/// Strip the query string (which often carries signatures/tokens) from a URL,
/// keeping the scheme, host and path.
pub fn redact_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(parsed) => {
            let mut out = format!("{}://{}", parsed.scheme(), parsed.host_str().unwrap_or(""));
            if let Some(port) = parsed.port() {
                out.push_str(&format!(":{port}"));
            }
            out.push_str(parsed.path());
            if parsed.query().is_some() {
                out.push_str("?<redacted>");
            }
            out
        }
        Err(_) => truncate(url, 80),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_handles_empty() {
        assert_eq!(truncate("", 10), "");
    }

    #[test]
    fn truncate_keeps_short() {
        assert_eq!(truncate("short", 10), "short");
    }

    #[test]
    fn truncate_appends_len() {
        let out = truncate("a]long-string-here", 5);
        assert!(out.starts_with("a]lon"));
        assert!(out.ends_with("(len=18)"));
    }

    #[test]
    fn truncate_counts_chars_not_bytes() {
        assert_eq!(truncate("你好世界", 2), "你好…(len=4)");
    }

    #[test]
    fn redact_token_cases() {
        assert_eq!(redact_token(None, None), "(none)");
        assert_eq!(redact_token(Some(""), None), "(none)");
        assert_eq!(redact_token(Some("abc"), Some(6)), "****(len=3)");
        assert_eq!(redact_token(Some("abcdef1234"), None), "abcdef…(len=10)");
    }

    #[test]
    fn redact_body_masks_sensitive_keys() {
        let out = redact_body(
            r#"{"context_token":"secret","token":"t","other":"keep"}"#,
            None,
        );
        assert!(out.contains(r#""context_token":"<redacted>""#));
        assert!(out.contains(r#""token":"<redacted>""#));
        assert!(out.contains(r#""other":"keep""#));
    }

    #[test]
    fn redact_body_empty() {
        assert_eq!(redact_body("", None), "(empty)");
    }

    #[test]
    fn redact_body_truncates() {
        let out = redact_body(&"a".repeat(500), Some(10));
        assert!(out.ends_with("…(len=500)"));
    }

    #[test]
    fn redact_body_does_not_mangle_text_mentioning_keys() {
        // "the token is here" must stay intact: no JSON key/value pair.
        let out = redact_body(r#"{"note":"the token is here"}"#, None);
        assert!(out.contains(r#""note":"the token is here""#));
    }

    #[test]
    fn redact_url_strips_query() {
        assert_eq!(
            redact_url("https://ilinkai.weixin.qq.com/ilink/bot/getupdates?token=abc"),
            "https://ilinkai.weixin.qq.com/ilink/bot/getupdates?<redacted>"
        );
    }

    #[test]
    fn redact_url_keeps_plain() {
        assert_eq!(redact_url("https://a.b/c"), "https://a.b/c");
    }

    #[test]
    fn redact_url_falls_back_for_non_urls() {
        assert_eq!(redact_url("not a url at all"), "not a url at all");
    }
}
