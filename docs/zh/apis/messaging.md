# 消息收发 API

消息收发覆盖**发送**与**接收**两端。发送由 [`MessageSender`](../apis/messaging.md#messagesender)
完成，接收通过底层 [`get_updates`](../apis/api-client.md) 长轮询拉取后再经上下文与
媒体管线处理。本节还包含输入状态（`Typing` / `TypingHelper`）与入站上下文
（`MsgContext` / `ContextTokenStore`）。

相关模块：

- [`../../src/messaging/outbound.rs`](../../src/messaging/outbound.rs) —— 发送
- [`../../src/messaging/typing.rs`](../../src/messaging/typing.rs) —— 输入状态
- [`../../src/messaging/context.rs`](../../src/messaging/context.rs) —— 入站上下文与 token 存储

## `SendOpts`

```rust
#[derive(Debug, Clone, Default)]
pub struct SendOpts {
    pub context_token: Option<String>,   // 取自入站消息的会话上下文 token，回发时务必回显
    pub run_id: Option<String>,          // 可选的生成/运行 id
}
```

> **`context_token` 是关键。** 微信后端要求每条出站消息回显其所属会话的
> `context_token`（来自入站消息）。缺失时仅记录 `warn` 并继续发送（与 TS 行为一致），
> 但消息可能无法正确路由/触达。接收侧请通过 `MsgContext.context_token` 取得并保存。

## `SendResult`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendResult {
    pub message_id: String,                 // 客户端生成的 id（"prefix:ts-hex"）
    pub server_message_id: Option<String>, // 后端返回的消息 id（若有）
}
```

`message_id` 是 SDK 本地生成（`generate_id("openclaw-weixin")`），可用于去重或日志；
`server_message_id` 是后端分配的（以 `String` 无损保存）。

## `MessageSender`

绑定到一个 [`WeixinApiClient`](../apis/api-client.md) 与账户 id 的发送器。

```rust
pub struct MessageSender<'a> { /* api, account_id */ }

impl<'a> MessageSender<'a> {
    pub fn new(api: &'a WeixinApiClient, account_id: impl Into<String>) -> Self;
    pub fn account_id(&self) -> &str;

    pub async fn send_text(&self, to: &str, text: &str, opts: &SendOpts) -> Result<SendResult>;
    pub async fn send_item(&self, to: &str, item: MessageItem, opts: &SendOpts) -> Result<SendResult>;
    pub async fn send_file(
        &self, to: &str, path: &Path, caption: &str, opts: &SendOpts, cdn_base_url: &str
    ) -> Result<SendResult>;
    pub async fn send_bytes(
        &self, to: &str, data: &[u8], file_name: &str, caption: &str, opts: &SendOpts, cdn_base_url: &str
    ) -> Result<SendResult>;
    pub async fn send_image_bytes(
        &self, to: &str, data: &[u8], extension: &str, caption: &str, opts: &SendOpts, cdn_base_url: &str
    ) -> Result<SendResult>;
}
```

通常不直接 `MessageSender::new`，而是经 `WeixinClient::sender()` 获取（已绑定账户）。

### `send_text`

发送纯文本。空 `text` 不发送任何 `item_list`（仅构造空消息体）。

```rust,no_run
# use openclaw_weixin::prelude::*;
# async fn example(sender: &MessageSender<'_>) -> openclaw_weixin::Result<()> {
let r = sender.send_text(
    "someone@im.wechat",
    "你好",
    &SendOpts { context_token: Some("ctx-1".into()), ..Default::default() },
).await?;
println!("sent {}", r.message_id);
# Ok(())
# }
```

### `send_item`

发送一个手工构造的 [`MessageItem`](../apis/api-client.md#apitypes-线类型)（例如带
`ref_msg` 的引用项、工具调用项）。

```rust,no_run
# use openclaw_weixin::prelude::*;
# use openclaw_weixin::api::types::{MessageItem, MessageItemType, TextItem};
# async fn example(sender: &MessageSender<'_>) -> openclaw_weixin::Result<()> {
let item = MessageItem {
    r#type: Some(MessageItemType::Text.value()),
    text_item: Some(TextItem { text: Some("自定义项".into()) }),
    ..Default::default()
};
sender.send_item("someone@im.wechat", item, &SendOpts::default()).await?;
# Ok(())
# }
```

### `send_file` / `send_bytes` / `send_image_bytes`

按 MIME 类型（视频 / 图片 / 文件）路由上传，先经 [CDN 上传管线](../apis/cdn.md)
预签名并加密上传，再分别发送「说明文字 + 媒体项」作为独立的请求。

- `send_file`：从本地路径发送，自动 MIME 探测。可选 `caption` 作为前置说明文字。
- `send_bytes`：从内存字节发送，需要显式 `file_name`；临时落盘后走 `send_file`。
- `send_image_bytes`：从内存字节发送图片，需 `extension`（如 `"png"`）。

`cdn_base_url` 参数来自账户配置（通常 `storage::accounts::CDN_BASE_URL` 或
`client.account().cdn_base_url`）。

```rust,no_run
# use openclaw_weixin::prelude::*;
# async fn example(sender: &MessageSender<'_>) -> openclaw_weixin::Result<()> {
use std::path::Path;
let _ = sender.send_file(
    "someone@im.wechat",
    Path::new("./photo.png"),
    "看看这张图",
    &SendOpts::default(),
    "https://novac2c.cdn.weixin.qq.com/c2c",
).await?;
# Ok(())
# }
```

## 接收：长轮询 + 上下文处理

SDK 不在内部维护接收循环；调用方需自行驱动 `get_updates` 长轮询（见
[api-client.md](api-client.md)）。拿到 `WeixinMessage` 后，可用
[`context`](#contexttokestore--mscontext) 模块构造 `MsgContext` 并解析媒体。

### `MsgContext`

面向宿主应用的入站上下文（对应 TS 的 `WeixinMsgContext`）。

```rust
pub struct MsgContext {
    pub body: String,
    pub from: String,
    pub to: String,
    pub account_id: String,
    pub originating_channel: String,      // "openclaw-weixin"
    pub originating_to: String,
    pub message_sid: String,             // 客户端生成的 sid
    pub message_sid_full: Option<String>, // 后端消息 id（无损）
    pub timestamp: Option<i64>,
    pub provider: String,                 // "openclaw-weixin"
    pub chat_type: String,                // "direct"
    pub session_key: Option<String>,      // 路由后由宿主设置
    pub context_token: Option<String>,    // 回发所需
    pub media_path: Option<String>,
    pub media_type: Option<String>,
    pub media_paths: Vec<String>,
    pub media_types: Vec<String>,
    pub media: Vec<MediaEntry>,
    pub channel_prompt_context: Vec<String>,
    pub reply_to_id: Option<String>,
    pub reply_to_body: Option<String>,
    pub reply_to_quote_text: Option<String>,
    pub reply_to_is_quote: Option<bool>,
}
```

`MediaEntry`：

```rust
pub struct MediaEntry {
    pub path: String,
    pub content_type: Option<String>,
    pub file_name: Option<String>,
    pub message_id: Option<String>,
}
```

### `ContextTokenStore`

内存 + 磁盘的 token 存储，key 为 `account:user`。

```rust
impl ContextTokenStore {
    pub fn new() -> Self;                                // 默认账户目录
    pub fn with_dir(dir: impl Into<PathBuf>) -> Self;    // 显式目录
    pub fn set(&mut self, account_id: &str, user_id: &str, token: &str);
    pub fn get(&self, account_id: &str, user_id: &str) -> Option<&str>;
    pub fn accounts_for_user<'a>(&self, account_ids: &'a [String], user_id: &str) -> Vec<&'a str>;
    pub fn clear_account(&mut self, account_id: &str);
    pub fn restore(&mut self, account_id: &str) -> usize; // 返回加载条数
}
```

持久化文件为 `<accounts_dir>/<account_id>.context-tokens.json`。

### 上下文相关自由函数

```rust
pub fn is_media_item(item: &MessageItem) -> bool;
pub fn media_label(item_type: Option<i64>) -> &'static str;   // "[图片]" / "[视频]"……
pub fn message_id(msg: &WeixinMessage) -> Option<String>;
pub fn body_from_items(items: &[MessageItem]) -> String;       // 文本/语音转录/媒体占位
pub fn message_to_context(msg: &WeixinMessage, account_id: &str, media: &InboundMedia) -> MsgContext;
pub fn remember_context_token(store: &mut ContextTokenStore, ctx: &MsgContext) -> bool;
```

- `body_from_items`：文本项返回文本；语音项带转录则返回转录文本，否则返回
  `[语音]` 占位；其它媒体返回类型占位（如 `[图片]`）。
- `message_to_context`：把 `WeixinMessage` 与已下载的 `InboundMedia` 合并为
  `MsgContext`，解析媒体路径与内联 `reply_to_*` 引用字段。
- `remember_context_token`：把 `MsgContext.context_token` 存入 token 存储，返回是否成功。

### 入站处理完整示例（示意）

```rust,no_run
use openclaw_weixin::prelude::*;
use openclaw_weixin::messaging::context::{ContextTokenStore, message_to_context};
use openclaw_weixin::cdn::download::{download_item_media, MediaSink, InboundMedia};

# async fn example(
#     api: &WeixinApiClient, msg: WeixinMessage, account_id: &str, cdn: &str,
#     store: &mut ContextTokenStore, sink: &MediaSink,
# ) -> openclaw_weixin::Result<()> {
// 假设 msg 来自 get_updates 长轮询
let media = download_item_media(
    msg.item_list.as_deref().unwrap_or(&[]).first().unwrap(),
    cdn, sink, "inbound",
).await; // 注意：实际应遍历 item_list

let ctx = message_to_context(&msg, account_id, &media);
println!("from={} body={} token={:?}", ctx.from, ctx.body, ctx.context_token);
store.set(account_id, &ctx.from, ctx.context_token.as_deref().unwrap_or(""));
# Ok(())
# }
```

## 输入状态 `Typing`

[`../../src/messaging/typing.rs`](../../src/messaging/typing.rs)

```rust
pub enum Typing { Typing, Cancel }
impl From<Typing> for TypingStatus { /* Typing->1, Cancel->2 */ }

/// 便捷函数：空 ticket 为空操作（不触网）。
pub async fn send_typing(
    api: &WeixinApiClient,
    ilink_user_id: &str,
    ticket: &str,
    status: Typing,
) -> Result<()>;

/// 绑定 api + user_id + ticket 的助手。
pub struct TypingHelper<'a> { /* ... */ }
impl<'a> TypingHelper<'a> {
    pub fn new(api: &'a WeixinApiClient, user_id: impl Into<String>, ticket: impl Into<String>) -> Self;
    pub fn enabled(&self) -> bool;                 // ticket 非空才为 true
    pub async fn start(&self) -> Result<()>;       // 显示输入状态
    pub async fn stop(&self) -> Result<()>;        // 隐藏输入状态
}
```

`ticket` 来自 `get_config` 的 `typing_ticket`（见
[api-client.md](api-client.md)）。空 ticket 时 `send_typing` 直接返回 `Ok`，不会
触网——因为某些账户不支持输入状态，`get_config` 不会下发 ticket。

示例：

```rust,no_run
# use openclaw_weixin::prelude::*;
# use openclaw_weixin::messaging::typing::{TypingHelper, send_typing, Typing};
# async fn example(api: &WeixinApiClient, user: &str, ticket: &str) -> openclaw_weixin::Result<()> {
let helper = TypingHelper::new(api, user, ticket);
if helper.enabled() {
    helper.start().await?;
    // ... 生成回复 ...
    helper.stop().await?;
}

// 或直接调用自由函数
send_typing(api, user, ticket, Typing::Typing).await?;
# Ok(())
# }
```

## 设计要点与注意事项

1. **`context_token` 必须回显。** 出站时通过 `SendOpts.context_token` 携带入站消息
   中的 token；缺失仅告警不阻断，但会影响投递。
2. **`send_file` 会拆成多条请求。** 说明文字 + 媒体项分别发送，因此一个 `send_file`
   可能产生多条 `WeixinMessage`；`SendResult` 仅返回最后一次的结果（`server_message_id`
   为最后一个）。若需逐条结果，使用 `send_item` 自行循环。
3. **`send_bytes` / `send_image_bytes` 会临时落盘。** 它们写入 `std::env::temp_dir()`，
   发送后删除；若进程被强杀可能残留临时文件。
4. **接收循环由调用方负责。** SDK 不提供监控循环；请用 `get_updates` 长轮询，遇到
   `is_stale_token(-14)` 时走 [`session_guard`](api-client.md#session_guard-子模块) 流程。
5. **`ContextTokenStore` 跨重启持久化。** `new()` 使用默认账户目录；若多实例共享状态
   目录，注意并发写入（实现为每账户一个 JSON 文件，覆盖写）。
6. **媒体下载依赖 `InboundMedia`。** `message_to_context` 需要先把媒体下载到本地
   （`download_item_media` / `download_item_to_path`），再传入；未下载时 `media_path`
   为空。
