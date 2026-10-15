//! Error types for the openclaw-weixin Rust SDK.
//!
//! Every fallible operation in this crate returns [`Result<T>`], which is
//! parameterised by [`Error`]. Transport-level failures carry a classified
//! [`ErrorKind`] so callers can implement retry / backoff policies without
//! string matching, mirroring `classifyFetchError` in the TypeScript plugin.

use std::fmt;

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Classification of a transport-level (fetch/IO) failure.
///
/// Mirrors the categories produced by `classifyFetchError` in the TypeScript
/// implementation. Business-level errors (HTTP 200 with `ret != 0`) are *not*
/// classified here — they surface as [`ErrorKind::Api`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// DNS resolution failure (`ENOTFOUND`, `EAI_AGAIN`, ...).
    Dns,
    /// TCP connect failure or connection reset (`ECONNREFUSED`, ...).
    Tcp,
    /// TLS handshake error (`SSL`, `CERT`, ...).
    Tls,
    /// Client-side timeout, or an externally supplied abort signal fired.
    Timeout,
    /// The request was cancelled by an external abort signal.
    Cancelled,
    /// Any other network-level error.
    Unknown,
}

impl ErrorKind {
    /// Stable lowercase identifier, useful for logs and metrics.
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorKind::Dns => "dns",
            ErrorKind::Tcp => "tcp",
            ErrorKind::Tls => "tls",
            ErrorKind::Timeout => "timeout",
            ErrorKind::Cancelled => "cancelled",
            ErrorKind::Unknown => "unknown",
        }
    }

    /// Human readable description (English), matching the TS wording.
    pub fn description(&self) -> &'static str {
        match self {
            ErrorKind::Dns => "DNS resolution failed, check DNS configuration",
            ErrorKind::Tcp => "TCP connection error",
            ErrorKind::Tls => "TLS handshake error",
            ErrorKind::Timeout => "request timeout",
            ErrorKind::Cancelled => "request cancelled",
            ErrorKind::Unknown => "network request failed",
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single error returned by this SDK.
#[derive(Debug, Clone)]
pub struct Error {
    kind: ErrorKind,
    message: String,
    /// `true` when the request was aborted by an external [`tokio_util::sync::CancellationToken`]
    /// rather than an internal timeout.
    external_cancel: bool,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Error {
            kind,
            message: message.into(),
            external_cancel: false,
        }
    }

    /// A transport-level error that is not a timeout or a cancel.
    /// A generic error that is not a transport failure.
    pub fn unknown(message: impl Into<String>) -> Self {
        Error::new(ErrorKind::Unknown, message)
    }

    /// Mark this error as caused by an external abort signal.
    pub fn with_external_cancel(mut self) -> Self {
        self.external_cancel = true;
        self
    }

    /// Classification of the underlying failure.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// `true` when this error was caused by an external abort signal.
    pub fn is_external_cancel(&self) -> bool {
        self.external_cancel
    }

    /// `true` when this error represents a client-side timeout **or** an
    /// external cancellation. This mirrors the TypeScript AbortError check
    /// used to decide whether `getUpdates` should return an empty result.
    pub fn is_abort(&self) -> bool {
        matches!(self.kind, ErrorKind::Timeout | ErrorKind::Cancelled)
    }

    /// `true` when the failure looks like a connectivity problem (as opposed
    /// to a protocol / business error), i.e. it is retryable.
    pub fn is_transport(&self) -> bool {
        !matches!(self.kind, ErrorKind::Unknown | ErrorKind::Cancelled) || self.external_cancel
    }

    /// Build an [`Error`] from an arbitrary `std::error::Error`, classifying it
    /// by inspecting the error message and its source chain.
    pub fn from_reqwest(err: &reqwest::Error) -> Self {
        if err.is_timeout() {
            return Error::new(ErrorKind::Timeout, err.to_string());
        }
        if err.is_connect() {
            let classified = classify_fetch_error_string(&err.to_string());
            return Error::new(classified, err.to_string());
        }
        if err.is_body() || err.is_decode() || err.is_builder() || err.is_redirect() {
            return Error::new(ErrorKind::Unknown, err.to_string());
        }
        if err.is_request() {
            return Error::new(ErrorKind::Unknown, err.to_string());
        }
        let classified = classify_fetch_error_string(&err.to_string());
        Error::new(classified, err.to_string())
    }

    /// Classify a [reqwest::Error], preserving the [ErrorKind::Timeout] /
    /// [ErrorKind::Cancelled] distinction where reqwest already determined it.
    pub fn from_reqwest_wrap(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            return Error::new(ErrorKind::Timeout, err.to_string());
        }
        Self::classify(&err)
    }

    /// Classify an arbitrary error chain the same way the TypeScript
    /// `classifyFetchError` does for `fetch()` failures.
    ///
    /// The concatenation of `Display` output plus explicit source chains is
    /// inspected for the well-known libuv/OpenSSL/Rust std error codes.
    pub fn classify<E>(err: &E) -> Self
    where
        E: std::error::Error,
    {
        let mut haystack = err.to_string();
        let mut source = err.source();
        let mut depth = 0;
        while let Some(inner) = source {
            if depth > 4 {
                break;
            }
            haystack.push(' ');
            haystack.push_str(&inner.to_string());
            source = inner.source();
            depth += 1;
        }
        let kind = classify_fetch_error_string(&haystack);
        Error::new(kind, haystack)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        let kind = match err.kind() {
            std::io::ErrorKind::TimedOut => ErrorKind::Timeout,
            std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::ConnectionReset => {
                ErrorKind::Tcp
            }
            std::io::ErrorKind::NotConnected | std::io::ErrorKind::NetworkDown => ErrorKind::Tcp,
            _ => classify_fetch_error_string(&err.to_string()),
        };
        Error::new(kind, err.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::unknown(format!("json: {err}"))
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Error::from_reqwest(&err)
    }
}

impl From<reqwest::header::InvalidHeaderValue> for Error {
    fn from(err: reqwest::header::InvalidHeaderValue) -> Self {
        Error::unknown(format!("invalid header value: {err}"))
    }
}

impl From<reqwest::header::ToStrError> for Error {
    fn from(err: reqwest::header::ToStrError) -> Self {
        Error::unknown(format!("invalid header value: {err}"))
    }
}

impl From<rustls::Error> for Error {
    fn from(err: rustls::Error) -> Self {
        Error::unknown(format!("tls: {err}"))
    }
}

impl From<url::ParseError> for Error {
    fn from(err: url::ParseError) -> Self {
        Error::unknown(format!("invalid url: {err}"))
    }
}

impl From<hex::FromHexError> for Error {
    fn from(err: hex::FromHexError) -> Self {
        Error::unknown(format!("hex: {err}"))
    }
}

impl From<base64::DecodeError> for Error {
    fn from(err: base64::DecodeError) -> Self {
        Error::unknown(format!("base64: {err}"))
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Error::unknown(format!("sqlite: {err}"))
    }
}

/// Mirrors `classifyFetchError` from the TypeScript plugin: pattern-match an
/// error string against well-known network error codes.
pub fn classify_fetch_error_string(raw: &str) -> ErrorKind {
    if raw.is_empty() {
        return ErrorKind::Unknown;
    }
    if raw.contains("ENOTFOUND") || raw.contains("EAI_AGAIN") || raw.contains("getaddrinfo") {
        return ErrorKind::Dns;
    }
    if raw.contains("ECONNREFUSED") {
        return ErrorKind::Tcp;
    }
    if raw.contains("UND_ERR_CONNECT_TIMEOUT")
        || raw.contains("ETIMEDOUT")
        || raw.contains("ENETUNREACH")
        || raw.contains("EHOSTUNREACH")
        || raw.contains("operation timed out")
        || raw.contains("timed out")
    {
        return ErrorKind::Tcp;
    }
    if raw.contains("UND_ERR_SOCKET")
        || raw.contains("SSL")
        || raw.contains("TLS")
        || raw.contains("CERT")
        || raw.contains("UNABLE_TO_VERIFY")
        || raw.contains("DEPTH_ZERO")
        || raw.contains("certificate")
        || raw.contains("handshake")
    {
        return ErrorKind::Tls;
    }
    ErrorKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_dns_errors() {
        assert_eq!(
            classify_fetch_error_string("error sending request: ENOTFOUND"),
            ErrorKind::Dns
        );
        assert_eq!(classify_fetch_error_string("dns EAI_AGAIN"), ErrorKind::Dns);
    }

    #[test]
    fn classifies_tcp_errors() {
        assert_eq!(
            classify_fetch_error_string("ECONNREFUSED 127.0.0.1:80"),
            ErrorKind::Tcp
        );
        assert_eq!(classify_fetch_error_string("ETIMEDOUT"), ErrorKind::Tcp);
        assert_eq!(
            classify_fetch_error_string("operation timed out"),
            ErrorKind::Tcp
        );
    }

    #[test]
    fn classifies_tls_errors() {
        assert_eq!(
            classify_fetch_error_string("invalid peer certificate"),
            ErrorKind::Tls
        );
        assert_eq!(
            classify_fetch_error_string("TLS handshake failure"),
            ErrorKind::Tls
        );
    }

    #[test]
    fn aborts_cover_timeout_and_cancel() {
        assert!(Error::new(ErrorKind::Timeout, "t").is_abort());
        assert!(Error::new(ErrorKind::Cancelled, "c").is_abort());
        assert!(!Error::new(ErrorKind::Dns, "d").is_abort());
    }
}
