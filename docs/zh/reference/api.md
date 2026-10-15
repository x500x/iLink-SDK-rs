# API 手册

本页是 `ilink_sdk` 包对外公开的全部 API 的权威参考。它根据 crate 的模块结构整理，建议配合其链接到的源文件一起阅读。

> 该 crate **不含内嵌 HTTP 服务器，也不依赖网关**。以下每一项都是你可以直接调用或构造的函数、类型或常量。关于登录流程、消息收发、媒体等叙事性指南，请参阅
> [docs/zh/README.md](../README.md) 索引。

## crate 根

包名：`ilink_sdk`（库），版本 `ilink_sdk::VERSION`（`env!("CARGO_PKG_VERSION")`）。

顶层模块：

| 模块 | 用途 |
| --- | --- |
| [`error`](#errormod) | `Error`、`ErrorKind`、`Result` |
| [`api`](#apimod) | 底层 HTTP 客户端、线类型、会话守卫 |
| [`client`](#clientmod) | 高层 `WeixinClient` / `WeixinClientBuilder` |
| [`messaging`](#messagingmod) | 二维码登录、出站、入站上下文、输入状态、引用缓存 |
| [`cdn`](#cdnmod) | AES-128-ECB 加密 + CDN 上传/下载 |
| [`storage`](#storagemod) | 账户存储、状态目录、同步游标、配置 |
| [`media`](#mediamod) | MIME 探测 |
| [`util`](#utilmod) | ID 生成、脱敏、时间 |
| [`logging`](#loggingmod) | `tracing` 订阅器辅助（CLI feature） |
| [`tls`](#tlsmod) | rustls 配置 |

### `prelude`

便捷再导出模块，汇集最常用的类型，调用方可以直接
`use ilink_sdk::prelude::*;`。

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

> ⚠️ `prelude` 再导出 `api::types` 的全部内容，因此 `WeixinMessage`、
> `MessageItem` 等名称会进入作用域。若需避免命名冲突，请显式导入
> `api::types`。

---

## `error` 模块

[`src/error.rs`](../src/error.rs)

crate 统一的失败类型。每个可能失败的调用都返回携带 crate `Error` 的
`Result<T>`。

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

### `enum ErrorKind`

传输层失败的分类。业务级错误（HTTP 200 但 `ret != 0`）以 `ErrorKind::Unknown`
呈现，而不是单独的变体。

| 变体 | 含义 |
| --- | --- |
| `Dns` | DNS 解析失败 |
| `Tcp` | TCP 连接 / 重置失败 |
| `Tls` | TLS 握手错误 |
| `Timeout` | 客户端超时 |
| `Cancelled` | 外部取消信号触发 |
| `Unknown` | 其他网络级错误 |

```rust
impl ErrorKind {
    pub fn as_str(&self) -> &'static str;        // "dns"、"tcp"……
    pub fn description(&self) -> &'static str;   // 人类可读的英文说明
}
impl fmt::Display for ErrorKind { /* …… */ }
```

### `struct Error`

```rust
impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self;
    pub fn unknown(message: impl Into<String>) -> Self;
    pub fn with_external_cancel(mut self) -> Self;   // 标记为外部取消

    pub fn kind(&self) -> ErrorKind;
    pub fn is_external_cancel(&self) -> bool;
    pub fn is_abort(&self) -> bool;        // Timeout 或 Cancelled
    pub fn is_transport(&self) -> bool;    // 可重试的连接错误
}
```

`Error` 实现了 `Display`、`std::error::Error`，以及针对 `std::io::Error`、
`serde_json::Error`、`reqwest::Error`、`reqwest::header::{InvalidHeaderValue,
ToStrError}`、`rustls::Error`、`url::ParseError`、`hex::FromHexError`、
`base64::DecodeError`、`rusqlite::Error` 的 `From`。

`Error::from_reqwest_wrap` 是传输层使用的主要分类器：它保留 `reqwest` 已判定
的 `Timeout`/`Cancelled` 区分，其余情况委托给 `classify_fetch_error_string`
（TypeScript `classifyFetchError` 字符串匹配的移植版本，匹配 `ENOTFOUND`、
`ECONNREFUSED`、`SSL`/`CERT`/`TLS` 等）。

---

## `api` 模块

[`src/api/mod.rs`](../src/api/mod.rs) · [`src/api/client.rs`](../src/api/client.rs)
· [`src/api/types.rs`](../src/api/types.rs) · [`src/api/session_guard.rs`](../src/api/session_guard.rs)

### `WeixinApiClient`

底层异步 HTTP 客户端，将每个 SDK 操作映射到一个 iLink CGI 端点。多数应用使用
[`WeixinClient`](#weixinclient)，但原始客户端可用于自定义流程、测试与插桩。

```rust
impl WeixinApiClient {
    pub fn new(opts: WeixinApiOptions) -> Result<Self>;
    pub fn with_transport(opts: WeixinApiOptions, transport: Arc<dyn HttpTransport>) -> Self;
    pub fn with_bot_agent(mut self, bot_agent: impl AsRef<str>) -> Self;

    pub fn options(&self) -> &WeixinApiOptions;
    pub fn base_info(&self) -> BaseInfo;     // 当前 base_info 负载

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

    // 原始辅助方法（二维码登录流程内部使用）：
    pub async fn get_text(&self, endpoint: &str, timeout_ms: u64) -> Result<String>;
    pub async fn post_text_unauthenticated(
        &self,
        endpoint: &str,
        json_body: &impl Serialize,
        timeout_ms: Option<u64>,
    ) -> Result<String>;
}
```

行为说明：

- 客户端可 `Clone`（传输层由 `Arc` 共享）。
- `get_updates` 在客户端超时**或**外部取消时，返回携带*相同* `get_updates_buf`
  的空 `ret: 0` 响应，因此调用方只需重试——它不会报错。
- `send_message` 将非零 `ret` 视为错误；`ret` 缺失则视为成功。
- 每个已认证的 POST 都携带 `iLink-App-Id: bot`、`iLink-App-ClientVersion`、
  `AuthorizationType: ilink_bot_token`、`Authorization: Bearer <token>`、
  `X-WECHAT-UIN`、可选 `SKRouteTag`，以及 `base_info` 请求体。

### `WeixinApiOptions`

```rust
impl WeixinApiOptions {
    pub fn new(base_url: impl Into<String>) -> Self;        // 无 token
    pub fn with_token(mut self, token: impl Into<String>) -> Self;
    pub fn with_route_tag(mut self, tag: impl Into<String>) -> Self;
    pub fn with_cdn_base_url(mut self, url: impl Into<String>) -> Self;
    pub fn with_timeout_ms(mut self, ms: u64) -> Self;      // 全局请求默认超时
}

pub struct WeixinApiOptions {
    pub base_url: String,
    pub token: Option<String>,
    pub timeout_ms: Option<u64>,
    pub route_tag: Option<String>,
    pub cdn_base_url: Option<String>,
}
```

`with_timeout_ms` 设置*默认*单次请求超时；未设置时，各操作回退到各自的默认
常量（见下）。

### 常量

```rust
pub const ILINK_APP_ID: &str = "bot";
pub const AUTHORIZATION_TYPE: &str = "ilink_bot_token";
pub const DEFAULT_LONG_POLL_TIMEOUT_MS: u64 = 35_000;
pub const DEFAULT_API_TIMEOUT_MS: u64 = 15_000;
pub const DEFAULT_CONFIG_TIMEOUT_MS: u64 = 10_000;
```

### `HttpTransport` / `ReqwestTransport`

一个 trait，便于在不触网的情况下对客户端做单元测试。

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

pub struct ReqwestTransport;   // 默认实现，基于 reqwest + rustls
```

### 自由函数（`api` 再导出）

```rust
pub fn build_client_version(version: &str) -> u32;          // "2.4.10" -> 0x0002040A
pub fn build_base_info() -> BaseInfo;
pub fn build_base_info_with(channel_version: &str, bot_agent: &str) -> BaseInfo;
pub fn sanitize_bot_agent(raw: Option<&str>) -> String;     // UA 风格语法，≤256 字节
pub fn parse_api_json<T: DeserializeOwned>(raw: &str) -> Result<T>;
pub fn quote_lossless_ids(raw: &str) -> String;
```

`parse_api_json` 会在反序列化前，对 uint64 标识字段（`message_id`、`msg_id`、
`svr_id`）加引号，使超过 `i64::MAX` 的值以 `String` 无损保存。已经是字符串、
`null` 或 `true`/`false` 的值保持原样。

### `session_guard` 子模块

[`src/api/session_guard.rs`](../src/api/session_guard.rs) —— 在令牌过期错误
（`errcode -14`）后，暂停某账户的所有 API 调用一小时。

```rust
pub const STALE_TOKEN_ERRCODE: i64 = -14;
pub const SESSION_PAUSE_DURATION: Duration = Duration::from_secs(60 * 60);

pub fn pause_session(account_id: &str);
pub fn is_session_paused(account_id: &str) -> bool;
pub fn get_remaining_pause_ms(account_id: &str) -> u128;
pub fn assert_session_active(account_id: &str) -> Result<()>;
pub fn resume_session(account_id: &str);
pub fn reset_session_pauses();          // 仅测试使用

pub struct SessionGuard { /* 绑定到单一 account_id */ }
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

### `types` 子模块

[`src/api/types.rs`](../src/api/types.rs) —— 线类型。所有请求/响应结构体都
`#[derive(Serialize, Deserialize)]` 且 `Default`。关键形态：

| 类型 | 说明 |
| --- | --- |
| `BaseInfo` | `{ channel_version, bot_agent }` |
| `UploadMediaType` | `Image=1, Video=2, File=3, Voice=4`；`value()`、`from_value()` |
| `GetUploadUrlReq` / `GetUploadUrlResp` | CDN 预签名 |
| `MessageType` | `None=0, User=1, Bot=2` |
| `MessageItemType` | `None, Text, Image, Voice, File, Video, ToolCallStart, ToolCallResult` |
| `MessageState` | `New=0, Generating=1, Finish=2` |
| `TextItem`、`ImageItem`、`VoiceItem`、`FileItem`、`VideoItem` | 媒体项 |
| `CdnMedia` | `encrypt_query_param`、`aes_key`、`encrypt_type`、`full_url` |
| `RefMessage`、`PartialText` | 内联引用 |
| `ToolCallStartItem`、`ToolCallResultItem` | 工具调用项 |
| `MessageItem` | 单个内容项；`WeixinMessage` 的 `item_list` |
| `WeixinMessage` | `seq`、`message_id`（String）、各类 id、`item_list`、`context_token`、`run_id` |
| `GetUpdatesReq` / `GetUpdatesResp` | 长轮询游标 `get_updates_buf` |
| `SendMessageReq` / `SendMessageResp` | 包裹一个 `WeixinMessage` |
| `TypingStatus` | `Typing=1, Cancel=2` |
| `SendTypingReq` / `SendTypingResp` | 输入状态 |
| `GetConfigResp` | 含 `typing_ticket` |
| `NotifyStopReq/Resp`、`NotifyStartReq/Resp` | 生命周期心跳 |

重要的 `impl` 辅助：

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

常量：`DEFAULT_BOT_AGENT = "OpenClaw"`、`BOT_AGENT_MAX_LEN = 256`。

---

## `client` 模块

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

当 `account_id` 未设置时，`build` 回退到唯一已注册账户；若存在零个或多个账户则报错。

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

`WeixinClient` 可 `Clone` 但**不** `Sync`（`QuoteStore` 是 `Send` 而非
`Sync`）；它旨在被克隆共享，而非跨任务以 `&` 共享。`from_session` 可直接从
`QrSession` 构造客户端，不接触账户存储——适合自定义持久化策略。

---

## `messaging` 模块

[`src/messaging/mod.rs`](../src/messaging/mod.rs)

### `auth` 子模块 —— 二维码登录

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

关键类型：

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

`wait` 的 `on_verification_required` 返回用户键入的验证码；空字符串视为错误
验证码。`on_refresh` 在二维码被替换时触发，以便宿主重新渲染。

### `outbound` 子模块 —— 发送

[`src/messaging/outbound.rs`](../src/messaging/outbound.rs)

```rust
#[derive(Debug, Clone, Default)]
pub struct SendOpts {
    pub context_token: Option<String>,
    pub run_id: Option<String>,
}

pub struct SendResult {
    pub message_id: String,             // 客户端生成的 id
    pub server_message_id: Option<String>,
}

pub struct MessageSender<'a> { /* 绑定到 api 客户端 + account_id */ }
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

`send_file` 按 MIME 类型路由（视频 / 图片 / 文件），先经 CDN 管道上传，再分别
发送「说明文字 + 媒体项」作为独立请求。缺少 `context_token` 时仅记录警告，
**不会**阻断发送。

### `typing` 子模块

[`src/messaging/typing.rs`](../src/messaging/typing.rs)

```rust
pub enum Typing { Typing, Cancel }
impl From<Typing> for TypingStatus { /* 1 / 2 */ }

pub async fn send_typing(
    api: &WeixinApiClient,
    ilink_user_id: &str,
    ticket: &str,
    status: Typing,
) -> Result<()>;             // ticket 为空时为空操作（不触网）

pub struct TypingHelper<'a> { /* api + user_id + ticket */ }
impl<'a> TypingHelper<'a> {
    pub fn new(api: &'a WeixinApiClient, user_id: impl Into<String>, ticket: impl Into<String>) -> Self;
    pub fn enabled(&self) -> bool;
    pub async fn start(&self) -> Result<()>;
    pub async fn stop(&self) -> Result<()>;
}
```

### `context` 子模块 —— 入站上下文

[`src/messaging/context.rs`](../src/messaging/context.rs)

```rust
pub struct ContextTokenStore { /* 内存 + JSON 持久化 */ }
impl ContextTokenStore {
    pub fn new() -> Self;
    pub fn with_dir(dir: impl Into<PathBuf>) -> Self;
    pub fn set(&mut self, account_id: &str, user_id: &str, token: &str);
    pub fn get(&self, account_id: &str, user_id: &str) -> Option<&str>;
    pub fn accounts_for_user<'a>(&self, account_ids: &'a [String], user_id: &str) -> Vec<&'a str>;
    pub fn clear_account(&mut self, account_id: &str);
    pub fn restore(&mut self, account_id: &str) -> usize;   // 返回加载的条数
}

pub struct MsgContext { /* body、from、to、account_id、message_sid、context_token、media*、reply_to_*…… */ }
pub struct MediaEntry { pub path: String, pub content_type: Option<String>, pub file_name: Option<String>, pub message_id: Option<String> }

pub fn is_media_item(item: &MessageItem) -> bool;
pub fn media_label(item_type: Option<i64>) -> &'static str;   // "[图片]"、 "[视频]"……
pub fn message_id(msg: &WeixinMessage) -> Option<String>;
pub fn body_from_items(items: &[MessageItem]) -> String;
pub fn message_to_context(msg: &WeixinMessage, account_id: &str, media: &InboundMedia) -> MsgContext;
pub fn remember_context_token(store: &mut ContextTokenStore, ctx: &MsgContext) -> bool;
```

`message_to_context` 构建面向宿主的 `MsgContext`，解析媒体路径与内联
`reply_to_*` 引用字段。`InboundMedia` 定义于
[`cdn::download`](#download-子模块) 子模块。

### `quote_cache` 子模块 —— 本地引用缓存

[`src/messaging/quote_cache.rs`](../src/messaging/quote_cache.rs) —— 基于
SQLite 的存储，用于仅有服务器消息 id 的入站引用。当 `QuoteCachePolicy::enabled`
为 `false` 时禁用。

```rust
pub struct QuoteCachePolicy {
    pub enabled: bool,
    pub retention_ms: u64,
    pub max_messages_per_account: u64,
    pub media_retention_ms: u64,
    pub max_media_bytes_per_account: u64,
    pub max_single_media_bytes: u64,
}
impl Default for QuoteCachePolicy { /* 文本 30 天、媒体 7 天、1 万条、256 MiB、25 MiB */ }

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

pub struct QuoteMessageRecord { /* 与 input 同字段 + created_at */ }

pub struct QuoteStore { /* SQLite + 受管媒体根目录 */ }
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

常量：`DEFAULT_RETENTION_DAYS = 30`、`DEFAULT_MAX_MESSAGES_PER_ACCOUNT =
10000`、`DEFAULT_MEDIA_RETENTION_DAYS = 7`、`DEFAULT_MAX_MEDIA_BYTES_PER_ACCOUNT
= 256 MiB`、`DEFAULT_MAX_SINGLE_MEDIA_BYTES = 25 MiB`、`QUOTE_MEDIA_SUBDIR =
"media/inbound/openclaw-weixin-quotes"`。

`QuoteStore::find` 在读取时删除过期记录；`gc` 删除过期消息、将各账户裁剪至上限、
使旧媒体过期，并强制执行每账户媒体预算。存储根目录之外的受管媒体会被拒绝。

---

## `cdn` 模块

[`src/cdn/mod.rs`](../src/cdn/mod.rs)

### `aes_ecb` 子模块

[`src/cdn/aes_ecb.rs`](../src/cdn/aes_ecb.rs) —— 逐块 AES-128-ECB（真正的
ECB，而非零 IV 的 CBC）。

```rust
pub const BLOCK_SIZE: usize = 16;
pub fn aes_ecb_padded_size(len: usize) -> usize;          // PKCS#7 填充后大小
pub fn encrypt_aes_ecb(plaintext: &[u8], key: &[u8; 16]) -> Result<Vec<u8>>;
pub fn decrypt_aes_ecb(ciphertext: &[u8], key: &[u8; 16]) -> Result<Vec<u8>>;
```

### `cdn_url` 子模块

[`src/cdn/cdn_url.rs`](../src/cdn/cdn_url.rs)

```rust
pub const CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";
pub const ENABLE_CDN_URL_FALLBACK: bool = true;

pub fn build_cdn_download_url(encrypted_query_param: &str, cdn_base_url: &str) -> Result<String>;
pub fn build_cdn_upload_url(cdn_base_url: &str, upload_param: &str, filekey: &str) -> Result<String>;
pub fn resolve_download_url(full_url: Option<&str>, encrypted_query_param: &str, cdn_base_url: &str, label: &str) -> Result<String>;
pub fn resolve_upload_url(upload_full_url: Option<&str>, upload_param: Option<&str>, filekey: &str, cdn_base_url: &str, label: &str) -> Result<String>;
```

服务器提供的 `full_url` 始终优先；否则当 `ENABLE_CDN_URL_FALLBACK` 开启时，由
参数构造 URL。

### `download` 子模块

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

pub struct MediaSink { /* 目录 + 可选子目录 */ }
impl MediaSink {
    pub fn new(dir: impl Into<PathBuf>) -> Self;
    pub fn with_subdir(mut self, subdir: impl Into<String>) -> Self;
    pub async fn save(&self, bytes: &[u8], extension: &str) -> Result<PathBuf>;
}

pub async fn download_item_media(item: &MessageItem, cdn_base_url: &str, sink: &MediaSink, label: &str) -> InboundMedia;
pub async fn download_item_to_path(item: &MessageItem, cdn_base_url: &str, dest_dir: &Path, label: &str) -> Result<Option<(PathBuf, String)>>;
pub fn outbound_kind(path: &Path) -> MediaKind;
```

`parse_aes_key` 接受实际中出现的两种编码：`base64(16 字节原始)`（图片）与
`base64(32 字符 hex 串)`（文件 / 语音 / 视频）。带转录文本的语音项被跳过
（转录文本已是正文）；原始 `SILK` 语音会被返回，交由宿主转码。

### `upload` 子模块

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
pub async fn upload_buffer_to_cdn(ciphertext: &[u8], url: &str, label: &str) -> Result<String>;   // 返回 x-encrypted-param
pub async fn download_remote_to_temp(url: &str, dest_dir: &Path) -> Result<PathBuf>;

pub fn backoff_delay(attempt: usize) -> Duration;   // 250ms，指数退避，封顶 4s
```

HTTP 4xx 立即中止；其他失败重试至 `UPLOAD_MAX_RETRIES` 次。

---

## `storage` 模块

[`src/storage/mod.rs`](../src/storage/mod.rs)

### `accounts` 子模块

[`src/storage/accounts.rs`](../src/storage/accounts.rs)

```rust
pub const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
pub const CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";

pub struct AccountData { pub token, saved_at, base_url, user_id: Option<String> }
impl AccountData { pub fn is_configured(&self) -> bool; pub fn effective_base_url(&self) -> String; }

pub struct StoredAccount { /* account_id、base_url、cdn_base_url、token、enabled、configured、name、user_id、route_tag */ }

pub struct AccountUpdate { pub token: Option<String>, pub base_url: Option<String>, pub user_id: Option<String> }

pub fn normalize_account_id(raw: &str) -> String;          // "@im.bot" -> "-im-bot"
pub fn derive_raw_account_id(normalized_id: &str) -> Option<String>;
pub fn list_indexed_account_ids() -> Vec<String>;
pub fn list_account_ids() -> Vec<String>;                  // 别名
pub fn register_account_id(account_id: &str) -> Result<()>;
pub fn unregister_account_id(account_id: &str) -> Result<()>;
pub fn load_account(account_id: &str) -> Option<AccountData>;
pub fn save_account(account_id: &str, update: &AccountUpdate) -> Result<AccountData>;
pub fn clear_account(account_id: &str) -> Result<()>;
pub fn clear_stale_accounts_for_user_id(current: &str, user_id: &str, on_clear: impl FnMut(&str)) -> Result<()>;
pub fn resolve_account(account_id: &str) -> Result<StoredAccount>;
```

`save_account` 仅当 `token` 非空时才覆盖；仅当 `base_url` 非空时才存储；当
`user_id` 显式设为 `""` 时清空。

### `config` 子模块

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
pub struct QuoteCacheRaw { /* camelCase 原始值 */ }
impl QuoteCacheRaw { pub fn to_policy(&self) -> QuoteCachePolicy; }   // storage::config::QuoteCachePolicy

pub struct QuoteCachePolicy { /* enabled、retention_ms、max_messages_per_account、media_retention_ms、max_media_bytes_per_account、max_single_media_bytes */ }
impl Default for QuoteCachePolicy { /* 见 config.rs */ }

pub fn resolve_config_path() -> PathBuf;
pub fn load_local_config() -> SdkConfig;
pub fn load_local_config_from(path: &Path) -> SdkConfig;
pub fn resolve_channel_config(overrides: Option<SdkConfig>) -> SdkConfig;
pub fn load_route_tag(account_id: &str) -> Option<String>;
pub fn load_bot_agent() -> Option<String>;
```

> 注意：存在两个 `QuoteCachePolicy` 类型 —— `storage::config::QuoteCachePolicy`
> （配置归一化）与 `messaging::quote_cache::QuoteCachePolicy`（运行时缓存策略）。
> 二者字段形态相同。

### `state_dir` 子模块

[`src/storage/state_dir.rs`](../src/storage/state_dir.rs)

```rust
pub fn resolve_state_dir() -> PathBuf;     // OPENCLAW_STATE_DIR > CLAWDBOT_STATE_DIR > XDG / ~
pub fn sdk_state_dir() -> PathBuf;         // resolve_state_dir()/openclaw-weixin
pub fn accounts_dir() -> PathBuf;          // sdk_state_dir()/accounts
pub fn ensure_dir(path: &Path) -> io::Result<&Path>;
```

### `sync_buf` 子模块

[`src/storage/sync_buf.rs`](../src/storage/sync_buf.rs) —— 持久化的
`get_updates_buf` 游标。

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

`load_get_updates_buf` 会回退到派生的原始 id 文件名，以及旧安装的单一账户
遗留路径。

---

## `media` 模块

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

## `util` 模块

[`src/util/mod.rs`](../src/util/mod.rs) · [`src/util/random.rs`](../src/util/random.rs)
· [`src/util/redact.rs`](../src/util/redact.rs) · [`src/util/time.rs`](../src/util/time.rs)

```rust
// random.rs
pub fn generate_id(prefix: &str) -> String;                // "prefix:<ts>-<hex>"
pub fn temp_file_name(prefix: &str, ext: &str) -> String;
pub fn file_key() -> String;                              // 32 个 hex 字符
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
pub fn now_iso8601() -> String;                           // 毫秒精度的 UTC
```

---

## `logging` 模块

[`src/logging.rs`](../src/logging.rs) —— 再导出 `tracing::{debug, error,
info, trace, warn}`，并在启用 `cli` feature 时安装遵循 `RUST_LOG` 的
`tracing_subscriber`。

```rust
pub const ACCOUNT_FIELD: &str = "account";

pub enum Level { Trace, Debug, Info, Warn, Error }
impl Level { pub fn as_str(&self) -> &'static str; }
impl FromStr for Level { /* "warning" -> Warn */ }

pub fn init_tracing(level: Level);   // 在 `cli` feature 下可用
```

该 SDK 基于 `tracing`，未安装全局订阅器时也能工作（记录会被静默丢弃）；
下游应用可自行安装订阅器。

## `tls` 模块

[`src/tls.rs`](../src/tls.rs) —— 通过 `rustls-platform-verifier` 依据操作系统
信任库校验服务器证书。

```rust
pub fn default_tls_config() -> Result<ClientConfig, RustlsError>;

pub struct DangerIgnoreVerifier;   // 仅测试用：接受任何证书
```

---

## 典型调用图

```text
WeixinClientBuilder::build()
  └─ WeixinClient
       ├─ .qr_login()  → QrLogin  → QrSession
       ├─ .sender()    → MessageSender::send_text / send_file
       │                     └─ cdn::upload_*  → WeixinApiClient::get_upload_url
       ├─ .api()       → WeixinApiClient  （原始端点：get_updates、send_message……）
       └─ .enable_quote_cache()  → QuoteStore （SQLite）

storage::*        —— 凭据、状态目录、同步游标、配置
messaging::context —— 入站 MsgContext + ContextTokenStore
cdn::*            —— AES-128-ECB 上传/下载
util::*           —— ID、脱敏、时间
```
