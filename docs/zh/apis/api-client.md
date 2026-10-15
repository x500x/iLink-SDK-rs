# 底层 API 客户端 API

底层 API 客户端是 SDK 与微信（iLink）后端通信的核心，位于 [`../../src/api/`](../../src/api/) 目录。
它把每个 SDK 操作映射到一个 CGI 端点。绝大多数应用直接使用高层
[`WeixinClient`](../apis/client.md)，但底层客户端在以下场景更有价值：

- 自定义鉴权流程或生命周期管理
- 单元测试：用 `with_transport` 注入 mock
- 在网关/代理/插桩之外直接访问某个端点
- 读取底层 `get_updates` 长轮询游标并自行驱动循环

## 模块构成

| 子模块 | 文件 | 职责 |
| --- | --- | --- |
| `client` | [`../../src/api/client.rs`](../../src/api/client.rs) | HTTP 客户端与请求/超时/头管理 |
| `types` | [`../../src/api/types.rs`](../../src/api/types.rs) | 线类型（请求/响应结构体） |
| `session_guard` | [`../../src/api/session_guard.rs`](../../src/api/session_guard.rs) | 令牌过期后暂停账户 |

## `WeixinApiClient`

```rust
#[derive(Clone)]
pub struct WeixinApiClient;   // 内部持有 opts、transport、channel_version、bot_agent
```

### 构造

```rust
impl WeixinApiClient {
    /// 使用默认 reqwest 传输构造。
    pub fn new(opts: WeixinApiOptions) -> Result<Self>;

    /// 注入自定义传输（测试、代理、插桩）。transport 必须是
    /// `Arc<dyn HttpTransport>`。
    pub fn with_transport(opts: WeixinApiOptions, transport: Arc<dyn HttpTransport>) -> Self;

    /// 覆盖 `bot_agent` 声明（会经 sanitize_bot_agent 处理）。
    pub fn with_bot_agent(mut self, bot_agent: impl AsRef<str>) -> Self;
}
```

客户端是 `Clone` 的，因为底层 transport 通过 `Arc` 共享；克隆开销很低。

### 读取配置

```rust
impl WeixinApiClient {
    pub fn options(&self) -> &WeixinApiOptions;  // 底层选项
    pub fn base_info(&self) -> BaseInfo;          // 当前 base_info 负载
}
```

`base_info()` 返回 `{ channel_version, bot_agent }`，每次请求都会自动附带。

### 端点方法

每个方法对应一个 iLink CGI 端点。所有方法都是 `async`。

```rust
impl WeixinApiClient {
    // 长轮询接收消息
    pub async fn get_updates(
        &self,
        req: &GetUpdatesReq,
        timeout_ms: Option<u64>,
        cancel: Option<CancellationToken>,
    ) -> Result<GetUpdatesResp>;

    // 发送消息
    pub async fn send_message(&self, req: &SendMessageReq) -> Result<SendMessageResp>;

    // 预签名 CDN 上传 URL
    pub async fn get_upload_url(&self, req: &GetUploadUrlReq) -> Result<GetUploadUrlResp>;

    // 获取 bot 配置（含 typing_ticket）
    pub async fn get_config(
        &self,
        ilink_user_id: &str,
        context_token: Option<&str>,
    ) -> Result<GetConfigResp>;

    // 发送 / 取消输入状态
    pub async fn send_typing(&self, req: &SendTypingReq) -> Result<()>;

    // 生命周期心跳：停止
    pub async fn notify_stop(&self) -> Result<NotifyStopResp>;
    // 生命周期心跳：开始
    pub async fn notify_start(&self) -> Result<NotifyStartResp>;
}
```

### 原始辅助方法（登录流程内部使用，也可以直接调用）

```rust
impl WeixinApiClient {
    /// 发送原始已认证的 GET 请求并返回文本（二维码状态轮询用）。
    pub async fn get_text(&self, endpoint: &str, timeout_ms: u64) -> Result<String>;

    /// 发送原始未认证的 POST 请求并返回文本（二维码获取用，不带 Authorization）。
    pub async fn post_text_unauthenticated(
        &self,
        endpoint: &str,
        json_body: &impl Serialize,
        timeout_ms: Option<u64>,
    ) -> Result<String>;
}
```

### 行为细节

- **认证头。** 每个已认证 POST 自动携带：
  - `iLink-App-Id: bot`（常量 `ILINK_APP_ID`）
  - `iLink-App-ClientVersion`：`build_client_version(VERSION)`，例如 `2.4.10`
    → `0x0002040A`
  - `AuthorizationType: ilink_bot_token`（常量 `AUTHORIZATION_TYPE`）
  - `Authorization: Bearer <token>`
  - `X-WECHAT-UIN`：`base64(随机 u32)`
  - 可选 `SKRouteTag`（来自 `WeixinApiOptions::route_tag`）
  - 请求体中的 `base_info`
- **长轮询超时/取消不报错。** `get_updates` 在客户端超时或外部取消时，返回
  空 `ret:0` 响应且携带**相同**的 `get_updates_buf`，调用方直接重试即可。
- **`send_message` 的错误语义。** `ret != 0` 视为错误；`ret` 缺失视为成功
  （对应 TS 的 `if (resp.ret && resp.ret !== 0)` 守卫）。
- **HTTP 状态码。** 非 2xx 会直接返回 `ErrorKind::Unknown`，消息里含截断且脱敏
  的响应体。

## `WeixinApiOptions`

```rust
#[derive(Debug, Clone)]
pub struct WeixinApiOptions {
    pub base_url: String,                  // 例如 "https://ilinkai.weixin.qq.com"
    pub token: Option<String>,             // 登录得到的 bot token
    pub timeout_ms: Option<u64>,           // 全局默认请求超时
    pub route_tag: Option<String>,         // 作为 SKRouteTag 转发
    pub cdn_base_url: Option<String>,      // 媒体管线使用
}

impl WeixinApiOptions {
    /// 仅 base_url，无 token（用于登录前请求）。
    pub fn new(base_url: impl Into<String>) -> Self;

    pub fn with_token(mut self, token: impl Into<String>) -> Self;
    pub fn with_route_tag(mut self, tag: impl Into<String>) -> Self;
    pub fn with_cdn_base_url(mut self, url: impl Into<String>) -> Self;
    pub fn with_timeout_ms(mut self, ms: u64) -> Self;   // 设置默认超时
}
```

`with_timeout_ms` 设置的是**全局默认**；每个操作在其未设置时，回退到各自的
默认常量（见下）。

## 常量（超时与标识）

```rust
pub const ILINK_APP_ID: &str = "bot";
pub const AUTHORIZATION_TYPE: &str = "ilink_bot_token";
pub const DEFAULT_LONG_POLL_TIMEOUT_MS: u64 = 35_000;   // get_updates
pub const DEFAULT_API_TIMEOUT_MS: u64 = 15_000;          // send_message / get_upload_url
pub const DEFAULT_CONFIG_TIMEOUT_MS: u64 = 10_000;       // get_config / send_typing / notify_*
```

## `HttpTransport` 与 `ReqwestTransport`

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

pub struct ReqwestTransport;   // 默认实现，使用 reqwest + rustls
```

实现 `HttpTransport` 即可注入 mock。集成测试
[`../../tests/api_integration.rs`](../../tests/api_integration.rs) 就是用它断言请求头、
请求体、超时与取消行为，而不触达真实后端。

示例——一个记录所有请求的 mock（示意）：

```rust
use ilink_sdk::api::{HttpTransport, ReqwestTransport, WeixinApiOptions, WeixinApiClient};
use std::sync::Arc;

// 用默认实现包装，仅在测试中替换
let transport = Arc::new(ReqwestTransport::default());
let client = WeixinApiClient::with_transport(
    WeixinApiOptions::new("https://ilinkai.weixin.qq.com"),
    transport,
);
```

## 自由函数

```rust
pub fn build_client_version(version: &str) -> u32;
// "2.4.10" -> 0x0002040A
// 规则：((major & 0xFF) << 16) | ((minor & 0xFF) << 8) | (patch & 0xFF)

pub fn build_base_info() -> BaseInfo;
pub fn build_base_info_with(channel_version: &str, bot_agent: &str) -> BaseInfo;

pub fn sanitize_bot_agent(raw: Option<&str>) -> String;
// 语法：name/version [ (comment) ]，无效 token 被丢弃；超过 256 字节则丢弃尾部 token

pub fn parse_api_json<T: DeserializeOwned>(raw: &str) -> Result<T>;
pub fn quote_lossless_ids(raw: &str) -> String;
```

`parse_api_json` 会在反序列化前，把 uint64 标识字段（`message_id`、`msg_id`、
`svr_id`）加引号，使超过 `i64::MAX` 的值以 `String` 无损保存。已经是字符串、
`null`、或 `true`/`false` 的值保持原样（见 [`api/types`](#apitypes-线类型)）。

## `api::types` 线类型

[`../../src/api/types.rs`](../../src/api/types.rs) 中的结构体直接对应后端 JSON 协议。
它们都是 `#[derive(Serialize, Deserialize)]` 且 `Default`。

### 枚举

| 枚举 | 变体与值 | 用途 |
| --- | --- | --- |
| `UploadMediaType` | `Image=1, Video=2, File=3, Voice=4` | CDN 上传媒体类型 |
| `MessageType` | `None=0, User=1, Bot=2` | 消息方向 |
| `MessageItemType` | `None, Text, Image, Voice, File, Video, ToolCallStart, ToolCallResult` | 内容项类型 |
| `MessageState` | `New=0, Generating=1, Finish=2` | 消息状态 |
| `TypingStatus` | `Typing=1, Cancel=2` | 输入状态 |

每个枚举都有 `value()`（转回 `i64`）和 `from_value()`（从 `i64` 解析，无效值
返回 `None`）。

### 请求/响应结构体（核心）

| 类型 | 说明 |
| --- | --- |
| `BaseInfo` | `{ channel_version, bot_agent }` |
| `GetUploadUrlReq` / `GetUploadUrlResp` | 预签名上传 |
| `WeixinMessage` | 消息本体，含 `message_id: Option<String>`、`item_list`、`context_token`、`run_id` |
| `MessageItem` | 单个内容项，含 `msg_id: Option<String>`、`ref_msg`、`text_item`、`image_item`… |
| `TextItem` / `ImageItem` / `VoiceItem` / `FileItem` / `VideoItem` | 各类内容项 |
| `CdnMedia` | `{ encrypt_query_param, aes_key, encrypt_type, full_url }` |
| `RefMessage` / `PartialText` | 内联引用及其子串 |
| `ToolCallStartItem` / `ToolCallResultItem` | 工具调用项 |
| `GetUpdatesReq` / `GetUpdatesResp` | 长轮询请求与响应，含 `get_updates_buf` 游标 |
| `SendMessageReq` / `SendMessageResp` | 发送消息 |
| `SendTypingReq` / `SendTypingResp` | 输入状态 |
| `GetConfigResp` | 含 `typing_ticket` |
| `NotifyStopReq/Resp` / `NotifyStartReq/Resp` | 生命周期心跳 |

### 重要的 `impl` 辅助

```rust
impl GetUpdatesResp {
    pub fn is_api_error(&self) -> bool;            // ret != 0 || errcode != 0
    pub fn is_stale_token(&self, stale_errcode: i64) -> bool;
}

impl SendMessageResp {
    pub fn is_error(&self) -> bool;                // Some(ret) && ret != 0（缺失 ret 不算错）
}

impl GetConfigResp {
    pub fn is_ok(&self) -> bool;                   // ret == Some(0)
}
```

### 常量

```rust
pub const DEFAULT_BOT_AGENT: &str = "OpenClaw";
pub const BOT_AGENT_MAX_LEN: usize = 256;
```

> 关于 uint64 的无损处理：`WeixinMessage` 的 `message_id` 与 `MessageItem` 的
> `msg_id`、`svr_id` 在线上是 JSON 数字，可能超过 `i64::MAX`。SDK 在解析前先用
> `quote_lossless_ids` 将它们加引号，因此 Rust 侧以 `String` 保存，不会静默
> 截断或溢出。

## `session_guard` 子模块

[`../../src/api/session_guard.rs`](../../src/api/session_guard.rs) ：当 `getUpdates`
返回 `errcode == -14`（令牌过期）时，暂停该账户所有 API 调用一小时。

```rust
pub const STALE_TOKEN_ERRCODE: i64 = -14;
pub const SESSION_PAUSE_DURATION: Duration = Duration::from_secs(60 * 60);

pub fn pause_session(account_id: &str);
pub fn is_session_paused(account_id: &str) -> bool;
pub fn get_remaining_pause_ms(account_id: &str) -> u128;
pub fn assert_session_active(account_id: &str) -> Result<()>;
pub fn resume_session(account_id: &str);
pub fn reset_session_pauses();          // 仅测试

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

- `pause_session` 之后，该账户下所有 API 请求都应被 `assert_session_active`
  拦截，返回 `ErrorKind::Unknown`（消息含剩余分钟数与 `STALE_TOKEN_ERRCODE`）。
- 各账户相互独立；过期条目在查询时惰性清理。
- 重新登录成功后，调用 `resume_session` 提前解除暂停。

## 用法示例

### 直接驱动长轮询

```rust,no_run
use ilink_sdk::api::{WeixinApiClient, WeixinApiOptions, GetUpdatesReq};
use ilink_sdk::error::{ErrorKind, Result};

async fn loop_updates(api: &WeixinApiClient, mut buf: String) -> Result<()> {
    loop {
        let req = GetUpdatesReq { get_updates_buf: Some(buf.clone()), ..Default::default() };
        let resp = api.get_updates(&req, None, None).await?;
        // 业务错误（如 -14 令牌过期）应在此处理
        if resp.is_stale_token(-14) {
            return Err(ilink_sdk::error::Error::new(
                ErrorKind::Unknown, "token stale"));
        }
        if let Some(msgs) = resp.msgs.as_ref() {
            for m in msgs { /* 处理消息 */ }
        }
        if let Some(next) = resp.get_updates_buf.clone() {
            buf = next;
        }
    }
}
```

### 注入 mock 传输做断言（示意）

```rust,no_run
use ilink_sdk::api::{HttpTransport, WeixinApiOptions, WeixinApiClient};
use std::sync::Arc;

// 自定义 transport 实现 HttpTransport::request，
// 返回 (StatusCode, HeaderMap, Vec<u8>) 即可在测试中校验头与体。
let client = WeixinApiClient::with_transport(
    WeixinApiOptions::new("https://ilinkai.weixin.qq.com").with_token("t"),
    Arc::new(MyMockTransport),
);
```

## 设计要点与注意事项

1. **优先用高层 `WeixinClient`。** 除非你需要精确的游标控制、自定义传输或生命周期
   管理，否则 `WeixinClient` 已经封装了账户解析、二维码登录与发送器。
2. **`get_updates` 不把超时当错误。** 这是有意设计：超时/取消返回空 `ret:0`，
   调用方继续轮询即可。真正需要中止时，传入 `CancellationToken` 或用返回错误
   的循环条件。
3. **`send_message` 的「缺失 ret」语义。** 不要假设 `SendMessageResp.ret`
   一定存在；缺失时按成功处理，仅在 `Some(r)` 且 `r != 0` 时报错。
4. **`base_url` 不能为空。** 空串或纯空白会返回 `ErrorKind::Unknown`；构造时
   会尽量规范化结尾的 `/`。
5. **`bot_agent` 会被 sanitize。** 传入非法 UA 串会被清洗；结果超过 256 字节会
   丢弃尾部 token；空结果回退为 `OpenClaw`。
