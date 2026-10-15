//! CDN URL construction, mirroring `src/cdn/cdn-url.ts`.

use url::Url;

use crate::error::{Error, ErrorKind, Result};

/// Default CDN base URL used when the account config does not override it.
pub const CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";

/// Whether to fall back to client-side URL construction when the server does
/// not return `full_url`.
///
/// Mirrors `ENABLE_CDN_URL_FALLBACK`; kept as a constant (not a runtime knob)
/// but overridable in tests through [`set_url_fallback`].
pub const ENABLE_CDN_URL_FALLBACK: bool = true;

/// Build a CDN download URL from `encrypt_query_param`.
///
/// `<cdn_base_url>/download?encrypted_query_param=<url-encoded param>`
pub fn build_cdn_download_url(encrypted_query_param: &str, cdn_base_url: &str) -> Result<String> {
    let mut url = Url::parse(&join_segment(cdn_base_url, "download"))?;
    url.query_pairs_mut()
        .append_pair("encrypted_query_param", encrypted_query_param);
    Ok(url.to_string())
}

/// Build a CDN upload URL from `upload_param` and `filekey`.
///
/// `<cdn_base_url>/upload?encrypted_query_param=<upload_param>&filekey=<filekey>`
pub fn build_cdn_upload_url(
    cdn_base_url: &str,
    upload_param: &str,
    filekey: &str,
) -> Result<String> {
    let mut url = Url::parse(&join_segment(cdn_base_url, "upload"))?;
    url.query_pairs_mut()
        .append_pair("encrypted_query_param", upload_param);
    url.query_pairs_mut().append_pair("filekey", filekey);
    Ok(url.to_string())
}

/// Append `segment` to a base URL, normalising slashes first.
///
/// `url::Url`'s `path_segments_mut().push()` normalises away a trailing
/// slash, which would turn `.../c2c/` into `.../c2c//download`, so the path is
/// composed as a plain string after trimming any trailing slashes.
fn join_segment(base_url: &str, segment: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    format!("{trimmed}/{segment}")
}

/// Resolve the effective URL for a download reference.
///
/// A server-provided `full_url` always wins; otherwise the URL is built from
/// `encrypt_query_param` against `cdn_base_url` when fallback is enabled.
pub fn resolve_download_url(
    full_url: Option<&str>,
    encrypted_query_param: &str,
    cdn_base_url: &str,
    label: &str,
) -> Result<String> {
    if let Some(full) = full_url.filter(|u| !u.trim().is_empty()) {
        return Ok(full.trim().to_string());
    }
    if ENABLE_CDN_URL_FALLBACK {
        return build_cdn_download_url(encrypted_query_param, cdn_base_url);
    }
    Err(Error::new(
        ErrorKind::Unknown,
        format!("{label}: fullUrl is required (CDN URL fallback is disabled)"),
    ))
}

/// Resolve the effective URL for an upload.
///
/// A server-provided `upload_full_url` always wins; otherwise the URL is built
/// from `upload_param` and `filekey`.
pub fn resolve_upload_url(
    upload_full_url: Option<&str>,
    upload_param: Option<&str>,
    filekey: &str,
    cdn_base_url: &str,
    label: &str,
) -> Result<String> {
    if let Some(full) = upload_full_url.filter(|u| !u.trim().is_empty()) {
        return Ok(full.trim().to_string());
    }
    match upload_param.filter(|p| !p.trim().is_empty()) {
        Some(param) => build_cdn_upload_url(cdn_base_url, param, filekey),
        None => Err(Error::new(
            ErrorKind::Unknown,
            format!("{label}: CDN upload URL missing (need upload_full_url or upload_param)"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_download_url() {
        let url = build_cdn_download_url("abc+def/ghi", "https://cdn.example/c2c").unwrap();
        assert_eq!(
            url,
            "https://cdn.example/c2c/download?encrypted_query_param=abc%2Bdef%2Fghi"
        );
    }

    #[test]
    fn handles_trailing_slash_in_base() {
        let a = build_cdn_download_url("p", "https://cdn.example/c2c").unwrap();
        let b = build_cdn_download_url("p", "https://cdn.example/c2c/").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn builds_upload_url() {
        let url = build_cdn_upload_url("https://cdn.example/c2c", "param", "key").unwrap();
        assert_eq!(
            url,
            "https://cdn.example/c2c/upload?encrypted_query_param=param&filekey=key"
        );
    }

    #[test]
    fn full_url_takes_precedence() {
        let url = resolve_download_url(
            Some("https://x/full?token=1"),
            "ignored",
            "https://cdn",
            "lbl",
        )
        .unwrap();
        assert_eq!(url, "https://x/full?token=1");
    }

    #[test]
    fn blank_full_url_falls_through() {
        let url = resolve_download_url(Some("   "), "p", "https://cdn/c2c", "lbl").unwrap();
        assert!(url.contains("encrypted_query_param=p"));
    }

    #[test]
    fn upload_requires_param_or_full_url() {
        assert!(resolve_upload_url(None, None, "k", "https://cdn/c2c", "lbl").is_err());
        assert!(
            resolve_upload_url(Some("https://x/u"), None, "k", "https://cdn/c2c", "lbl").is_ok()
        );
        assert!(resolve_upload_url(None, Some("p"), "k", "https://cdn/c2c", "lbl").is_ok());
    }

    #[test]
    fn invalid_base_url_errors() {
        assert!(build_cdn_download_url("p", "not a url").is_err());
    }
}
