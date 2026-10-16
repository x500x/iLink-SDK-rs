# 消息收发

> English: [English](../../guides/messaging.md)

本指南涵盖接收消息（长轮询）、发送文本与媒体、输入状态指示器，以及会话上下文
token。

## 接收消息（长轮询）

`WeixinApiClient::get_updates` 就是收件箱。它对服务端发起长轮询，服务端会持有该
请求直到有消息到达，或达到长轮询超时（默认 35s，`DEFAULT_LONG_POLL_TIMEOUT_MS`）。

```rust,no_run
# async fn run(client: ilink_sdk::WeixinClient) -> ilink_sdk::Result<()> {
use ilink_sdk::api::types::{GetUpdatesReq, MessageItemType};
use ilink_sdk::storage::sync_buf::load_for_account;
use tokio_util::sync::CancellationToken;

let account_id = client.account_id().to_string();
let mut cursor = load_for_account(&account_id);

// 可选：从另一个任务取消长轮询。is_abort() 风格的 error 仍会返回携带相同游标
// 的空 ret:0，因此循环仍然正确。
let cancel = CancellationToken::new();

loop {
    let resp = client.api().get_updates(
        &GetUpdatesReq { get_updates_buf: cursor.clone(), sync_buf: None },
        None,   // 默认 35s 长轮询超时
        Some(cancel.clone()),
    ).await?;

    if let Some(buf) = resp.get_updates_buf {
        cursor = Some(buf);
        ilink_sdk::storage::sync_buf::save_for_account(&account_id, &cursor.clone().unwrap_or_default());
    }

    for msg in resp.msgs.unwrap_or_default() {
        for item in msg.item_list.unwrap_or_default() {
            match item.r#type.and_then(MessageItemType::from_value) {
                Some(MessageItemType::Text) => { /* item.text_item.text */ }
                Some(MessageItemType::Voice) => { /* item.voice_item.text = 转写 */ }
                Some(MessageItemType::Image) => { /* 通过 cdn 下载 */ }
                _ => {}
            }
        }
    }

    if cancel.is_cancelled() { break; }
}
# Ok(())
# }
```

- 游标（`get_updates_buf`）通过 [`save_for_account`](../../../src/storage/sync_buf.rs)
  持久化，以便重启后续传，而不是重放整个历史。
- 客户端超时或外部取消会返回一个**携带相同游标、`ret: 0` 的空响应**——直接循环即可。
  其他错误以 [`Error`] 传播。
- 用 [`GetUpdatesResp::is_api_error`] / [`is_stale_token`] 检测业务错误
  （如 `errcode == -14` token 失效）。

## 发送文本

```rust,no_run
# async fn run(client: ilink_sdk::WeixinClient) -> ilink_sdk::Result<()> {
let to = "someone@im.wechat";

client.sender().send_text(
    to,
    "来自 Rust 的问候",
    &ilink_sdk::messaging::SendOpts {
        context_token: Some("ctx-abc".into()),   // 回显入站 token
        run_id: None,
    },
).await?;
# Ok(())
# }
```

`context_token` **是将回复路由到同一会话所必需的**。缺失该 token 会被记录为警告，
但不会阻断发送。如何从入站消息捕获它见下文。

## 发送文件 / 媒体

`send_file` 会把本地文件上传到 CDN（AES-128-ECB 加密），然后发送一条引用该文件的
消息。媒体类型（图片 / 视频 / 文件）根据文件扩展名自动判断。

```rust,no_run
# async fn run(client: ilink_sdk::WeixinClient) -> ilink_sdk::Result<()> {
use std::path::Path;
use ilink_sdk::storage::accounts::CDN_BASE_URL;

let to = "someone@im.wechat";
let cdn_base_url = client.account().cdn_base_url.clone();

client.sender().send_file(
    to,
    Path::new("./photo.png"),
    "看这张图",            // 可选说明（作为单独文本项发送）
    &ilink_sdk::messaging::SendOpts::default(),
    &cdn_base_url,
).await?;
# Ok(())
# }
```

还有针对原始字节的便捷方法：

- `send_bytes(to, data, file_name, caption, opts, cdn_base_url)` —— 字节作为通用文件附件。
- `send_image_bytes(to, data, extension, caption, opts, cdn_base_url)` —— 字节作为图片。
- `send_item(to, MessageItem, opts)` —— 发送一个预先构建好的项。

所有媒体发送都经过 [`cdn::upload_media`]：它调用 `get_upload_url`、用 AES-128-ECB
加密、POST 密文，然后返回会成为 `CdnMedia` 引用的下载参数。

## 输入状态指示器

在发送回复之前，你可以显示输入状态。先用 `get_config` 获取输入票据，再调用
`send_typing`（或使用 [`TypingHelper`]）。

```rust,no_run
# async fn run(client: ilink_sdk::WeixinClient, user_id: &str) -> ilink_sdk::Result<()> {
use ilink_sdk::messaging::Typing;

let cfg = client.api().get_config(user_id, None).await?;
let ticket = cfg.typing_ticket.unwrap_or_default();

// 空票据是空操作（该账户可能不支持输入状态）。
ilink_sdk::messaging::send_typing(client.api(), user_id, &ticket, Typing::Typing).await?;
// ... 发送回复 ...
ilink_sdk::messaging::send_typing(client.api(), user_id, &ticket, Typing::Cancel).await?;
# Ok(())
# }
```

- `Typing::Typing` → 线上 `status = 1`；`Typing::Cancel` → `status = 2`。
- 空票据会短路，不发起网络请求。

## 会话上下文 token

服务端会为每条入站消息颁发一个 `context_token`。**你必须把它回显到同一会话的每一
条出站发送上**，否则回复无法正确归并。SDK 提供两个工具：

### `ContextTokenStore`（持久化、多账户）

```rust,no_run
# fn run() {
use ilink_sdk::messaging::context::ContextTokenStore;

let mut store = ContextTokenStore::new();       // 默认指向 accounts 目录
store.set("acct", "user@im.wechat", "ctx-xyz");  // 内存 + 磁盘
let token = store.get("acct", "user@im.wechat"); // Some("ctx-xyz")
let _ = store.restore("acct");                   // 重新加载持久化的 token
# }
```

启动时调用 `restore(account_id)` 以重新加载上一次运行写入的 token。

### 构建面向主机的上下文

当入站消息到达时，把它转换为你的应用使用的 [`MsgContext`]：它会拍平正文、附加已下载
的媒体路径，并把行内引用（`ref_msg`）元数据提取到 `reply_to_*` 字段中。

```rust,no_run
# fn run(msg: ilink_sdk::api::types::WeixinMessage, account_id: &str, media: ilink_sdk::cdn::download::InboundMedia) {
use ilink_sdk::messaging::context::message_to_context;

let ctx = message_to_context(&msg, account_id, &media);
println!("正文 = {}", ctx.body);
println!("context_token = {:?}", ctx.context_token);
# }
```

- `message_to_context` 通过 [`body_from_items`] 推导正文（先文本，再语音转写，再媒体
  占位符如 `[图片]`）。
- `remember_context_token(&mut store, &ctx)` 把 token 存回 `ContextTokenStore`。
- 行内引用会填充 `reply_to_id`、`reply_to_body`、`reply_to_is_quote`。

## 行内引用与引用缓存

较新的微信客户端在引用消息时只发送一个服务端消息 id。为了让后续引用能恢复其原始
文本/媒体，请开启本地引用缓存：

```rust,no_run
# fn run(client: &mut ilink_sdk::WeixinClient) -> ilink_sdk::Result<()> {
client.enable_quote_cache()?;        // 打开 ref-messages.sqlite
# Ok(())
# }
```

该缓存（[`QuoteStore`]）在本地按账户 + 会话 + 消息 id 存储每条消息的正文与受管媒体，
并带有保留期与容量上限（见[存储与状态](../guides/storage.md)）。`Direction::Inbound`
与 `Direction::Outbound` 都支持。
