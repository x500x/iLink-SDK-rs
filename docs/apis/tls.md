# TLS config API

The TLS module ([`../../src/tls.rs`](../../src/tls.rs)) handles the transport-layer
security config used by the HTTP client. Server certificates are verified by
`rustls-platform-verifier` against the **operating system trust store**, so it
matches the browser's root certificates and correctly recognizes enterprise MITM
proxies; cipher suites come from `reqwest`'s built-in `ring` provider.

## `default_tls_config`

```rust
/// Build the default rustls ClientConfig (using platform cert verification).
pub fn default_tls_config() -> Result<ClientConfig, RustlsError>;
```

`WeixinApiClient` and the CDN upload/download call it when constructing a
`reqwest::Client`:

```rust
let client = reqwest::Client::builder()
    .use_preconfigured_tls(crate::tls::default_tls_config()?)
    .build()?;
```

Callers rarely need to call it directly, unless they build their own `reqwest::Client`
and want to reuse the SDK's verification policy.

## `DangerIgnoreVerifier`

```rust
/// A verifier that accepts any certificate (test-only).
#[derive(Debug)]
pub struct DangerIgnoreVerifier;

impl rustls::client::danger::ServerCertVerifier for DangerIgnoreVerifier { /* always Ok */ }
```

This is a **test-only** "accept-all" verifier, equivalent to the TypeScript test's
`NODE_TLS_REJECT_UNAUTHORIZED=0`. Production code must **never** use it, as it
completely disables TLS verification.

Example (test stub, illustrative):

```rust,no_run
# #[cfg(test)]
# mod tests {
use openclaw_weixin::tls::DangerIgnoreVerifier;
use rustls::crypto::ring::default_provider;
use rustls::{ClientConfig, RootCertStore};

# fn make_danger_config() -> ClientConfig {
let mut cfg = ClientConfig::builder()
    .with_safe_defaults()
    .with_custom_certificate_verifier(std::sync::Arc::new(DangerIgnoreVerifier))
    .with_no_client_auth();
cfg
# }
# }
```

> Note: `DangerIgnoreVerifier` is listed here only because the SDK uses it
> internally for integration tests, to document the TLS verification replacement
> point and to emphasize "never disable verification in production".

## Design notes and caveats

1. **Defaults to trusting the system root store.** Via `rustls-platform-verifier`,
   the SDK automatically respects the OS trust store and enterprise proxy
   certificates, with no need to manually configure `ca-certificates`.
2. **Cipher suites come from `ring`.** `reqwest` enables `rustls` + `ring` by
   default; TLS 1.2/1.3 is provided by `default_provider()`, and the SDK explicitly
   enables the `tls12` feature in `Cargo.toml`.
3. **`default_tls_config` failure is a hard error.** It returns `Result`; if the
   platform verifier fails to initialize (very rare), `WeixinApiClient::new` fails
   with it — expected, not something to silently swallow.
4. **Never disable verification in production.** `DangerIgnoreVerifier` is test-only;
   any production path using it exposes communication to man-in-the-middle attacks.
5. **Aligned with `reqwest`'s TLS feature.** The SDK's `reqwest` dependency enables
   `rustls-tls` (not `native-tls`), so the entire crate's TLS stack is rustls, not
   OpenSSL / SChannel.
