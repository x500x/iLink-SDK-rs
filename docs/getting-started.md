# Getting started

This guide takes you from an empty project to a running Weixin bot that can log
in with a QR code and send messages.

## 1. Add the dependency

```toml
[dependencies]
ilink-sdk = "2.4"
tokio = { version = "1", features = ["full"] }
```

> Tokio is required. If you only need the library (not the bundled `weixin`
> binary), turn off the default `cli` feature:
>
> ```toml
> ilink-sdk = { version = "2.4", default-features = false }
> ```

## 2. QR-code login

`QrLogin::start` returns a URL that you render (or print) for the user to scan.
`QrLogin::wait` drives the polling loop to completion. On success you get a
[`QrSession`] holding the bot `token`, `bot_id`, base URL and the paired
`user_id`.

```rust,no_run
use ilink_sdk::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = WeixinApiClient::new(WeixinApiOptions::new(DEFAULT_BASE_URL))?;
    let qr = QrLogin::new(&api);
    let start = qr.start(None).await?;
    println!("Scan this: {}", start.qrcode_url);

    // Wait up to 8 minutes for the user to scan and confirm on their phone.
    // The closures read a verification code from stdin when the server asks
    // for one, and re-render when the QR code is refreshed.
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

    let session = result.session.expect("login must succeed");
    let account_id = ilink_sdk::storage::accounts::normalize_account_id(&session.bot_id);

    // Persist the token so later runs can build a client directly.
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

`start.qrcode_url` is the URL the user opens on their phone; you can also render
it as a QR image (for example with the `qrcode` crate) or, as the CLI does,
print it to the terminal.

## 3. Build a client and send a message

Once the credentials are persisted you rarely touch QR login again. Build a
[`WeixinClient`] from the stored account:

```rust,no_run
use ilink_sdk::prelude::*;

# async fn run() -> ilink_sdk::Result<()> {
let client = WeixinClientBuilder::new()
    .account_id("deadbeef-im-bot")          // optional; defaults to the only registered account
    .build()
    .await?;

client
    .sender()
    .send_text(
        "someone@im.wechat",
        "hello from Rust",
        &ilink_sdk::messaging::SendOpts::default(),
    )
    .await?;
# Ok(())
# }
```

For a one-shot program you can also skip account storage and build the client
straight from a [`QrSession`]:

```rust,no_run
# async fn run(session: ilink_sdk::messaging::QrSession) -> ilink_sdk::Result<()> {
let client = WeixinClient::from_session(session, None).await?;
# Ok(())
# }
```

## 4. Receive messages (long-poll)

```rust,no_run
# async fn run(client: ilink_sdk::WeixinClient) -> ilink_sdk::Result<()> {
use ilink_sdk::api::types::{GetUpdatesReq, MessageItemType};
use ilink_sdk::storage::sync_buf::{load_for_account, save_for_account};

let account_id = client.account_id().to_string();
let mut cursor = load_for_account(&account_id); // resume across restarts

loop {
    let resp = client.api().get_updates(
        &GetUpdatesReq { get_updates_buf: cursor.clone(), sync_buf: None },
        None,    // use the default 35s long-poll timeout
        None,    // no external cancellation token
    ).await?;

    if let Some(buf) = &resp.get_updates_buf {
        cursor = Some(buf.clone());
        let _ = save_for_account(&account_id, buf);
    }

    for msg in resp.msgs.unwrap_or_default() {
        for item in msg.item_list.unwrap_or_default() {
            if item.r#type == Some(MessageItemType::Text.value()) {
                if let Some(t) = item.text_item.and_then(|i| i.text) {
                    println!("from {}: {}", msg.from_user_id.unwrap_or_default(), t);
                }
            }
        }
    }
}
# }
```

> When the long-poll times out or is cancelled, `get_updates` returns an **empty
> response with `ret: 0` and the same cursor**. That is by design — just loop.
> Real transport failures propagate as an [`Error`].

## 5. Where state lives

All credentials and caches live under the SDK state directory (see
[Storage & state](guides/storage.md)). The default location:

```text
$HOME/.openclaw/openclaw-weixin/
├── accounts.json                     # registered account ids
├── accounts/<id>.json                # token, baseUrl, userId
├── accounts/<id>.sync.json           # getUpdates cursor
└── ref-messages.sqlite               # quote cache
```

Override the root with `OPENCLAW_STATE_DIR`.

## Next steps

- [Authentication (QR login)](guides/authentication.md) — verification codes, IDC redirects, refresh limits.
- [Messaging](guides/messaging.md) — media, typing indicators, context tokens, inline quotes.
- [Media (CDN)](guides/media.md) — how upload/download AES works.
- [Errors & retries](reference/errors.md) — classify failures to decide backoff.
