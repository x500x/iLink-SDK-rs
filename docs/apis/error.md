# Error handling API

Every fallible operation in `ilink_sdk` returns `Result<T>` (i.e.
`std::result::Result<T, Error>`). The error-handling module lives in
[`../../src/error.rs`](../../src/error.rs). It classifies transport failures into
`ErrorKind` so callers can branch on the failure category instead of string
matching.

## Type overview

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

## `enum ErrorKind`

Classification of a transport-level failure. `Business`-level failures (HTTP 200
but `ret != 0`) are **not** represented here — they surface as `ErrorKind::Unknown`
with the business error text in `Error::message`.

| Variant | Meaning | Typical trigger |
| --- | --- | --- |
| `Dns` | DNS resolution failure | `ENOTFOUND`, `EAI_AGAIN`, `getaddrinfo` |
| `Tcp` | TCP connect / reset failure | `ECONNREFUSED`, `ETIMEDOUT`, `ENETUNREACH` |
| `Tls` | TLS handshake error | `SSL`, `CERT`, certificate verification failure |
| `Timeout` | Client-side timeout | request exceeded `timeout_ms` |
| `Cancelled` | External cancellation signal fired | a passed `CancellationToken` was cancelled |
| `Unknown` | Any other network-level error | unclassifiable failure, and all business errors |

### Methods

```rust
impl ErrorKind {
    /// Stable lowercase identifier, useful for logs and metrics. e.g. "dns", "tcp", "tls".
    pub fn as_str(&self) -> &'static str;

    /// Human-readable English description, matching the TS wording.
    pub fn description(&self) -> &'static str;
}

impl std::fmt::Display for ErrorKind { /* delegates to as_str */ }
```

Example:

```rust
use ilink_sdk::error::ErrorKind;
assert_eq!(ErrorKind::Dns.as_str(), "dns");
assert_eq!(ErrorKind::Tls.description(), "TLS handshake error");
```

## `struct Error`

The single error type returned by the SDK.

```rust
#[derive(Debug, Clone)]
pub struct Error {
    kind: ErrorKind,
    message: String,
    external_cancel: bool,
}
```

### Construction and conversion

```rust
impl Error {
    /// Construct an error from a category and message.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self;

    /// Convenience constructor: `ErrorKind::Unknown`.
    pub fn unknown(message: impl Into<String>) -> Self;

    /// Mark this error as caused by an external cancellation (vs. an internal timeout).
    pub fn with_external_cancel(mut self) -> Self;
}
```

`Error` also implements `From` for common underlying error types, so the `?`
operator works directly with them:

| Source type | Conversion |
| --- | --- |
| `std::io::Error` | `TimedOut`→`Timeout`; `ConnectionRefused`/`ConnectionReset`/`NotConnected`/`NetworkDown`→`Tcp`; else by string |
| `serde_json::Error` | `Unknown` (wrapped as `"json: ..."`) |
| `reqwest::Error` | see `from_reqwest` / `from_reqwest_wrap` |
| `reqwest::header::InvalidHeaderValue` / `ToStrError` | `Unknown` |
| `rustls::Error` | `Unknown` (wrapped as `"tls: ..."`) |
| `url::ParseError` | `Unknown` |
| `hex::FromHexError` | `Unknown` |
| `base64::DecodeError` | `Unknown` |
| `rusqlite::Error` | `Unknown` (wrapped as `"sqlite: ..."`) |

### Predicate methods

```rust
impl Error {
    /// The failure category.
    pub fn kind(&self) -> ErrorKind;

    /// Whether this error was caused by an external cancellation signal.
    pub fn is_external_cancel(&self) -> bool;

    /// Whether this is a client timeout **or** an external cancellation. Matches the TS AbortError check.
    pub fn is_abort(&self) -> bool;

    /// Whether this looks like a retryable connectivity problem (vs. protocol/business error).
    pub fn is_transport(&self) -> bool;
}
```

`is_abort()` is the key predicate the long-poll loop uses to decide "silently
retry": `get_updates` returns an empty `ret:0` response (no error) on an internal
timeout or external cancellation, but on other paths an `is_abort()`-true error
can likewise be treated as "keep polling".

Example — branching by category (from the top-level README):

```rust
match client.sender().send_text(to, text, &opts).await {
    Ok(result) => println!("sent {}", result.message_id),
    Err(err) => match err.kind() {
        ErrorKind::Timeout | ErrorKind::Cancelled => { /* retry or stop cleanly */ }
        ErrorKind::Dns | ErrorKind::Tcp => { /* back off and retry */ }
        _ => { /* log and surface to the user */ }
    },
}
```

## Classifiers

The transport layer uses two classifiers internally; callers rarely need them
directly, but understanding them helps predict error categories:

```rust
impl Error {
    /// Classify from any `std::error::Error` chain, walking the Display + source chain
    /// and matching the known error codes of TypeScript's `classifyFetchError`.
    pub fn classify<E: std::error::Error>(err: &E) -> Self;

    /// Classify from `reqwest::Error`, preserving the Timeout/Cancelled distinction reqwest already determined.
    pub fn from_reqwest_wrap(err: reqwest::Error) -> Self;
}
```

`classify_fetch_error_string` is the core string matcher (not public; behavior):

- contains `ENOTFOUND` / `EAI_AGAIN` / `getaddrinfo` → `Dns`
- contains `ECONNREFUSED` → `Tcp`
- contains `UND_ERR_CONNECT_TIMEOUT` / `ETIMEDOUT` / `ENETUNREACH` / `EHOSTUNREACH`
  / `operation timed out` / `timed out` → `Tcp`
- contains `UND_ERR_SOCKET` / `SSL` / `TLS` / `CERT` / `UNABLE_TO_VERIFY` /
  `DEPTH_ZERO` / `certificate` / `handshake` → `Tls`
- otherwise → `Unknown`

## Design notes and caveats

1. **Business errors are not split out in `ErrorKind`.** `send_message` returns
   `ErrorKind::Unknown` when `ret != 0`, but `message` contains `ret=<code>
   errmsg=...`. To branch on a business code, read the response body (e.g.
   `SendMessageResp`) after the call rather than relying on `ErrorKind`.
2. **`is_transport()` is not "not `Unknown`".** `is_transport()` is defined as
   "not `Unknown` or `Cancelled`, or triggered by an external cancel" — so
   `Unknown` **can** be judged retryable (when `external_cancel` is true). For
   backoff policy, prefer `is_abort()` + `kind()` over `is_transport()`.
3. **`is_abort()` is the monitor-loop predicate.** Its semantics match the
   TypeScript `AbortError` check: client timeout or external cancellation.
4. **Errors are safe to print and serialize.** `Error` implements `Display` and
   `std::error::Error`; `message` may contain part of a response body (already
   truncated/redacted via `redact_body`).
