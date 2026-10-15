# Low-level API client API

The low-level API client is the core of the SDK's communication with the Weixin
(iLink) backend, in [`../../src/api/`](../../src/api/). It maps each SDK operation
to one CGI endpoint. Most applications use the high-level
[`WeixinClient`](client.md) instead, but the raw client is valuable for:

- custom authentication flows or lifecycle management
- unit tests: inject a mock via `with_transport`
- reaching a specific endpoint outside the gateway/proxy/instrumentation
- reading the raw `get_updates` long-poll cursor and driving the loop yourself

## Module composition

| Submodule | File | Responsibility |
| --- | --- | --- |
| `client` | [`../../src/api/client.rs`](../../src/api/client.rs) | HTTP client and request/timeout/header management |
| `types` | [`../../src/api/types.rs`](../../src/api/types.rs) | Wire types (request/response structs) |
| `session_guard` | [`../../src/api/session_guard.rs`](../../src/api/session_guard.rs) | Account pause after stale token |

## `WeixinApiClient`

```rust
#[derive(Clone)]
pub struct WeixinApiClient;   // holds opts, transport, channel_version, bot_agent internally
```

### Construction

```rust
impl WeixinApiClient {
    /// Construct with the default reqwest transport.
    pub fn new(opts: WeixinApiOptions) -> Result<Self>;

    /// Inject a custom transport (tests, proxy, instrumentation). Must be
    /// `Arc<dyn HttpTransport>`.
    pub fn with_transport(opts: WeixinApiOptions, transport: Arc<dyn HttpTransport>) -> Self;

    /// Override the `bot_agent` declaration (sanitized via sanitize_bot_agent).
    pub fn with_bot_agent(mut self, bot_agent: impl AsRef<str>) -> Self;
}
```

The client is `Clone` because the underlying transport is shared behind `Arc`;
cloning is cheap.

### Reading configuration

```rust
impl WeixinApiClient {
    pub fn options(&self) -> &WeixinApiOptions;  // underlying options
    pub fn base_info(&self) -> BaseInfo;          // current base_info payload
}
```

`base_info()` returns `{ channel_version, bot_agent }`, attached automatically to
every request.

### Endpoint methods

Each method maps to one iLink CGI endpoint. All methods are `async`.

```rust
impl WeixinApiClient {
    // Long-poll for inbound messages
    pub async fn get_updates(
        &self,
        req: &GetUpdatesReq,
        timeout_ms: Option<u64>,
        cancel: Option<CancellationToken>,
    ) -> Result<GetUpdatesResp>;

    // Send a message
    pub async fn send_message(&self, req: &SendMessageReq) -> Result<SendMessageResp>;

    // Pre-sign a CDN upload URL
    pub async fn get_upload_url(&self, req: &GetUploadUrlReq) -> Result<GetUploadUrlResp>;

    // Fetch bot config (includes typing_ticket)
    pub async fn get_config(
        &self,
        ilink_user_id: &str,
        context_token: Option<&str>,
    ) -> Result<GetConfigResp>;

    // Send / cancel typing indicator
    pub async fn send_typing(&self, req: &SendTypingReq) -> Result<()>;

    // Lifecycle heartbeat: stop
    pub async fn notify_stop(&self) -> Result<NotifyStopResp>;
    // Lifecycle heartbeat: start
    pub async fn notify_start(&self) -> Result<NotifyStartResp>;
}
```

### Raw helper methods (used by the login flow, also callable directly)

```rust
impl WeixinApiClient {
    /// Send a raw authenticated GET request and return text (QR status polling).
    pub async fn get_text(&self, endpoint: &str, timeout_ms: u64) -> Result<String>;

    /// Send a raw unauthenticated POST request and return text (QR fetch; no Authorization).
    pub async fn post_text_unauthenticated(
        &self,
        endpoint: &str,
        json_body: &impl Serialize,
        timeout_ms: Option<u64>,
    ) -> Result<String>;
}
```

### Behavior details

- **Auth headers.** Every authenticated POST automatically carries:
  - `iLink-App-Id: bot` (constant `ILINK_APP_ID`)
  - `iLink-App-ClientVersion`: `build_client_version(VERSION)`, e.g. `2.4.10`
    → `0x0002040A`
  - `AuthorizationType: ilink_bot_token` (constant `AUTHORIZATION_TYPE`)
  - `Authorization: Bearer <token>`
  - `X-WECHAT-UIN`: `base64(random u32)`
  - optional `SKRouteTag` (from `WeixinApiOptions::route_tag`)
  - a `base_info` body
- **Long-poll timeout/cancel is not an error.** `get_updates` on a client timeout
  or external cancellation returns an empty `ret:0` response carrying the *same*
  `get_updates_buf`, so the caller simply retries.
- **`send_message` error semantics.** A non-zero `ret` is an error; an absent
  `ret` is accepted (mirroring the TS `if (resp.ret && resp.ret !== 0)` guard).
- **HTTP status.** Non-2xx returns `ErrorKind::Unknown` with a truncated and
  redacted response body.

## `WeixinApiOptions`

```rust
#[derive(Debug, Clone)]
pub struct WeixinApiOptions {
    pub base_url: String,                  // e.g. "https://ilinkai.weixin.qq.com"
    pub token: Option<String>,             // bot token from QR login
    pub timeout_ms: Option<u64>,           // global default request timeout
    pub route_tag: Option<String>,         // forwarded as SKRouteTag
    pub cdn_base_url: Option<String>,      // used by the media pipeline
}

impl WeixinApiOptions {
    /// Base URL only, no token (for pre-login requests).
    pub fn new(base_url: impl Into<String>) -> Self;

    pub fn with_token(mut self, token: impl Into<String>) -> Self;
    pub fn with_route_tag(mut self, tag: impl Into<String>) -> Self;
    pub fn with_cdn_base_url(mut self, url: impl Into<String>) -> Self;
    pub fn with_timeout_ms(mut self, ms: u64) -> Self;   // sets the default timeout
}
```

`with_timeout_ms` sets the **global default**; each operation falls back to its
own default constant when unset (see below).

## Constants (timeouts and identifiers)

```rust
pub const ILINK_APP_ID: &str = "bot";
pub const AUTHORIZATION_TYPE: &str = "ilink_bot_token";
pub const DEFAULT_LONG_POLL_TIMEOUT_MS: u64 = 35_000;   // get_updates
pub const DEFAULT_API_TIMEOUT_MS: u64 = 15_000;          // send_message / get_upload_url
pub const DEFAULT_CONFIG_TIMEOUT_MS: u64 = 10_000;       // get_config / send_typing / notify_*
```

## `HttpTransport` and `ReqwestTransport`

```rust
#[async_trait::async_trait]
pub trait HttpTransport: Send + Sync + 'static {
    async fn request(
        &self,
        method: Method,
        url: String,
        headers: HeaderMap,
        body: Option<Vec<u8>>,
        timeout: Option<Duration>,
        cancel: Option<CancellationToken>,
    ) -> Result<(StatusCode, HeaderMap, Vec<u8>)>;
}

pub struct ReqwestTransport;   // default impl, backed by reqwest + rustls
```

Implement `HttpTransport` to inject a mock. The integration tests in
[`../../tests/api_integration.rs`](../../tests/api_integration.rs) use it to assert
on headers, bodies, timeouts, and cancellation without hitting the real backend.

Example — a mock that records all requests (illustrative):

```rust
use openclaw_weixin::api::{HttpTransport, ReqwestTransport, WeixinApiOptions, WeixinApiClient};
use std::sync::Arc;

// Wrap the default impl; replace with your own mock in tests
let transport = Arc::new(ReqwestTransport::default());
let client = WeixinApiClient::with_transport(
    WeixinApiOptions::new("https://ilinkai.weixin.qq.com"),
    transport,
);
```

## Free functions

```rust
pub fn build_client_version(version: &str) -> u32;
// "2.4.10" -> 0x0002040A
// rule: ((major & 0xFF) << 16) | ((minor & 0xFF) << 8) | (patch & 0xFF)

pub fn build_base_info() -> BaseInfo;
pub fn build_base_info_with(channel_version: &str, bot_agent: &str) -> BaseInfo;

pub fn sanitize_bot_agent(raw: Option<&str>) -> String;
// grammar: name/version [ (comment) ]; invalid tokens dropped; >256 bytes trims trailing tokens

pub fn parse_api_json<T: DeserializeOwned>(raw: &str) -> Result<T>;
pub fn quote_lossless_ids(raw: &str) -> String;
```

`parse_api_json` quotes uint64 identifier fields (`message_id`, `msg_id`,
`svr_id`) before deserializing, so values above `i64::MAX` survive losslessly as
`String`. A value that is already a string, `null`, or `true`/`false` is left
untouched (see [`api/types`](#apitypes-wire-types)).

## `api::types` wire types

Structs in [`../../src/api/types.rs`](../../src/api/types.rs) map directly to the
backend JSON protocol. All are `#[derive(Serialize, Deserialize)]` and `Default`.

### Enums

| Enum | Variants and values | Purpose |
| --- | --- | --- |
| `UploadMediaType` | `Image=1, Video=2, File=3, Voice=4` | CDN upload media type |
| `MessageType` | `None=0, User=1, Bot=2` | Message direction |
| `MessageItemType` | `None, Text, Image, Voice, File, Video, ToolCallStart, ToolCallResult` | Content item type |
| `MessageState` | `New=0, Generating=1, Finish=2` | Message state |
| `TypingStatus` | `Typing=1, Cancel=2` | Typing indicator |

Each enum has `value()` (back to `i64`) and `from_value()` (from `i64`, returning
`None` for invalid values).

### Request/response structs (core)

| Type | Notes |
| --- | --- |
| `BaseInfo` | `{ channel_version, bot_agent }` |
| `GetUploadUrlReq` / `GetUploadUrlResp` | Pre-signed upload |
| `WeixinMessage` | message body, with `message_id: Option<String>`, `item_list`, `context_token`, `run_id` |
| `MessageItem` | a single content item, with `msg_id: Option<String>`, `ref_msg`, `text_item`, `image_item`… |
| `TextItem` / `ImageItem` / `VoiceItem` / `FileItem` / `VideoItem` | each content item type |
| `CdnMedia` | `{ encrypt_query_param, aes_key, encrypt_type, full_url }` |
| `RefMessage` / `PartialText` | inline quote and its substring |
| `ToolCallStartItem` / `ToolCallResultItem` | tool-call items |
| `GetUpdatesReq` / `GetUpdatesResp` | long-poll request/response, with `get_updates_buf` cursor |
| `SendMessageReq` / `SendMessageResp` | send message |
| `SendTypingReq` / `SendTypingResp` | typing indicator |
| `GetConfigResp` | includes `typing_ticket` |
| `NotifyStopReq/Resp` / `NotifyStartReq/Resp` | lifecycle heartbeats |

### Important `impl` helpers

```rust
impl GetUpdatesResp {
    pub fn is_api_error(&self) -> bool;            // ret != 0 || errcode != 0
    pub fn is_stale_token(&self, stale_errcode: i64) -> bool;
}

impl SendMessageResp {
    pub fn is_error(&self) -> bool;                // Some(ret) && ret != 0 (absent ret is OK)
}

impl GetConfigResp {
    pub fn is_ok(&self) -> bool;                   // ret == Some(0)
}
```

### Constants

```rust
pub const DEFAULT_BOT_AGENT: &str = "OpenClaw";
pub const BOT_AGENT_MAX_LEN: usize = 256;
```

> On lossless uint64: `WeixinMessage.message_id` and `MessageItem.msg_id` /
> `svr_id` are JSON numbers on the wire that can exceed `i64::MAX`. The SDK quotes
> them via `quote_lossless_ids` before parsing, so Rust keeps them as `String`
> without silent truncation or overflow.

## `session_guard` submodule

[`../../src/api/session_guard.rs`](../../src/api/session_guard.rs): when `getUpdates`
returns `errcode == -14` (stale token), it pauses all API calls for that account
for one hour.

```rust
pub const STALE_TOKEN_ERRCODE: i64 = -14;
pub const SESSION_PAUSE_DURATION: Duration = Duration::from_secs(60 * 60);

pub fn pause_session(account_id: &str);
pub fn is_session_paused(account_id: &str) -> bool;
pub fn get_remaining_pause_ms(account_id: &str) -> u128;
pub fn assert_session_active(account_id: &str) -> Result<()>;
pub fn resume_session(account_id: &str);
pub fn reset_session_pauses();          // tests only

pub struct SessionGuard { /* bound to one account_id */ }
impl SessionGuard {
    pub fn new(account_id: impl Into<String>) -> Self;
    pub fn account_id(&self) -> &str;
    pub fn pause(&self);
    pub fn resume(&self);
    pub fn is_paused(&self) -> bool;
    pub fn remaining_ms(&self) -> u128;
    pub fn assert_active(&self) -> Result<()>;
}
```

- After `pause_session`, all API requests for that account should be blocked by
  `assert_session_active`, returning `ErrorKind::Unknown` (message includes
  remaining minutes and `STALE_TOKEN_ERRCODE`).
- Accounts are independent; expired entries are lazily cleaned on query.
- After a successful re-login, call `resume_session` to lift the pause early.

## Usage examples

### Driving the long poll directly

```rust,no_run
use openclaw_weixin::api::{WeixinApiClient, WeixinApiOptions, GetUpdatesReq};
use openclaw_weixin::error::{ErrorKind, Result};

async fn loop_updates(api: &WeixinApiClient, mut buf: String) -> Result<()> {
    loop {
        let req = GetUpdatesReq { get_updates_buf: Some(buf.clone()), ..Default::default() };
        let resp = api.get_updates(&req, None, None).await?;
        // Handle business errors (e.g. stale token -14) here
        if resp.is_stale_token(-14) {
            return Err(openclaw_weixin::error::Error::new(
                ErrorKind::Unknown, "token stale"));
        }
        if let Some(msgs) = resp.msgs.as_ref() {
            for m in msgs { /* process message */ }
        }
        if let Some(next) = resp.get_updates_buf.clone() {
            buf = next;
        }
    }
}
```

### Injecting a mock transport for assertions (illustrative)

```rust,no_run
use openclaw_weixin::api::{HttpTransport, WeixinApiOptions, WeixinApiClient};
use std::sync::Arc;

// Implement HttpTransport::request to return (StatusCode, HeaderMap, Vec<u8>)
// and assert on headers/bodies in tests.
let client = WeixinApiClient::with_transport(
    WeixinApiOptions::new("https://ilinkai.weixin.qq.com").with_token("t"),
    Arc::new(MyMockTransport),
);
```

## Design notes and caveats

1. **Prefer the high-level `WeixinClient`.** Unless you need precise cursor
   control, a custom transport, or lifecycle management, `WeixinClient` already
   wraps account resolution, QR login, and the sender.
2. **`get_updates` does not treat timeout as an error.** By design, a timeout or
   cancellation returns an empty `ret:0`; the caller keeps polling. To truly
   stop, pass a `CancellationToken` or use a loop condition that returns an error.
3. **`send_message` "absent ret" semantics.** Do not assume `SendMessageResp.ret`
   is always present; treat it as success when absent, and error only when
   `Some(r)` with `r != 0`.
4. **`base_url` cannot be empty.** An empty or whitespace-only string returns
   `ErrorKind::Unknown`; construction normalizes a trailing `/` where possible.
5. **`bot_agent` is sanitized.** An invalid UA string is cleaned; a result over
   256 bytes drops trailing tokens; an empty result falls back to `OpenClaw`.
