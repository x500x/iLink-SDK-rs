# 认证（二维码登录）

所有已认证的调用都需要一个 **bot token**，它通过微信 App 扫码获得。该流程实现于
[`messaging::auth`](../../../src/messaging/auth.rs)，并通过 [`QrLogin`] 暴露。

## 流程

```text
1. QrLogin::start        POST /ilink/bot/get_bot_qrcode?bot_type=3
2. QrLogin::wait  ─────► GET  /ilink/bot/get_qrcode_status?qrcode=...  （长轮询，重复）
3.            on "confirmed" → QrSession { token, bot_id, base_url, user_id }
```

- `bot_type` 默认为 `"3"`（`DEFAULT_ILINK_BOT_TYPE`）。
- 最多会把本地存储的十个 token 放进 `local_token_list`，以便已绑定的账户重新绑定，
  而不是创建重复账户。
- 二维码会话保存在一个进程级注册表中，以 `session_key` 为键，TTL 为 5 分钟
  （`ACTIVE_LOGIN_TTL`）。

## 开始一个会话

```rust,no_run
use ilink_sdk::prelude::*;

# async fn run() -> ilink_sdk::Result<()> {
let api = WeixinApiClient::new(WeixinApiOptions::new(DEFAULT_BASE_URL))?;
let qr = QrLogin::new(&api);
// 或：qr.with_bot_type("3")

let start = qr.start(None).await?;
println!("session_key = {}", start.session_key);
println!("请扫描：{}", start.qrcode_url);
// start.session_key 是要传给 wait() / poll() / refresh() 的值
# Ok(())
# }
```

如果你已经有一个 `session_key`（例如同一进程中来自上一次 `start`），把它传给
`start` 即可重新注册同一会话。

## 驱动流程到完成

`wait` 会运行整个循环：轮询、处理验证码、过期刷新，并返回 [`QrWaitResult`]。

```rust,no_run
# async fn run(api: ilink_sdk::WeixinApiClient) -> ilink_sdk::Result<()> {
# let qr = ilink_sdk::messaging::QrLogin::new(&api);
# let start = qr.start(None).await?;
# let session_key = start.session_key;
let result = qr
    .wait(
        &session_key,
        std::time::Duration::from_secs(480),   // 总超时
        |_retry| async {
            // 当服务端要求手机上显示的验证码时调用。返回用户输入的验证码。
            // 空字符串视为错误验证码。
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            Ok(line.trim().to_string())
        },
        |_refreshed| async {
            // 当二维码被替换（过期/被封）时调用。在这里重新渲染 start.qrcode_url。
        },
    )
    .await?;

match result {
    r if r.connected => {
        let session = r.session.expect("已连接");
        println!("已登录为 {}", session.bot_id);
    }
    r if r.already_connected => {
        println!("已与此实例绑定；无需持久化");
    }
    r => {
        println!("登录失败：{}", r.message);
    }
}
# Ok(())
# }
```

### `QrWaitResult` 字段

| 字段 | 含义 |
| --- | --- |
| `connected` | 当颁发了一个新的 [`QrSession`] 时为 `true`。 |
| `already_connected` | 当为 `binded_redirect` 时为 `true`：已绑定，保留现有凭证。 |
| `session` | `connected` 时颁发的 [`QrSession`]。 |
| `message` | 未连接时的人类可读失败原因。 |

## 手动轮询

如果你需要把循环嵌入自己的事件循环，可直接使用 `poll`。它返回 [`PollOutcome`]：

```rust,no_run
# async fn run(api: ilink_sdk::WeixinApiClient, session_key: &str) -> ilink_sdk::Result<()> {
# let qr = ilink_sdk::messaging::QrLogin::new(&api);
loop {
    match qr.poll(session_key).await? {
        ilink_sdk::messaging::PollOutcome::Continue => {}
        ilink_sdk::messaging::PollOutcome::NeedsVerification { retry } => {
            // 提示输入验证码；提交后立刻再次轮询
            qr.submit_verify_code(session_key, "123456");
            continue;
        }
        ilink_sdk::messaging::PollOutcome::Expired
        | ilink_sdk::messaging::PollOutcome::VerifyCodeBlocked => {
            let refreshed = qr.refresh(session_key).await?; // 重新渲染 refreshed.qrcode_url
            let _ = refreshed;
        }
        ilink_sdk::messaging::PollOutcome::AlreadyConnected => break,
        ilink_sdk::messaging::PollOutcome::Done(session) => {
            println!("完成：{}", session.bot_id);
            break;
        }
    }
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}
# Ok(())
# }
```

## 状态码

[`QrStatus`] 解析服务端返回的 `status` 字符串：

| 状态 | 处理 |
| --- | --- |
| `wait` | 继续轮询。 |
| `scaned` | 等待确认；清除待处理的验证码。 |
| `need_verifycode` | 向用户索取手机上的验证码。 |
| `confirmed` | 颁发会话（需要 `ilink_bot_id`）。 |
| `expired` | 刷新二维码（最多 `MAX_QR_REFRESH_COUNT = 3` 次）。 |
| `verify_code_blocked` | 错误尝试过多后刷新 / 停止。 |
| `scaned_but_redirect` | 跟随 `redirect_host`（IDC 迁移）。 |
| `binded_redirect` | 已绑定；保留现有凭证。 |
| `unknown` | 视为 `wait`。 |

## 持久化与裁剪

登录成功后，持久化 token 并裁剪同一用户下的陈旧账户，使得只保留最新的那个：

```rust,no_run
# fn run(session: &ilink_sdk::messaging::QrSession) -> ilink_sdk::Result<()> {
use ilink_sdk::storage::accounts::{normalize_account_id, save_account, register_account_id,
    clear_stale_accounts_for_user_id, AccountUpdate};

let account_id = normalize_account_id(&session.bot_id);
save_account(&account_id, &AccountUpdate {
    token: Some(session.token.clone()),
    base_url: Some(session.base_url.clone()),
    user_id: Some(session.user_id.clone()),
})?;
register_account_id(&account_id)?;

clear_stale_accounts_for_user_id(&account_id, &session.user_id, |_stale_id| {
    // 用于清理被移除账户的上下文 token 的钩子
});
# Ok(())
# }
```

完成之后，[`WeixinClientBuilder::build`] 会自动解析该账户——无需再保留
`QrSession`。
