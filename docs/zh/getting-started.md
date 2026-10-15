# 快速开始

本指南带你从一个空项目，到一个可以二维码登录并发送消息的可运行微信机器人。

## 1. 添加依赖

```toml
[dependencies]
ilink-sdk = "2.4"
tokio = { version = "1", features = ["full"] }
```

> 本 SDK 需要 Tokio。如果只需要库（不需要内置的 `weixin` 二进制），可以关闭
> 默认的 `cli` feature：
>
> ```toml
> ilink-sdk = { version = "2.4", default-features = false }
> ```

## 2. 二维码登录

`QrLogin::start` 返回一个 URL，你把它渲染（或打印）出来让用户扫码。
`QrLogin::wait` 驱动轮询循环直到完成。成功后你会拿到一个 [`QrSession`]，
其中包含 bot `token`、`bot_id`、base URL，以及配对的 `user_id`。

```rust,no_run
use ilink_sdk::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = WeixinApiClient::new(WeixinApiOptions::new(DEFAULT_BASE_URL))?;
    let qr = QrLogin::new(&api);
    let start = qr.start(None).await?;
    println!("请扫描此二维码：{}", start.qrcode_url);

    // 等待至多 8 分钟，让用户在手机上扫码并确认。闭包用于：当服务端要求
    // 验证码时从 stdin 读取，以及当二维码刷新时重新渲染。
    let result = qr
        .wait(
            &start.session_key,
            std::time::Duration::from_secs(480),
            |_retry| async {
                let mut line = String::new();
                std::io::stdin().read_line(&mut line)?;
                Ok(line.trim().to_string())
            },
            |_| async {},
        )
        .await?;

    let session = result.session.expect("登录必须成功");
    let account_id = ilink_sdk::storage::accounts::normalize_account_id(&session.bot_id);

    // 持久化 token，以便后续运行直接构建客户端。
    ilink_sdk::storage::accounts::save_account(
        &account_id,
        &ilink_sdk::storage::accounts::AccountUpdate {
            token: Some(session.token),
            base_url: Some(session.base_url),
            user_id: Some(session.user_id),
        },
    )?;
    ilink_sdk::storage::accounts::register_account_id(&account_id)?;

    Ok(())
}
```

`start.qrcode_url` 是用户在手机上打开的 URL；你也可以把它渲染成二维码图片
（例如用 `qrcode` crate），或在 CLI 中直接打印到终端。

## 3. 构建客户端并发送消息

凭证持久化之后，你几乎不再需要碰二维码登录。从已存储的账户构建一个
[`WeixinClient`]：

```rust,no_run
use ilink_sdk::prelude::*;

# async fn run() -> ilink_sdk::Result<()> {
let client = WeixinClientBuilder::new()
    .account_id("deadbeef-im-bot")          // 可选；默认使用唯一已注册账户
    .build()
    .await?;

client
    .sender()
    .send_text(
        "someone@im.wechat",
        "来自 Rust 的问候",
        &ilink_sdk::messaging::SendOpts::default(),
    )
    .await?;
# Ok(())
# }
```

对于一次性程序，也可以不碰账户存储，直接从一个 [`QrSession`] 构建客户端：

```rust,no_run
# async fn run(session: ilink_sdk::messaging::QrSession) -> ilink_sdk::Result<()> {
let client = WeixinClient::from_session(session, None).await?;
# Ok(())
# }
```

## 4. 接收消息（长轮询）

```rust,no_run
# async fn run(client: ilink_sdk::WeixinClient) -> ilink_sdk::Result<()> {
use ilink_sdk::api::types::{GetUpdatesReq, MessageItemType};
use ilink_sdk::storage::sync_buf::{load_for_account, save_for_account};

let account_id = client.account_id().to_string();
let mut cursor = load_for_account(&account_id); // 跨重启续传

loop {
    let resp = client.api().get_updates(
        &GetUpdatesReq { get_updates_buf: cursor.clone(), sync_buf: None },
        None,    // 使用默认 35s 长轮询超时
        None,    // 无外部取消 token
    ).await?;

    if let Some(buf) = &resp.get_updates_buf {
        cursor = Some(buf.clone());
        let _ = save_for_account(&account_id, buf);
    }

    for msg in resp.msgs.unwrap_or_default() {
        for item in msg.item_list.unwrap_or_default() {
            if item.r#type == Some(MessageItemType::Text.value()) {
                if let Some(t) = item.text_item.and_then(|i| i.text) {
                    println!("来自 {}：{}", msg.from_user_id.unwrap_or_default(), t);
                }
            }
        }
    }
}
# }
```

> 当长轮询超时或被取消时，`get_updates` 会返回一个**携带相同游标、且 `ret: 0`
> 的空响应**。这是设计使然——直接循环即可。真正的传输错误会以 [`Error`] 传播。

## 5. 状态文件存放位置

所有凭证与缓存都存放在 SDK 的状态目录下（见[存储与状态](guides/storage.md)）。
默认位置：

```text
$HOME/.openclaw/openclaw-weixin/
├── accounts.json                     # 已注册账户 id 列表
├── accounts/<id>.json                # token、baseUrl、userId
├── accounts/<id>.sync.json           # getUpdates 游标
└── ref-messages.sqlite               # 引用缓存
```

可用 `OPENCLAW_STATE_DIR` 覆盖根目录。

## 下一步

- [认证（二维码登录）](guides/authentication.md) — 验证码、IDC 重定向、刷新上限。
- [消息收发](guides/messaging.md) — 媒体发送、输入状态、上下文 token、行内引用。
- [媒体（CDN）](guides/media.md) — 上传/下载的 AES 如何工作。
- [错误与重试](reference/errors.md) — 分类失败以决定退避。
