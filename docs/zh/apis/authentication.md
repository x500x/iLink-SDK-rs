# 二维码登录 API

二维码登录位于 [`../../src/messaging/auth.rs`](../../src/messaging/auth.rs)，是 SDK 拿到
`QrSession`（含 bot token）的入口。流程对应 TypeScript 的
`src/auth/login-qr.ts`：

1. `POST /ilink/bot/get_bot_qrcode?bot_type=3` —— 返回 `qrcode` 值与待渲染的 URL；
   最多附带本地保存的 10 个 bot token。
2. `GET /ilink/bot/get_qrcode_status?qrcode=...` —— 长轮询状态；服务端要求验证时
   追加 `&verify_code=...`。
3. 状态为 `confirmed` 时，响应携带新 bot token、bot id、API base URL 以及配对的
   微信 user id。

支持的状态：`wait`、`scaned`、`confirmed`、`expired`、`need_verifycode`、
`verify_code_blocked`、`scaned_but_redirect`、`binded_redirect`。

## `QrLogin`

绑定到某个 [`WeixinApiClient`](../apis/api-client.md) 的登录构造器。

```rust
pub struct QrLogin<'a> { /* client, bot_type */ }
```

### 构造与配置

```rust
impl<'a> QrLogin<'a> {
    /// 基于现有客户端创建登录构造器。
    pub fn new(client: &'a WeixinApiClient) -> Self;

    /// 覆盖 `bot_type` 查询参数（默认 "3"）。
    pub fn with_bot_type(mut self, bot_type: impl Into<String>) -> Self;
}
```

常量：

```rust
pub const DEFAULT_ILINK_BOT_TYPE: &str = "3";
pub const FIXED_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
pub const ACTIVE_LOGIN_TTL: Duration = Duration::from_secs(5 * 60);
pub const QR_LONG_POLL_TIMEOUT_MS: u64 = 35_000;
pub const MAX_QR_REFRESH_COUNT: usize = 3;
pub const QR_POLL_INTERVAL: Duration = Duration::from_secs(1);
```

`FIXED_BASE_URL` 是所有二维码请求使用的固定 API base URL（独立于账户的
`base_url`）。

### 启动与获取二维码

```rust
impl<'a> QrLogin<'a> {
    /// 请求新二维码并登记会话。
    /// 返回待展示的 URL + 传入 wait() 的 session_key。
    pub async fn start(&self, session_key: Option<&str>) -> Result<QrStartResult>;

    /// 从固定 base URL 拉取二维码（内部使用，也可单独调用）。
    pub async fn fetch_qr_code(&self) -> Result<QrCodeResponse>;
}
```

`start` 会先清理过期会话，再生成一个 `session_key`（默认 `qr-<随机u32>`，也可由
调用方指定）。返回的 `QrStartResult`：

```rust
pub struct QrStartResult {
    pub qrcode: String,        // 不透明的 qrcode 值，仅渲染用无需关心
    pub qrcode_url: String,    // 要渲染成二维码的 URL/内容
    pub session_key: String,   // 传入 wait() / poll() 的 key
}
```

### 轮询单步

```rust
impl<'a> QrLogin<'a> {
    /// 轮询一次并推进状态机，不阻塞整个流程。
    pub async fn poll(&self, session_key: &str) -> Result<PollOutcome>;
}
```

`poll` 返回 [`PollOutcome`](#polloutcome)，调用方据此决定继续、刷新还是结束。

### 验证与状态管理

```rust
impl<'a> QrLogin<'a> {
    /// 提交手机上显示的验证码（供下一次 poll 使用）。
    pub fn submit_verify_code(&self, session_key: &str, code: &str);

    /// 忘记一个会话。
    pub fn remove(&self, session_key: &str);

    /// 判断是否存在未过期的会话。
    pub fn has_session(session_key: &str) -> bool;
}
```

### 等待完成（一体化）

```rust
impl<'a> QrLogin<'a> {
    pub async fn wait<F, Fut, G, GFut>(
        &self,
        session_key: &str,
        timeout: Duration,
        on_verification_required: F,   // fn(bool) -> Fut<Output = Result<String>>
        on_refresh: G,                 // fn(&QrStartResult) -> GFut<Output = ()>
    ) -> Result<QrWaitResult>;
}
```

- `on_verification_required`：服务端要求验证码时被调用，`retry: true` 表示上一次
  提交的码被拒绝；返回用户键入的验证码。返回错误会中止登录；空字符串等价于错误码。
- `on_refresh`：二维码被替换时（过期或验证次数过多）触发，便于宿主重新渲染。

### 刷新二维码

```rust
impl<'a> QrLogin<'a> {
    /// 用新的二维码替换活动会话中的旧码。
    pub async fn refresh(&self, session_key: &str) -> Result<QrStartResult>;
}
```

## `PollOutcome`

```rust
pub enum PollOutcome {
    Continue,                              // 继续轮询
    NeedsVerification { retry: bool },     // 需要验证码
    Expired,                              // 二维码过期，需刷新
    VerifyCodeBlocked,                    // 验证次数过多，需刷新或停止
    AlreadyConnected,                     // 已绑定本实例，无需新凭据
    Done(QrSession),                      // 登录成功
}
```

## 关键类型

### `QrSession`

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QrSession {
    pub token: String,       // 已认证 API 请求用的 bot token
    pub bot_id: String,      // bot 账户 id
    pub base_url: String,    // 后端为该账户分配的 API base URL
    pub user_id: String,     // 扫码的微信用户 id，建议加入白名单
}
```

### `QrWaitResult`

```rust
pub struct QrWaitResult {
    pub connected: bool,         // 本次是否为本实例建立连接
    pub already_connected: bool, // binded_redirect：已配对，无需持久化
    pub session: Option<QrSession>,
    pub message: String,         // connected==false 时的失败说明
}
```

### `QrStatus`

```rust
pub enum QrStatus { Wait, Scaned, Confirmed, Expired, ScanButRedirect, NeedVerifyCode, VerifyCodeBlocked, BindedRedirect, Unknown }
impl QrStatus { pub fn parse(raw: &str) -> Self; }
```

### `local_token_list`

```rust
pub fn local_token_list() -> Vec<String>;
```

读取本地保存的最多 10 个 bot token（倒序），在 `get_bot_qrcode` 时一并发给后端。

## 用法示例

### 完整等待流程（带控制台提示）

```rust,no_run
use ilink_sdk::prelude::*;
use std::io::Read;
use std::time::Duration;

# async fn example() -> ilink_sdk::Result<()> {
let api = WeixinApiClient::new(WeixinApiOptions::new(DEFAULT_BASE_URL))?;
let qr = QrLogin::new(&api);
let start = qr.start(None).await?;
println!("扫码登录: {}", start.qrcode_url);

let result = qr.wait(
    &start.session_key,
    Duration::from_secs(480),
    |retry| async move {
        if retry { println!("验证码错误，请重新输入"); }
        print!("请输入手机上显示的验证码: ");
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        Ok(line.trim().to_string())
    },
    |new_start| async move {
        println!("二维码已刷新，请重新扫描: {}", new_start.qrcode_url);
    },
).await?;

match result.session {
    Some(session) => {
        println!("登录成功，bot_id={}, user_id={}", session.bot_id, session.user_id);
        // 接下来：save_account + register_account_id（见 storage.md）
    }
    None => println!("登录失败: {}", result.message),
}
# Ok(())
# }
```

### 手动驱动状态机（更细控制）

```rust,no_run
use ilink_sdk::messaging::auth::PollOutcome;

# async fn example(qr: &ilink_sdk::messaging::auth::QrLogin<'_>, key: &str)
# -> ilink_sdk::Result<()> {
loop {
    match qr.poll(key).await? {
        PollOutcome::Continue => {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        PollOutcome::NeedsVerification { retry } => {
            let code = if retry { "上次错误" } else { "首次" };
            println!("需要验证码 ({code})");
            // 这里从 UI 获取 code 并 qr.submit_verify_code(key, &code);
        }
        PollOutcome::Expired | PollOutcome::VerifyCodeBlocked => {
            let _ = qr.refresh(key).await?;
        }
        PollOutcome::AlreadyConnected => { println!("已连接"); break; }
        PollOutcome::Done(session) => { println!("done: {}", session.bot_id); break; }
    }
}
# Ok(())
# }
```

## 设计要点与注意事项

1. **会话有客户端 TTL（`ACTIVE_LOGIN_TTL`，5 分钟）。** `start` 调用前会清理过期
   会话；超过 TTL 的 `session_key` 在 `poll`/`wait` 时返回错误。
2. **刷新次数上限（`MAX_QR_REFRESH_COUNT=3`）。** 超过后 `wait` 返回
   `failed`，消息为「二维码过期太多次」或「验证码错误太多次」。
3. **`binded_redirect` 不会返回新凭据。** 它表示本实例已经配对，现存本地凭据仍然
   有效，`already_connected=true`。不要覆盖已有账户。
4. **IDC 重定向（`scaned_but_redirect`）。** 服务端返回 `redirect_host` 时，后续
   轮询会切换到 `https://<host>`，无需调用方处理。
5. **`verify_code` 空串 = 错误码。** 在 `on_verification_required` 中返回空串会令
   流程按「拒绝」处理，触发 `retry=true`。
6. **登录后务必持久化。** 拿到 `QrSession` 后，应通过 `storage::accounts::save_account`
   落盘并 `register_account_id`（详见 [storage.md](storage.md)），否则下次启动仍需
   重新扫码。`WeixinClient::save_and_load` 是一步到位的方式。
