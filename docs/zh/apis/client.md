# 高层客户端 API

高层客户端 [`../../src/client/mod.rs`](../../src/client/mod.rs) 把底层 API 客户端、账户
存储、二维码登录、媒体管线与引用缓存组合成一个易用的句柄。大多数应用只需要
`WeixinClientBuilder` 和 `WeixinClient`，无需直接接触底层端点。

## `WeixinConfig`

```rust
#[derive(Debug, Clone, Default)]
pub struct WeixinConfig {
    pub account_id: Option<String>,      // 指定操作的账户；None 时回退到唯一已注册账户
    pub base_url: Option<String>,        // 覆盖 API base URL
    pub cdn_base_url: Option<String>,    // 覆盖 CDN base URL
    pub route_tag: Option<String>,       // 作为 SKRouteTag 转发
    pub bot_agent: Option<String>,       // 可选可观测标识，作为 bot_agent
    pub timeout_ms: Option<u64>,         // 每请求超时覆盖
}
```

所有字段都是可选的，未设置时由账户存储与默认值决定。

## `WeixinClientBuilder`

构造器，链式调用后 `build()` 出 `WeixinClient`。

```rust
#[derive(Debug, Default, Clone)]
pub struct WeixinClientBuilder;

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

    /// 从账户存储解析凭据并构建客户端。
    pub async fn build(self) -> Result<WeixinClient>;
}
```

### `build()` 的解析规则

1. `account_id` 优先；为空时取 `accounts::list_indexed_account_ids()` 的第一个。
2. 若零个或多个账户都无法确定唯一账户 → 报错：`"no accounts registered —
   run the QR-code login flow first"`。
3. `account_id` 空串或纯空白 → 报错：`"account_id is required (no default
   account)"`。
4. 解析出的账户若无 `token` → 报错：`"account {id} has no bot token — run the
   QR-code login flow"`。
5. `base_url` / `cdn_base_url` 非空覆盖；`route_tag` 取 builder 或账户存储中的值；
   `timeout_ms` 设置默认超时；`bot_agent` 覆盖 `bot_agent`。

## `WeixinClient`

```rust
#[derive(Clone)]
pub struct WeixinClient;
```

> **重要：** `WeixinClient` 可 `Clone`，但**不是** `Sync`。原因是其内部持有的
> `QuoteStore` 是 `Send` 而非 `Sync`（rusqlite 内部用 `RefCell`）。因此它的使用
> 方式是「克隆后各自持有」，而不是跨任务以 `&self` 共享。单个任务内使用或
> `Arc<WeixinClient>` 配合 `clone()` 都是安全的。

### 构造

```rust
impl WeixinClient {
    /// 从显式配置构造（见上方解析规则）。
    pub async fn from_config(config: WeixinConfig) -> Result<Self>;

    /// 直接从 QrSession 构造，不接触账户存储。适合自定义持久化。
    pub async fn from_session(session: QrSession, bot_agent: Option<String>) -> Result<Self>;

    /// 持久化凭据后立即返回该账户的客户端。
    pub async fn save_and_load(account_id: &str, update: AccountUpdate) -> Result<Self>;
}
```

`from_session` 直接用登录得到的 `QrSession`，跳过了账户存储读写。它会用
`storage::accounts::CDN_BASE_URL` 作为 CDN base URL，并把 `QrSession` 的字段映射到
内部 `StoredAccount`。

### 访问器与子句柄

```rust
impl WeixinClient {
    pub fn account(&self) -> &StoredAccount;          // 已解析账户信息
    pub fn account_id(&self) -> &str;                // 生效的 account id
    pub fn token(&self) -> Option<&str>;             // 当前 bot token
    pub fn api(&self) -> &WeixinApiClient;           // 底层客户端（高级调用）

    pub fn qr_login(&self) -> QrLogin<'_>;           // 绑定到本账户的二维码登录器
    pub fn sender(&self) -> MessageSender<'_>;       // 出站消息发送器

    pub fn enable_quote_cache(&mut self) -> Result<()>;   // 开启本地引用缓存
    pub fn quote_store(&self) -> Option<&Arc<QuoteStore>>; // 缓存（若已开启）

    pub async fn notify_start(&self) -> Result<()>;   // 通知后端：本客户端启动
    pub async fn notify_stop(&self) -> Result<()>;    // 通知后端：本客户端停止
}
```

### `enable_quote_cache`

默认 `WeixinClient` 不带引用缓存。调用 `enable_quote_cache()` 会打开一个 SQLite
支持的 [`QuoteStore`](../apis/quote-cache.md)（基于默认策略）。开启后可经
`quote_store()` 取得引用。

```rust,no_run
use openclaw_weixin::prelude::*;
# async fn example() -> openclaw_weixin::Result<()> {
let mut client = WeixinClientBuilder::new().account_id("acct").build().await?;
client.enable_quote_cache()?;
if let Some(store) = client.quote_store() {
    // 使用 store.find(...) / store.put(...)
}
# Ok(())
# }
```

## 用法示例

### 从已注册账户构建并发送

```rust,no_run
use openclaw_weixin::prelude::*;

#[tokio::main]
async fn main() -> openclaw_weixin::Result<()> {
    let client = WeixinClientBuilder::new()
        .account_id("b0f5860fdecb-im-bot")
        .build()
        .await?;

    client.notify_start().await?;

    let sender = client.sender();
    let result = sender.send_text(
        "someone@im.wechat",
        "你好，来自 Rust",
        &SendOpts { context_token: Some("ctx-123".into()), ..Default::default() },
    ).await?;
    println!("client id: {}, server id: {:?}", result.message_id, result.server_message_id);

    client.notify_stop().await?;
    Ok(())
}
```

### 登录后立即保存并构建

```rust,no_run
use openclaw_weixin::prelude::*;
use openclaw_weixin::storage::accounts::{self, AccountUpdate};

# async fn example(session: QrSession) -> openclaw_weixin::Result<()> {
let account_id = accounts::normalize_account_id(&session.bot_id);
let client = WeixinClient::save_and_load(&account_id, AccountUpdate {
    token: Some(session.token),
    base_url: Some(session.base_url),
    user_id: Some(session.user_id),
}).await?;
# Ok(())
# }
```

### 从 QrSession 直接构建（不落盘）

```rust,no_run
use openclaw_weixin::prelude::*;

# async fn example(session: QrSession) -> openclaw_weixin::Result<()> {
let client = WeixinClient::from_session(session, Some("MyBot/1.0".into())).await?;
client.sender().send_text("u@im.wechat", "临时会话", &SendOpts::default()).await?;
# Ok(())
# }
```

## 设计要点与注意事项

1. **`Sync` 限制。** 不要试图把 `&WeixinClient` 跨 `tokio::spawn` 共享；要么
   克隆后 move 进任务，要么用 `Arc<WeixinClient>` 配合 `clone()`。这是 rusqlite
   引用计数的固有约束，clippy 会对此 `Arc::new(QuoteStore)` 给出提示但不应消除。
2. **`build()` 需要已登录账户。** 若目录里没有账户（或只有一个空 token），构建会
   失败；请先完成二维码登录并 `register_account_id`。
3. **`api()` 暴露底层能力。** 需要 `get_updates` 长轮询、自定义 `get_config` 等时，
   通过 `client.api()` 拿到底层 `WeixinApiClient`，所有底层方法都可用。
4. **`notify_start` / `notify_stop` 是生命周期心跳。** 它们不返回业务数据，仅通知
   后端通道客户端的启停；失败通常可忽略或仅记录。
5. **`account_id` 会被规范化。** 存储层使用文件系统安全的 key（`@im.bot` →
   `-im-bot`），`build()` 内部会处理，调用方传入原始或规范化 id 皆可。
