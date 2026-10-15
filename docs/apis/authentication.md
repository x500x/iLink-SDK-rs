# QR login API

QR login lives in [`../../src/messaging/auth.rs`](../../src/messaging/auth.rs). It is
how the SDK obtains a `token` and `base_url` from the Weixin (iLink) backend. The
flow mirrors the TypeScript `QrLogin`: fetch a QR code, poll until scanned, submit
an SMS verification code if prompted, then resolve into a `QrSession`.

The `WeixinClient` exposes a `qr_login()` handle to the same `QrLogin` instance it
used during construction.

## Constants

```rust
pub const DEFAULT_ILINK_BOT_TYPE: &str = "3";
pub const QR_LONG_POLL_TIMEOUT_MS: u64 = 35_000;
pub const MAX_QR_REFRESH_COUNT: usize = 3;
```

## `QrLogin`

```rust
pub struct QrLogin;   // holds an Arc<WeixinApiClient>

impl QrLogin {
    /// Bind to a low-level client (which carries the base_url + optional route_tag).
    pub fn new(api: &WeixinApiClient) -> Self;

    /// Override the bot_type sent in the QR start request (default "3").
    pub fn with_bot_type(mut self, bot_type: impl Into<String>) -> Self;

    // --- flow steps ---
    pub async fn start(&self, state: Option<&str>) -> Result<QrStartResult>;
    pub async fn fetch_qr_code(&self, session_key: &str, timeout_ms: Option<u64>) -> Result<String>;
    pub async fn poll(&self, session_key: &str, timeout_ms: Option<u64>) -> Result<PollOutcome>;
    pub async fn submit_verify_code(&self, session_key: &str, code: &str) -> Result<()>;
    pub async fn remove(&self, session_key: &str) -> Result<()>;

    pub fn has_session(&self, session_key: &str) -> bool;
    pub fn refresh(&self) -> bool;   // rotate session_key; returns false after MAX_QR_REFRESH_COUNT

    // --- convenience ---
    pub async fn wait<F, Fut>(
        &self,
        session_key: &str,
        long_poll_timeout: Duration,
        get_code: F,
        on_refresh_qr: impl Fn(&str) + Send + Sync,   // actually OnRefreshFn below
    ) -> Result<QrWaitResult>
    where F: Fn(()) -> Fut + Send + Sync, Fut: Future<Output = Result<String>> + Send;
}
```

### `QrStartResult`

```rust
pub struct QrStartResult {
    pub session_key: String,
    pub qrcode_url: String,     // where to render/scan the QR image
    pub expires: u64,
    pub bot_id: Option<String>,
    pub user_id: Option<String>,
}
```

### `PollOutcome`

```rust
pub enum PollOutcome {
    Pending,                    // not yet scanned
    WaitingForCode { session_key: String, phone_hint: Option<String> },
    NeedReLogin,                // session expired; start over
    Accepted(QrSession),        // login complete
}
```

`PollOutcome::Accepted` carries the fully-populated `QrSession`.

### `QrSession`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrSession {
    pub bot_id: String,
    pub token: String,
    pub base_url: String,
    pub user_id: String,
    pub session_key: String,
}
```

### `QrWaitResult`

```rust
pub struct QrWaitResult {
    pub session: Option<QrSession>,   // Some on success, None on timeout/cancel
    pub timeout: bool,
    pub cancelled: bool,
}
```

### `local_token_list`

```rust
/// In-memory store of active token sessions (mirrors TS localTokenList);
/// updated on a successful QrLogin::wait.
pub fn local_token_list() -> &'static LocalTokenList;
```

`local_token_list()` is a process-global registry indexed by `user_id`, used by
the CLI to find or reuse an already-authenticated session.

## The login flow step by step

1. **`start`** — request a `session_key` + `qrcode_url` from the backend. Render
   `qrcode_url` to the user (the CLI prints it / serves an image).
2. **`fetch_qr_code`** — fetch the QR image bytes (or the URL) so it can be
   displayed. (Either use `qrcode_url` directly or call this helper.)
3. **`poll`** — long-poll until the backend reports a status:
   - `Pending` → not scanned yet (keep polling)
   - `WaitingForCode` → user scanned but must enter an SMS verification code
   - `NeedReLogin` → session expired; call `start` again
   - `Accepted(QrSession)` → done
4. **`submit_verify_code`** — when `WaitingForCode`, prompt the user for the SMS
   code and submit it, then continue polling.
5. **`wait`** — convenience that drives steps 3–4 automatically: it polls, and when
   `WaitingForCode` occurs it calls `get_code(())` to obtain the SMS code, then
   resubmits and keeps polling until `Accepted` or the timeout/cancel fires.
6. **`remove`** — revoke the session server-side when done.
7. **`has_session` / `refresh`** — local session-key bookkeeping (rotation to
   resist replay; `refresh` returns `false` after `MAX_QR_REFRESH_COUNT`).

## Usage examples

### Minimal: just start + fetch + poll (manual)

```rust,no_run
use ilink_sdk::prelude::*;
use ilink_sdk::messaging::auth::QrLogin;

# async fn example(api: &WeixinApiClient) -> ilink_sdk::Result<()> {
let qr = QrLogin::new(api);
let start = qr.start(None).await?;
println!("Scan this QR: {}", start.qrcode_url);
loop {
    match qr.poll(&start.session_key, None).await? {
        PollOutcome::Pending => continue,
        PollOutcome::WaitingForCode { .. } => { /* prompt SMS code, submit_verify_code */ break; }
        PollOutcome::NeedReLogin => { /* restart */ break; }
        PollOutcome::Accepted(session) => { println!("token={}", session.token); break; }
    }
}
# Ok(())
# }
```

### Convenience: `wait` with a code prompt

```rust,no_run
use ilink_sdk::prelude::*;
use ilink_sdk::messaging::auth::QrLogin;
use std::time::Duration;

# async fn example(api: &WeixinApiClient) -> ilink_sdk::Result<()> {
let qr = QrLogin::new(api);
let start = qr.start(None).await?;
let result = qr.wait(
    &start.session_key,
    Duration::from_secs(480),
    |_| async {
        // In a real app, prompt the user for the SMS code; here we just echo.
        Ok(String::new())
    },
    |_| println!("QR refreshed, show a new code"),
).await?;
if let Some(session) = result.session {
    println!("logged in as {}", session.user_id);
} else if result.timeout {
    println!("login timed out");
}
# Ok(())
# }
```

## Design notes and caveats

1. **`wait` blocks until success or timeout/cancel.** Its `long_poll_timeout` is the
   total budget; pass a `CancellationToken` (via `api`) to abort early. The SMS
   `get_code` closure must be `Send + Sync + 'static`.
2. **`has_session` is local only.** It checks the in-memory map, not the server.
   Use it to decide whether to skip `start`, not to validate liveness.
3. **`refresh` caps at `MAX_QR_REFRESH_COUNT`.** Beyond that it returns `false`; the
   caller should `start` again rather than keep refreshing.
4. **`local_token_list` is process-global.** Multiple `WeixinClient` instances share
   it; do not rely on it for cross-process state — persist via
   [storage.md](storage.md) instead.
5. **`bot_type` is normally "3".** Override only when the backend documents another
   value; the default matches TypeScript behavior.
6. **Errors are surfaced as `Error`.** Network failures classify via
   [error.md](error.md); a `NeedReLogin` outcome is **not** an error — it is a
   normal branch the caller must handle.
