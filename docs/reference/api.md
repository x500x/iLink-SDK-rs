# API handbook

This page is the authoritative reference for everything the `ilink_sdk`
crate exposes publicly. It is generated from the crate's module structure and
is intended to be read alongside the source files it links to.

> The crate has **no embedded HTTP server and no gateway dependency**. Every
> entry below is a function, type, or constant you call or construct directly.
> For narrative guides (login flow, messaging, media), see the
> [docs/README.md](README.md) index.

## Crate root

Crate name: `ilink_sdk` (library), version `ilink_sdk::VERSION`
(`env!("CARGO_PKG_VERSION")`).

Top-level modules:

| Module | Purpose |
| --- | --- |
| [`error`](#errormod) | `Error`, `ErrorKind`, `Result` |
| [`api`](#apimod) | Low-level HTTP client, wire types, session guard |
| [`client`](#clientmod) | High-level `WeixinClient` / `WeixinClientBuilder` |
| [`messaging`](#messagingmod) | QR login, outbound, inbound context, typing, quote cache |
| [`cdn`](#cdnmod) | AES-128-ECB crypto + CDN upload/download |
| [`storage`](#storagemod) | Account store, state dir, sync cursor, config |
| [`media`](#mediamod) | MIME detection |
| [`util`](#utilmod) | IDs, redaction, time |
| [`logging`](#loggingmod) | `tracing` subscriber helpers (CLI feature) |
| [`tls`](#tlsmod) | rustls configuration |
| `ffi` | C ABI (`ilink_call`/`ilink_version`/`ilink_string_free`) — see [ffi.md](ffi.md) |

### `prelude`

The convenience re-export module gathers the most common types so callers can
`use ilink_sdk::prelude::*;`.

```rust
pub mod prelude {
    pub use crate::api::types::*;
    pub use crate::api::{WeixinApiClient, WeixinApiOptions};
    pub use crate::client::{WeixinClient, WeixinClientBuilder, WeixinConfig};
    pub use crate::error::{Error, ErrorKind, Result};
    pub use crate::messaging::auth::QrLogin;
    pub use crate::storage::{CDN_BASE_URL, DEFAULT_BASE_URL};
}
```

> ⚠️ `prelude` re-exports `*` of `api::types`, so names like `WeixinMessage`
> and `MessageItem` are in scope. If you need to avoid a clash, import
> `api::types` explicitly instead.

---

## `error` module

[`src/error.rs`](../src/error.rs)

The crate-wide fallible type. Every fallible call returns `Result<T>` with the
crate `Error`.

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

### `enum ErrorKind`

Classification of a transport-level failure. Business-level errors (HTTP 200
with `ret != 0`) surface as `ErrorKind::Unknown`, not as a dedicated variant.

| Variant | Meaning |
| --- | --- |
| `Dns` | DNS resolution failure |
| `Tcp` | TCP connect / reset failure |
| `Tls` | TLS handshake error |
| `Timeout` | Client-side timeout |
| `Cancelled` | External abort signal fired |
| `Unknown` | Any other network-level error |

```rust
impl ErrorKind {
    pub fn as_str(&self) -> &'static str;        // "dns", "tcp", ...
    pub fn description(&self) -> &'static str;   // human-readable English
}
impl fmt::Display for ErrorKind { /* ... */ }
```

### `struct Error`

```rust
impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self;
    pub fn unknown(message: impl Into<String>) -> Self;
    pub fn with_external_cancel(mut self) -> Self;   // mark as external abort

    pub fn kind(&self) -> ErrorKind;
    pub fn is_external_cancel(&self) -> bool;
    pub fn is_abort(&self) -> bool;        // Timeout OR Cancelled
    pub fn is_transport(&self) -> bool;    // retryable connectivity error
}
```

`Error` implements `Display`, `std::error::Error`, and `From` for
`std::io::Error`, `serde_json::Error`, `reqwest::Error`,
`reqwest::header::{InvalidHeaderValue, ToStrError}`, `rustls::Error`,
`url::ParseError`, `hex::FromHexError`, `base64::DecodeError`, and
`rusqlite::Error`.

`Error::from_reqwest_wrap` is the main classifier used by the transport layer:
it preserves the `Timeout`/`Cancelled` distinction that `reqwest` already
determined and otherwise delegates to `classify_fetch_error_string` (a port of
the TypeScript `classifyFetchError` string matcher over `ENOTFOUND`,
`ECONNREFUSED`, `SSL`/`CERT`/`TLS`, etc.).

---

## `api` module

[`src/api/mod.rs`](../src/api/mod.rs) · [`src/api/client.rs`](../src/api/client.rs)
· [`src/api/types.rs`](../src/api/types.rs) · [`src/api/session_guard.rs`](../src/api/session_guard.rs)

### `WeixinApiClient`

The low-level async HTTP client mapping each SDK operation to one iLink CGI
endpoint. Most applications use [`WeixinClient`](#weixinclient) instead, but
the raw client is available for custom flows, testing, and instrumentation.

```rust
impl WeixinApiClient {
    pub fn new(opts: WeixinApiOptions) -> Result<Self>;
    pub fn with_transport(opts: WeixinApiOptions, transport: Arc<dyn HttpTransport>) -> Self;
    pub fn with_bot_agent(mut self, bot_agent: impl AsRef<str>) -> Self;

    pub fn options(&self) -> &WeixinApiOptions;
    pub fn base_info(&self) -> BaseInfo;     // current base_info payload

    pub async fn get_updates(
        &self,
        req: &GetUpdatesReq,
        timeout_ms: Option<u64>,
        cancel: Option<CancellationToken>,
    ) -> Result<GetUpdatesResp>;

    pub async fn send_message(&self, req: &SendMessageReq) -> Result<SendMessageResp>;
    pub async fn get_upload_url(&self, req: &GetUploadUrlReq) -> Result<GetUploadUrlResp>;
    pub async fn get_config(
        &self,
        ilink_user_id: &str,
        context_token: Option<&str>,
    ) -> Result<GetConfigResp>;
    pub async fn send_typing(&self, req: &SendTypingReq) -> Result<()>;
    pub async fn notify_stop(&self) -> Result<NotifyStopResp>;
    pub async fn notify_start(&self) -> Result<NotifyStartResp>;

    // Raw helpers (used internally by the QR login flow):
    pub async fn get_text(&self, endpoint: &str, timeout_ms: u64) -> Result<String>;
    pub async fn post_text_unauthenticated(
        &self,
        endpoint: &str,
        json_body: &impl Serialize,
        timeout_ms: Option<u64>,
    ) -> Result<String>;
}
```

Behaviour notes:

- The client is `Clone` (the transport is shared behind `Arc`).
- `get_updates` on a client-side timeout **or** external cancellation returns
  an empty `ret: 0` response carrying the *same* `get_updates_buf`, so the
  caller simply retries — it does not error.
- `send_message` treats a non-zero `ret` as an error; an absent `ret` is accepted.
- Every authenticated POST carries `iLink-App-Id: bot`,
  `iLink-App-ClientVersion`, `AuthorizationType: ilink_bot_token`,
  `Authorization: Bearer <token>`, `X-WECHAT-UIN`, optional `SKRouteTag`, and a
  `base_info` body.

### `WeixinApiOptions`

```rust
impl WeixinApiOptions {
    pub fn new(base_url: impl Into<String>) -> Self;        // no token
    pub fn with_token(mut self, token: impl Into<String>) -> Self;
    pub fn with_route_tag(mut self, tag: impl Into<String>) -> Self;
    pub fn with_cdn_base_url(mut self, url: impl Into<String>) -> Self;
    pub fn with_timeout_ms(mut self, ms: u64) -> Self;      // global request default
}

pub struct WeixinApiOptions {
    pub base_url: String,
    pub token: Option<String>,
    pub timeout_ms: Option<u64>,
    pub route_tag: Option<String>,
    pub cdn_base_url: Option<String>,
}
```

`with_timeout_ms` sets the *default* per-call timeout; individual operations
fall back to their own default constants (below) when unset.

### Constants

```rust
pub const ILINK_APP_ID: &str = "bot";
pub const AUTHORIZATION_TYPE: &str = "ilink_bot_token";
pub const DEFAULT_LONG_POLL_TIMEOUT_MS: u64 = 35_000;
pub const DEFAULT_API_TIMEOUT_MS: u64 = 15_000;
pub const DEFAULT_CONFIG_TIMEOUT_MS: u64 = 10_000;
```

### `HttpTransport` / `ReqwestTransport`

A trait so the client can be unit-tested without touching the network.

```rust
#[async_trait]
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

pub struct ReqwestTransport;   // default, backed by reqwest + rustls
```

### Free functions (re-exported by `api`)

```rust
pub fn build_client_version(version: &str) -> u32;          // "2.4.10" -> 0x0002040A
pub fn build_base_info() -> BaseInfo;
pub fn build_base_info_with(channel_version: &str, bot_agent: &str) -> BaseInfo;
pub fn sanitize_bot_agent(raw: Option<&str>) -> String;     // UA-style grammar, <=256 bytes
pub fn parse_api_json<T: DeserializeOwned>(raw: &str) -> Result<T>;
pub fn quote_lossless_ids(raw: &str) -> String;
```

`parse_api_json` quotes uint64 identifier fields (`message_id`, `msg_id`,
`svr_id`) before deserializing so values above `i64::MAX` survive losslessly as
`String`. A value that is already a string, `null`, or `true`/`false` is left
untouched.

### `session_guard` submodule

[`src/api/session_guard.rs`](../src/api/session_guard.rs) — pauses all API
calls for an account for one hour after a stale-token error (`errcode -14`).

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

### `types` submodule

[`src/api/types.rs`](../src/api/types.rs) — the wire types. All request/response
structs are `#[derive(Serialize, Deserialize)]` and `Default`. The key shapes:

| Type | Notes |
| --- | --- |
| `BaseInfo` | `{ channel_version, bot_agent }` |
| `UploadMediaType` | `Image=1, Video=2, File=3, Voice=4`; `value()`, `from_value()` |
| `GetUploadUrlReq` / `GetUploadUrlResp` | CDN pre-sign |
| `MessageType` | `None=0, User=1, Bot=2` |
| `MessageItemType` | `None, Text, Image, Voice, File, Video, ToolCallStart, ToolCallResult` |
| `MessageState` | `New=0, Generating=1, Finish=2` |
| `TextItem`, `ImageItem`, `VoiceItem`, `FileItem`, `VideoItem` | media items |
| `CdnMedia` | `encrypt_query_param`, `aes_key`, `encrypt_type`, `full_url` |
| `RefMessage`, `PartialText` | inline quote |
| `ToolCallStartItem`, `ToolCallResultItem` | tool-call items |
| `MessageItem` | one content item; `item_list` of `WeixinMessage` |
| `WeixinMessage` | `seq`, `message_id` (String), ids, `item_list`, `context_token`, `run_id` |
| `GetUpdatesReq` / `GetUpdatesResp` | long-poll cursor `get_updates_buf` |
| `SendMessageReq` / `SendMessageResp` | wraps a `WeixinMessage` |
| `TypingStatus` | `Typing=1, Cancel=2` |
| `SendTypingReq` / `SendTypingResp` | typing indicator |
| `GetConfigResp` | includes `typing_ticket` |
| `NotifyStopReq/Resp`, `NotifyStartReq/Resp` | lifecycle pings |

Important `impl` helpers:

```rust
impl GetUpdatesResp {
    pub fn is_api_error(&self) -> bool;            // ret != 0 || errcode != 0
    pub fn is_stale_token(&self, stale_errcode: i64) -> bool;
}
impl SendMessageResp {
    pub fn is_error(&self) -> bool;                // Some(ret) && ret != 0
}
impl GetConfigResp {
    pub fn is_ok(&self) -> bool;                   // ret == Some(0)
}
```

Constants: `DEFAULT_BOT_AGENT = "OpenClaw"`, `BOT_AGENT_MAX_LEN = 256`.

---

## `client` module

[`src/client/mod.rs`](../src/client/mod.rs)

### `WeixinConfig`

```rust
#[derive(Debug, Clone, Default)]
pub struct WeixinConfig {
    pub account_id: Option<String>,
    pub base_url: Option<String>,
    pub cdn_base_url: Option<String>,
    pub route_tag: Option<String>,
    pub bot_agent: Option<String>,
    pub timeout_ms: Option<u64>,
}
```

### `WeixinClientBuilder`

```rust
impl WeixinClientBuilder {
    pub fn new() -> Self;
    pub fn account_id(mut self, id: impl Into<String>) -> Self;
    pub fn account_id_opt(mut self, id: Option<String>) -> Self;
    pub fn base_url(mut self, url: impl Into<String>) -> Self;
    pub fn base_url_opt(mut self, url: Option<String>) -> Self;
    pub fn cdn_base_url(mut self, url: impl Into<String>) -> Self;
    pub fn cdn_base_url_opt(mut self, url: Option<String>) -> Self;
    pub fn route_tag(mut self, tag: impl Into<String>) -> Self;
    pub fn bot_agent(mut self, agent: impl Into<String>) -> Self;
    pub fn timeout_ms(mut self, ms: u64) -> Self;
    pub async fn build(self) -> Result<WeixinClient>;
}
```

When `account_id` is unset, `build` falls back to the single registered
account and errors if zero or several are present.

### `WeixinClient`

```rust
impl WeixinClient {
    pub async fn from_config(config: WeixinConfig) -> Result<Self>;
    pub async fn from_session(session: QrSession, bot_agent: Option<String>) -> Result<Self>;
    pub async fn save_and_load(account_id: &str, update: AccountUpdate) -> Result<Self>;

    pub fn account(&self) -> &StoredAccount;
    pub fn account_id(&self) -> &str;
    pub fn token(&self) -> Option<&str>;
    pub fn api(&self) -> &WeixinApiClient;
    pub fn qr_login(&self) -> QrLogin<'_>;
    pub fn sender(&self) -> MessageSender<'_>;

    pub fn enable_quote_cache(&mut self) -> Result<()>;
    pub fn quote_store(&self) -> Option<&Arc<QuoteStore>>;

    pub async fn notify_start(&self) -> Result<()>;
    pub async fn notify_stop(&self) -> Result<()>;
}
```

`WeixinClient` is `Clone` but **not** `Sync` (the `QuoteStore` is `Send` but
not `Sync`); it is meant to be clone-shared, not shared behind a `&` across
tasks. `from_session` builds a client directly from a `QrSession` without
touching the account store — useful for bespoke persistence.

---

## `messaging` module

[`src/messaging/mod.rs`](../src/messaging/mod.rs)

### `auth` submodule — QR login

[`src/messaging/auth.rs`](../src/messaging/auth.rs)

```rust
impl QrLogin<'_> {
    pub fn new(client: &WeixinApiClient) -> Self;
    pub fn with_bot_type(mut self, bot_type: impl Into<String>) -> Self;
    pub async fn start(&self, session_key: Option<&str>) -> Result<QrStartResult>;
    pub async fn fetch_qr_code(&self) -> Result<QrCodeResponse>;
    pub async fn poll(&self, session_key: &str) -> Result<PollOutcome>;
    pub fn submit_verify_code(&self, session_key: &str, code: &str);
    pub fn remove(&self, session_key: &str);
    pub fn has_session(session_key: &str) -> bool;
    pub async fn wait<F, Fut, G, GFut>(
        &self,
        session_key: &str,
        timeout: Duration,
        on_verification_required: F,   // fn(bool) -> Fut<Output = Result<String>>
        on_refresh: G,                 // fn(&QrStartResult) -> GFut<Output = ()>
    ) -> Result<QrWaitResult>;
    pub async fn refresh(&self, session_key: &str) -> Result<QrStartResult>;
}

pub fn local_token_list() -> Vec<String>;
```

Key types:

```rust
pub struct QrStartResult { pub qrcode: String, pub qrcode_url: String, pub session_key: String }
pub struct QrSession { pub token: String, pub bot_id: String, pub base_url: String, pub user_id: String }
pub struct QrWaitResult { pub connected: bool, pub already_connected: bool, pub session: Option<QrSession>, pub message: String }

pub enum PollOutcome {
    Continue,
    NeedsVerification { retry: bool },
    Expired,
    VerifyCodeBlocked,
    AlreadyConnected,
    Done(QrSession),
}

pub enum QrStatus { Wait, Scaned, Confirmed, Expired, ScanButRedirect, NeedVerifyCode, VerifyCodeBlocked, BindedRedirect, Unknown }
impl QrStatus { pub fn parse(raw: &str) -> Self; }

pub const DEFAULT_ILINK_BOT_TYPE: &str = "3";
pub const QR_LONG_POLL_TIMEOUT_MS: u64 = 35_000;
pub const MAX_QR_REFRESH_COUNT: usize = 3;
```

`wait`'s `on_verification_required` returns the code the user typed; an empty
string is treated as an incorrect code. `on_refresh` is invoked whenever the QR
code is replaced so the host can re-render it.

### `outbound` submodule — sending

[`src/messaging/outbound.rs`](../src/messaging/outbound.rs)

```rust
#[derive(Debug, Clone, Default)]
pub struct SendOpts {
    pub context_token: Option<String>,
    pub run_id: Option<String>,
}

pub struct SendResult {
    pub message_id: String,             // client-generated id
    pub server_message_id: Option<String>,
}

pub struct MessageSender<'a> { /* bound to an api client + account_id */ }
impl<'a> MessageSender<'a> {
    pub fn new(api: &'a WeixinApiClient, account_id: impl Into<String>) -> Self;
    pub fn account_id(&self) -> &str;

    pub async fn send_text(&self, to: &str, text: &str, opts: &SendOpts) -> Result<SendResult>;
    pub async fn send_item(&self, to: &str, item: MessageItem, opts: &SendOpts) -> Result<SendResult>;
    pub async fn send_file(&self, to: &str, path: &Path, caption: &str, opts: &SendOpts, cdn_base_url: &str) -> Result<SendResult>;
    pub async fn send_bytes(&self, to: &str, data: &[u8], file_name: &str, caption: &str, opts: &SendOpts, cdn_base_url: &str) -> Result<SendResult>;
    pub async fn send_image_bytes(&self, to: &str, data: &[u8], extension: &str, caption: &str, opts: &SendOpts, cdn_base_url: &str) -> Result<SendResult>;
}
```

`send_file` routes by MIME type (video / image / file), uploading first via the
CDN pipeline and then sending a caption + media item as separate requests. A
missing `context_token` is logged as a warning but does **not** block the send.

### `typing` submodule

[`src/messaging/typing.rs`](../src/messaging/typing.rs)

```rust
pub enum Typing { Typing, Cancel }
impl From<Typing> for TypingStatus { /* 1 / 2 */ }

pub async fn send_typing(
    api: &WeixinApiClient,
    ilink_user_id: &str,
    ticket: &str,
    status: Typing,
) -> Result<()>;             // empty ticket is a no-op (no network)

pub struct TypingHelper<'a> { /* api + user_id + ticket */ }
impl<'a> TypingHelper<'a> {
    pub fn new(api: &'a WeixinApiClient, user_id: impl Into<String>, ticket: impl Into<String>) -> Self;
    pub fn enabled(&self) -> bool;
    pub async fn start(&self) -> Result<()>;
    pub async fn stop(&self) -> Result<()>;
}
```

### `context` submodule — inbound context

[`src/messaging/context.rs`](../src/messaging/context.rs)

```rust
pub struct ContextTokenStore { /* memory + JSON persistence */ }
impl ContextTokenStore {
    pub fn new() -> Self;
    pub fn with_dir(dir: impl Into<PathBuf>) -> Self;
    pub fn set(&mut self, account_id: &str, user_id: &str, token: &str);
    pub fn get(&self, account_id: &str, user_id: &str) -> Option<&str>;
    pub fn accounts_for_user<'a>(&self, account_ids: &'a [String], user_id: &str) -> Vec<&'a str>;
    pub fn clear_account(&mut self, account_id: &str);
    pub fn restore(&mut self, account_id: &str) -> usize;   // returns count loaded
}

pub struct MsgContext { /* body, from, to, account_id, message_sid, context_token, media*, reply_to_* ... */ }
pub struct MediaEntry { pub path: String, pub content_type: Option<String>, pub file_name: Option<String>, pub message_id: Option<String> }

pub fn is_media_item(item: &MessageItem) -> bool;
pub fn media_label(item_type: Option<i64>) -> &'static str;   // "[图片]", "[视频]", ...
pub fn message_id(msg: &WeixinMessage) -> Option<String>;
pub fn body_from_items(items: &[MessageItem]) -> String;
pub fn message_to_context(msg: &WeixinMessage, account_id: &str, media: &InboundMedia) -> MsgContext;
pub fn remember_context_token(store: &mut ContextTokenStore, ctx: &MsgContext) -> bool;
```

`message_to_context` builds the host-facing `MsgContext`, resolving media paths
and inline `reply_to_*` quote fields. `InboundMedia` is defined in the
[`cdn::download`](#download-submodule) submodule.

### `quote_cache` submodule — local quote cache

[`src/messaging/quote_cache.rs`](../src/messaging/quote_cache.rs) — SQLite-backed
store for ID-only inbound quotes. Disabled when `QuoteCachePolicy::enabled` is
`false`.

```rust
pub struct QuoteCachePolicy {
    pub enabled: bool,
    pub retention_ms: u64,
    pub max_messages_per_account: u64,
    pub media_retention_ms: u64,
    pub max_media_bytes_per_account: u64,
    pub max_single_media_bytes: u64,
}
impl Default for QuoteCachePolicy { /* 30d text, 7d media, 10k msgs, 256 MiB, 25 MiB */ }

pub enum Direction { #[default] Inbound, Outbound }
impl Direction { fn as_str(self) -> &'static str; fn from_str(raw: &str) -> Self; }

pub struct QuoteMessageInput {
    pub account_id: String,
    pub conversation_id: String,
    pub message_id: String,
    pub direction: Direction,
    pub body: String,
    pub source_media_path: Option<String>,
    pub media_mime: Option<String>,
    pub media_name: Option<String>,
    pub created_at: i64,
}

pub struct QuoteMessageRecord { /* same fields as input + created_at */ }

pub struct QuoteStore { /* SQLite + managed media root */ }
impl QuoteStore {
    pub fn open(policy: QuoteCachePolicy) -> Result<Option<QuoteStore>>;
    pub fn open_default() -> Result<QuoteStore>;
    pub fn open_at(policy: QuoteCachePolicy, root_dir: &Path, media_root: &Path) -> Result<QuoteStore>;

    pub fn set_policy(&mut self, policy: QuoteCachePolicy);
    pub fn policy(&self) -> &QuoteCachePolicy;

    pub fn find(&self, account_id: &str, conversation_id: &str, message_id: &str) -> Option<QuoteMessageRecord>;
    pub fn put(&mut self, input: &QuoteMessageInput) -> Result<()>;
    pub fn delete_account(&mut self, account_id: &str);
    pub fn gc(&mut self);

    pub fn account_media_dir(&self, account_id: &str) -> PathBuf;
    pub fn ensure_account_media_dir(&self, account_id: &str) -> Result<PathBuf>;
}

pub fn account_media_dir_name(account_id: &str) -> String;
pub fn quote_media_subdir(account_id: &str) -> String;
```

Constants: `DEFAULT_RETENTION_DAYS = 30`, `DEFAULT_MAX_MESSAGES_PER_ACCOUNT =
10000`, `DEFAULT_MEDIA_RETENTION_DAYS = 7`, `DEFAULT_MAX_MEDIA_BYTES_PER_ACCOUNT
= 256 MiB`, `DEFAULT_MAX_SINGLE_MEDIA_BYTES = 25 MiB`, `QUOTE_MEDIA_SUBDIR =
"media/inbound/openclaw-weixin-quotes"`.

`QuoteStore::find` deletes expired records on read; `gc` drops expired messages,
trims each account to its cap, expires old media, and enforces the per-account
media budget. Managed media outside the store's root is refused.

---

## `cdn` module

[`src/cdn/mod.rs`](../src/cdn/mod.rs)

### `aes_ecb` submodule

[`src/cdn/aes_ecb.rs`](../src/cdn/aes_ecb.rs) — block-by-block AES-128-ECB
(true ECB, not CBC-with-zero-IV).

```rust
pub const BLOCK_SIZE: usize = 16;
pub fn aes_ecb_padded_size(len: usize) -> usize;          // PKCS#7 padded size
pub fn encrypt_aes_ecb(plaintext: &[u8], key: &[u8; 16]) -> Result<Vec<u8>>;
pub fn decrypt_aes_ecb(ciphertext: &[u8], key: &[u8; 16]) -> Result<Vec<u8>>;
```

### `cdn_url` submodule

[`src/cdn/cdn_url.rs`](../src/cdn/cdn_url.rs)

```rust
pub const CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";
pub const ENABLE_CDN_URL_FALLBACK: bool = true;

pub fn build_cdn_download_url(encrypted_query_param: &str, cdn_base_url: &str) -> Result<String>;
pub fn build_cdn_upload_url(cdn_base_url: &str, upload_param: &str, filekey: &str) -> Result<String>;
pub fn resolve_download_url(full_url: Option<&str>, encrypted_query_param: &str, cdn_base_url: &str, label: &str) -> Result<String>;
pub fn resolve_upload_url(upload_full_url: Option<&str>, upload_param: Option<&str>, filekey: &str, cdn_base_url: &str, label: &str) -> Result<String>;
```

`full_url` (server-provided) always wins; otherwise the URL is built from the
param when `ENABLE_CDN_URL_FALLBACK` is set.

### `download` submodule

[`src/cdn/download.rs`](../src/cdn/download.rs)

```rust
pub const MEDIA_MAX_BYTES: usize = 100 * 1024 * 1024;

pub async fn fetch_cdn_bytes(url: &str, label: &str) -> Result<Vec<u8>>;
pub fn parse_aes_key(aes_key_base64: &str, label: &str) -> Result<[u8; 16]>;
pub fn media_download_url(media: &CdnMedia, cdn_base_url: &str, label: &str) -> Result<String>;
pub async fn download_and_decrypt(media: &CdnMedia, aes_key_base64: &str, cdn_base_url: &str, label: &str) -> Result<Vec<u8>>;
pub async fn download_plain(media: &CdnMedia, cdn_base_url: &str, label: &str) -> Result<Vec<u8>>;

pub struct InboundMedia {
    pub image_path: Option<String>,
    pub voice_path: Option<String>,
    pub voice_mime: Option<String>,
    pub file_path: Option<String>,
    pub file_mime: Option<String>,
    pub video_path: Option<String>,
    pub referenced: bool,
}
impl InboundMedia { pub fn is_empty(&self) -> bool; }

pub struct MediaSink { /* dir + optional subdir */ }
impl MediaSink {
    pub fn new(dir: impl Into<PathBuf>) -> Self;
    pub fn with_subdir(mut self, subdir: impl Into<String>) -> Self;
    pub async fn save(&self, bytes: &[u8], extension: &str) -> Result<PathBuf>;
}

pub async fn download_item_media(item: &MessageItem, cdn_base_url: &str, sink: &MediaSink, label: &str) -> InboundMedia;
pub async fn download_item_to_path(item: &MessageItem, cdn_base_url: &str, dest_dir: &Path, label: &str) -> Result<Option<(PathBuf, String)>>;
pub fn outbound_kind(path: &Path) -> MediaKind;
```

`parse_aes_key` accepts two encodings seen in the wild: `base64(16 raw bytes)`
(images) and `base64(32-char hex string)` (file / voice / video). Voice items
carrying a transcript are skipped (the transcript is already the body); raw
`SILK` voice is handed back for the host to transcode.

### `upload` submodule

[`src/cdn/upload.rs`](../src/cdn/upload.rs)

```rust
pub const UPLOAD_MAX_RETRIES: usize = 3;

pub struct UploadedFile {
    pub filekey: String,
    pub download_param: String,
    pub aeskey_hex: String,
    pub file_size: u64,
    pub file_size_ciphertext: u64,
}
impl UploadedFile { pub fn aes_key_base64(&self) -> String; }

pub async fn upload_media(to_user_id: &str, file_path: &Path, media_type: UploadMediaType, api: &WeixinApiClient, cdn_base_url: &str) -> Result<UploadedFile>;
pub async fn upload_image(to_user_id: &str, file_path: &Path, api: &WeixinApiClient, cdn_base_url: &str) -> Result<UploadedFile>;
pub async fn upload_video(to_user_id: &str, file_path: &Path, api: &WeixinApiClient, cdn_base_url: &str) -> Result<UploadedFile>;
pub async fn upload_file_attachment(to_user_id: &str, file_path: &Path, file_name: &str, api: &WeixinApiClient, cdn_base_url: &str) -> Result<UploadedFile>;
pub async fn upload_buffer_to_cdn(ciphertext: &[u8], url: &str, label: &str) -> Result<String>;   // returns x-encrypted-param
pub async fn download_remote_to_temp(url: &str, dest_dir: &Path) -> Result<PathBuf>;

pub fn backoff_delay(attempt: usize) -> Duration;   // 250ms, doubling, capped at 4s
```

HTTP 4xx aborts immediately; other failures retry up to `UPLOAD_MAX_RETRIES`.

---

## `storage` module

[`src/storage/mod.rs`](../src/storage/mod.rs)

### `accounts` submodule

[`src/storage/accounts.rs`](../src/storage/accounts.rs)

```rust
pub const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
pub const CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";

pub struct AccountData { pub token, saved_at, base_url, user_id: Option<String> }
impl AccountData { pub fn is_configured(&self) -> bool; pub fn effective_base_url(&self) -> String; }

pub struct StoredAccount { /* account_id, base_url, cdn_base_url, token, enabled, configured, name, user_id, route_tag */ }

pub struct AccountUpdate { pub token: Option<String>, pub base_url: Option<String>, pub user_id: Option<String> }

pub fn normalize_account_id(raw: &str) -> String;          // "@im.bot" -> "-im-bot"
pub fn derive_raw_account_id(normalized_id: &str) -> Option<String>;
pub fn list_indexed_account_ids() -> Vec<String>;
pub fn list_account_ids() -> Vec<String>;                  // alias
pub fn register_account_id(account_id: &str) -> Result<()>;
pub fn unregister_account_id(account_id: &str) -> Result<()>;
pub fn load_account(account_id: &str) -> Option<AccountData>;
pub fn save_account(account_id: &str, update: &AccountUpdate) -> Result<AccountData>;
pub fn clear_account(account_id: &str) -> Result<()>;
pub fn clear_stale_accounts_for_user_id(current: &str, user_id: &str, on_clear: impl FnMut(&str)) -> Result<()>;
pub fn resolve_account(account_id: &str) -> Result<StoredAccount>;
```

`save_account` overwrites `token` only when non-empty, stores `base_url` only
when non-empty, and clears `user_id` when explicitly set to `""`.

### `config` submodule

[`src/storage/config.rs`](../src/storage/config.rs)

```rust
pub const CHANNEL_CONFIG_SECTION: &str = "openclaw-weixin";

pub struct SdkConfig {
    pub accounts: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    pub reply_progress_messages: bool,
    pub quote_cache: QuoteCacheRaw,
    pub channel_config_updated_at: Option<String>,
    pub route_tag: Option<serde_json::Value>,
    pub bot_agent: Option<String>,
}
pub struct QuoteCacheRaw { /* camelCase raw values */ }
impl QuoteCacheRaw { pub fn to_policy(&self) -> QuoteCachePolicy; }   // storage::config::QuoteCachePolicy

pub struct QuoteCachePolicy { /* enabled, retention_ms, max_messages_per_account, media_retention_ms, max_media_bytes_per_account, max_single_media_bytes */ }
impl Default for QuoteCachePolicy { /* see config.rs */ }

pub fn resolve_config_path() -> PathBuf;
pub fn load_local_config() -> SdkConfig;
pub fn load_local_config_from(path: &Path) -> SdkConfig;
pub fn resolve_channel_config(overrides: Option<SdkConfig>) -> SdkConfig;
pub fn load_route_tag(account_id: &str) -> Option<String>;
pub fn load_bot_agent() -> Option<String>;
```

> Note: there are two `QuoteCachePolicy` types — `storage::config::QuoteCachePolicy`
> (config normalization) and `messaging::quote_cache::QuoteCachePolicy` (the
> runtime cache policy). They share the same field shape.

### `state_dir` submodule

[`src/storage/state_dir.rs`](../src/storage/state_dir.rs)

```rust
pub fn resolve_state_dir() -> PathBuf;     // OPENCLAW_STATE_DIR > CLAWDBOT_STATE_DIR > XDG / ~
pub fn sdk_state_dir() -> PathBuf;         // resolve_state_dir()/openclaw-weixin
pub fn accounts_dir() -> PathBuf;          // sdk_state_dir()/accounts
pub fn ensure_dir(path: &Path) -> io::Result<&Path>;
```

### `sync_buf` submodule

[`src/storage/sync_buf.rs`](../src/storage/sync_buf.rs) — the persistent
`get_updates_buf` cursor.

```rust
pub struct SyncBufData { pub get_updates_buf: String }
impl SyncBufData { pub fn new(get_updates_buf: impl Into<String>) -> Self; }

pub fn get_sync_buf_path(account_id: &str) -> PathBuf;
pub fn load_get_updates_buf(path: &Path) -> Option<String>;
pub fn load_for_account(account_id: &str) -> Option<String>;
pub fn save_get_updates_buf(path: &Path, get_updates_buf: &str) -> io::Result<()>;
pub fn save_for_account(account_id: &str, get_updates_buf: &str) -> io::Result<()>;
pub fn clear_for_account(account_id: &str);
```

`load_get_updates_buf` falls back to a derived raw-id filename and a legacy
single-account path on older installs.

---

## `media` module

[`src/media/mod.rs`](../src/media/mod.rs) · [`src/media/mime.rs`](../src/media/mime.rs)

```rust
pub const OCTET_STREAM: &str = "application/octet-stream";
pub const BIN_EXTENSION: &str = ".bin";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind { Image, Video, File }
impl MediaKind { pub fn from_path(path: &str) -> Self; }   // video/ > image/ > File

pub fn mime_from_filename(filename: &str) -> String;
pub fn mime_from_extension(ext: &str) -> String;
pub fn extension_from_mime(mime: &str) -> String;
pub fn extension_from_content_type_or_url(content_type: Option<&str>, url: &str) -> String;
pub fn extension_of(path: &str) -> String;
pub fn is_video(path: &str) -> bool;
pub fn is_image(path: &str) -> bool;
pub fn is_audio(path: &str) -> bool;
```

---

## `util` module

[`src/util/mod.rs`](../src/util/mod.rs) · [`src/util/random.rs`](../src/util/random.rs)
· [`src/util/redact.rs`](../src/util/redact.rs) · [`src/util/time.rs`](../src/util/time.rs)

```rust
// random.rs
pub fn generate_id(prefix: &str) -> String;                // "prefix:<ts>-<hex>"
pub fn temp_file_name(prefix: &str, ext: &str) -> String;
pub fn file_key() -> String;                              // 32 hex chars
pub fn random_u32() -> u32;

// redact.rs
pub const DEFAULT_TOKEN_PREFIX_LEN: usize = 6;
pub const DEFAULT_BODY_MAX_LEN: usize = 200;
pub fn truncate(s: &str, max: usize) -> String;
pub fn redact_token(token: Option<&str>, prefix_len: Option<usize>) -> String;
pub fn redact_body(body: &str, max_len: Option<usize>) -> String;
pub fn redact_json_fields(input: &str) -> String;
pub fn redact_url(url: &str) -> String;

// time.rs
pub fn now_millis() -> u128;
pub fn now_iso8601() -> String;                           // millisecond-precision UTC
```

---

## `logging` module

[`src/logging.rs`](../src/logging.rs) — re-exports `tracing::{debug, error,
info, trace, warn}` and installs a `tracing_subscriber` honouring `RUST_LOG`
when the `cli` feature is enabled.

```rust
pub const ACCOUNT_FIELD: &str = "account";

pub enum Level { Trace, Debug, Info, Warn, Error }
impl Level { pub fn as_str(&self) -> &'static str; }
impl FromStr for Level { /* "warning" -> Warn */ }

pub fn init_tracing(level: Level);   // available with the `cli` feature
```

The SDK is `tracing`-based and works with no global subscriber installed
(records are dropped silently); downstream apps may install their own.

## `tls` module

[`src/tls.rs`](../src/tls.rs) — server certificates are validated against the
OS trust store via `rustls-platform-verifier`.

```rust
pub fn default_tls_config() -> Result<ClientConfig, RustlsError>;

pub struct DangerIgnoreVerifier;   // tests only: accepts every certificate
```

---

## Typical call graph

```text
WeixinClientBuilder::build()
  └─ WeixinClient
       ├─ .qr_login()  → QrLogin  → QrSession
       ├─ .sender()    → MessageSender::send_text / send_file
       │                     └─ cdn::upload_*  → WeixinApiClient::get_upload_url
       ├─ .api()       → WeixinApiClient  (raw endpoints: get_updates, send_message, ...)
       └─ .enable_quote_cache()  → QuoteStore (SQLite)

storage::*        — credentials, state dir, sync cursor, config
messaging::context — inbound MsgContext + ContextTokenStore
cdn::*            — AES-128-ECB upload/download
util::*           — ids, redaction, time
```
