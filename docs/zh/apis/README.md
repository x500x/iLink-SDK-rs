# 分板块 API 手册

本目录把 `ilink_sdk` crate 对外公开的 API 按职责拆成若干板块，逐板块
详细介绍每个 API 的**作用、参数、返回值、错误、用法示例与注意事项**。

与 [`../../reference/api.md`](../../reference/api.md)（一页式签名清单）不同，这里
每个文件都聚焦一个领域，并给出可运行的代码片段与典型组合方式。

> 约定：所有示例都假定已 `use ilink_sdk::prelude::*;`，并运行在
> `tokio` 运行时下。需要真实账号或触网的示例会标注为「示意」。

## 板块索引

| 文档 | 领域 | 覆盖的公开项 |
| --- | --- | --- |
| [错误处理](error.md) | 统一的 `Result` / `Error` / `ErrorKind` | `Error`、`ErrorKind`、分类器、`From` 实现 |
| [底层 API 客户端](api-client.md) | 原始 HTTP 客户端与线类型 | `WeixinApiClient`、`WeixinApiOptions`、`HttpTransport`、`BaseInfo`、`api::types::*`、`session_guard` |
| [高层客户端](client.md) | 开箱即用的机器人客户端 | `WeixinClient`、`WeixinClientBuilder`、`WeixinConfig` |
| [二维码登录](authentication.md) | 扫码登录流程 | `QrLogin`、`QrStartResult`、`QrSession`、`QrWaitResult`、`PollOutcome` |
| [消息收发](messaging.md) | 发送与接收消息 | `MessageSender`、`SendOpts`、`SendResult`、`get_updates` 长轮询、`MsgContext`、`ContextTokenStore`、`Typing`/`TypingHelper` |
| [引用缓存](quote-cache.md) | 本地引用缓存（SQLite） | `QuoteStore`、`QuoteCachePolicy`、`QuoteMessageInput`、`QuoteMessageRecord`、`Direction` |
| [媒体传输（CDN）](cdn.md) | 加密上传/下载 | `aes_ecb`、`cdn_url`、`download`、`upload`、`InboundMedia`、`MediaSink`、`UploadedFile` |
| [存储与配置](storage.md) | 凭据、状态目录、配置 | `accounts`、`state_dir`、`sync_buf`、`config` |
| [MIME 检测](media.md) | 媒体类型探测 | `MediaKind` 及 `mime_*` 函数 |
| [工具函数](util.md) | ID、脱敏、时间 | `random`、`redact`、`time` |
| [日志](logging.md) | `tracing` 接入 | `init_tracing`、`Level`、`ACCOUNT_FIELD` |
| [TLS 配置](tls.md) | 传输层安全 | `default_tls_config`、`DangerIgnoreVerifier` |

## 模块到板块的映射

```text
ilink_sdk/
├── error        → error.md
├── api          → api-client.md
│   ├── client
│   ├── types
│   └── session_guard
├── client       → client.md
├── messaging     → authentication.md / messaging.md / quote-cache.md
│   ├── auth
│   ├── context
│   ├── outbound
│   ├── typing
│   └── quote_cache
├── cdn          → cdn.md
├── storage      → storage.md
├── media        → media.md
├── util         → util.md
├── logging      → logging.md
└── tls          → tls.md
```

## 一个最小的完整流程

下面把多个板块串起来，展示「登录 → 持久化 → 发送 → 接收」的典型顺序
（示意，需要真实账号）：

```rust,no_run
use ilink_sdk::prelude::*;

#[tokio::main]
async fn main() -> ilink_sdk::Result<()> {
    // 1) 登录（详见 authentication.md）
    let api = WeixinApiClient::new(WeixinApiOptions::new(DEFAULT_BASE_URL))?;
    let qr = QrLogin::new(&api);
    let start = qr.start(None).await?;
    println!("扫码: {}", start.qrcode_url);
    let result = qr.wait(&start.session_key, std::time::Duration::from_secs(480),
        |_| async { Ok(String::new()) }, |_| async {}).await?;
    let session = result.session.expect("登录失败");

    // 2) 持久化凭据（详见 storage.md）
    let account_id = storage::accounts::normalize_account_id(&session.bot_id);
    storage::accounts::save_account(&account_id, &storage::accounts::AccountUpdate {
        token: Some(session.token),
        base_url: Some(session.base_url),
        user_id: Some(session.user_id),
    })?;
    storage::accounts::register_account_id(&account_id)?;

    // 3) 用高层客户端发送（详见 client.md / messaging.md）
    let client = WeixinClientBuilder::new().account_id(&account_id).build().await?;
    client.sender().send_text(
        "someone@im.wechat", "你好", &SendOpts::default(),
    ).await?;

    Ok(())
}
```
