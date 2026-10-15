# Authentication (QR login)

All authenticated calls need a **bot token**, which you obtain by scanning a
QR code with the Weixin app. The flow is implemented in
[`messaging::auth`](../../src/messaging/auth.rs) and exposed through
[`QrLogin`].

## The flow

```text
1. QrLogin::start        POST /ilink/bot/get_bot_qrcode?bot_type=3
2. QrLogin::wait  ─────► GET  /ilink/bot/get_qrcode_status?qrcode=...  (long-poll, repeat)
3.            on "confirmed" → QrSession { token, bot_id, base_url, user_id }
```

- `bot_type` defaults to `"3"` (`DEFAULT_ILINK_BOT_TYPE`).
- Up to ten locally stored tokens are sent in `local_token_list` so an
  already-linked account re-binds instead of creating a duplicate.
- The QR session is kept in a process-global registry keyed by
  `session_key`, with a 5-minute TTL (`ACTIVE_LOGIN_TTL`).

## Starting a session

```rust,no_run
use openclaw_weixin::prelude::*;

# async fn run() -> openclaw_weixin::Result<()> {
let api = WeixinApiClient::new(WeixinApiOptions::new(DEFAULT_BASE_URL))?;
let qr = QrLogin::new(&api);
// or: qr.with_bot_type("3")

let start = qr.start(None).await?;
println!("session_key = {}", start.session_key);
println!("scan: {}", start.qrcode_url);
// start.session_key is what you pass to wait() / poll() / refresh()
# Ok(())
# }
```

If you already have a `session_key` (e.g. from a previous `start` in the same
process), pass it to `start` to re-register the same session.

## Driving the flow to completion

`wait` runs the whole loop: poll, handle verification, refresh on expiry, and
return a [`QrWaitResult`].

```rust,no_run
# async fn run(api: openclaw_weixin::WeixinApiClient) -> openclaw_weixin::Result<()> {
# let qr = openclaw_weixin::messaging::QrLogin::new(&api);
# let start = qr.start(None).await?;
# let session_key = start.session_key;
let result = qr
    .wait(
        &session_key,
        std::time::Duration::from_secs(480),   // overall timeout
        |_retry| async {
            // Called when the server asks for the verification code shown on
            // the phone. Return the code the user typed. An empty string is
            // treated as a wrong code.
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            Ok(line.trim().to_string())
        },
        |_refreshed| async {
            // Called whenever the QR code is replaced (expired/blocked).
            // Re-render start.qrcode_url here.
        },
    )
    .await?;

match result {
    r if r.connected => {
        let session = r.session.expect("connected");
        println!("logged in as {}", session.bot_id);
    }
    r if r.already_connected => {
        println!("already paired to this instance; nothing to persist");
    }
    r => {
        println!("login failed: {}", r.message);
    }
}
# Ok(())
# }
```

### `QrWaitResult` fields

| Field | Meaning |
| --- | --- |
| `connected` | `true` when a fresh `QrSession` was issued. |
| `already_connected` | `true` for `binded_redirect`: already paired, keep existing credentials. |
| `session` | The issued [`QrSession`] when `connected`. |
| `message` | Human-readable failure reason when not connected. |

## Manual polling

If you need to embed the loop in your own event loop, use `poll` directly. It
returns a [`PollOutcome`]:

```rust,no_run
# async fn run(api: openclaw_weixin::WeixinApiClient, session_key: &str) -> openclaw_weixin::Result<()> {
# let qr = openclaw_weixin::messaging::QrLogin::new(&api);
loop {
    match qr.poll(session_key).await? {
        openclaw_weixin::messaging::PollOutcome::Continue => {}
        openclaw_weixin::messaging::PollOutcome::NeedsVerification { retry } => {
            // prompt for code; submit it, then poll again immediately
            qr.submit_verify_code(session_key, "123456");
            continue;
        }
        openclaw_weixin::messaging::PollOutcome::Expired
        | openclaw_weixin::messaging::PollOutcome::VerifyCodeBlocked => {
            let refreshed = qr.refresh(session_key).await?; // re-render refreshed.qrcode_url
            let _ = refreshed;
        }
        openclaw_weixin::messaging::PollOutcome::AlreadyConnected => break,
        openclaw_weixin::messaging::PollOutcome::Done(session) => {
            println!("done: {}", session.bot_id);
            break;
        }
    }
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}
# Ok(())
# }
```

## Statuses

[`QrStatus`] parses the server's `status` string:

| Status | Handling |
| --- | --- |
| `wait` | Keep polling. |
| `scaned` | Waiting for confirmation; clears a pending verify code. |
| `need_verifycode` | Ask the user for the code on the phone. |
| `confirmed` | Issue the session (needs `ilink_bot_id`). |
| `expired` | Refresh the QR code (up to `MAX_QR_REFRESH_COUNT = 3`). |
| `verify_code_blocked` | Refresh / stop after too many wrong attempts. |
| `scaned_but_redirect` | Follow `redirect_host` (IDC migration). |
| `binded_redirect` | Already paired; keep existing credentials. |
| `unknown` | Treated as `wait`. |

## Persisting and pruning

After a successful login, persist the token and prune stale accounts for the
same user so only the latest survives:

```rust,no_run
# fn run(session: &openclaw_weixin::messaging::QrSession) -> openclaw_weixin::Result<()> {
use openclaw_weixin::storage::accounts::{normalize_account_id, save_account, register_account_id,
    clear_stale_accounts_for_user_id, AccountUpdate};

let account_id = normalize_account_id(&session.bot_id);
save_account(&account_id, &AccountUpdate {
    token: Some(session.token.clone()),
    base_url: Some(session.base_url.clone()),
    user_id: Some(session.user_id.clone()),
})?;
register_account_id(&account_id)?;

clear_stale_accounts_for_user_id(&account_id, &session.user_id, |_stale_id| {
    // hook to clear context tokens for the removed account
});
# Ok(())
# }
```

After this, [`WeixinClientBuilder::build`] resolves the account automatically —
no need to keep the `QrSession` around.
