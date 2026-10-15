# API by board

This directory splits the `openclaw_weixin` crate's public API into focused
boards, each explaining in depth the **purpose, parameters, return values,
errors, usage examples, and caveats** of every API in that area.

Unlike [`../reference/api.md`](../reference/api.md) (a one-page signature
catalog), each file here focuses on one domain and gives runnable code snippets
and typical composition patterns.

> Convention: all examples assume `use openclaw_weixin::prelude::*;` and run under
> a `tokio` runtime. Examples that need a real account or the network are marked
> "illustrative".

## Board index

| Doc | Domain | Public items covered |
| --- | --- | --- |
| [Error handling](error.md) | Unified `Result` / `Error` / `ErrorKind` | `Error`, `ErrorKind`, classifiers, `From` impls |
| [Low-level API client](api-client.md) | Raw HTTP client and wire types | `WeixinApiClient`, `WeixinApiOptions`, `HttpTransport`, `BaseInfo`, `api::types::*`, `session_guard` |
| [High-level client](client.md) | Batteries-included bot client | `WeixinClient`, `WeixinClientBuilder`, `WeixinConfig` |
| [QR login](authentication.md) | Scan-to-login flow | `QrLogin`, `QrStartResult`, `QrSession`, `QrWaitResult`, `PollOutcome` |
| [Messaging](messaging.md) | Send and receive | `MessageSender`, `SendOpts`, `SendResult`, `get_updates` long-poll, `MsgContext`, `ContextTokenStore`, `Typing`/`TypingHelper` |
| [Quote cache](quote-cache.md) | Local quote cache (SQLite) | `QuoteStore`, `QuoteCachePolicy`, `QuoteMessageInput`, `QuoteMessageRecord`, `Direction` |
| [Media transport (CDN)](cdn.md) | Encrypted upload/download | `aes_ecb`, `cdn_url`, `download`, `upload`, `InboundMedia`, `MediaSink`, `UploadedFile` |
| [Storage & config](storage.md) | Credentials, state dir, config | `accounts`, `state_dir`, `sync_buf`, `config` |
| [MIME detection](media.md) | Media type sniffing | `MediaKind` and `mime_*` functions |
| [Utilities](util.md) | IDs, redaction, time | `random`, `redact`, `time` |
| [Logging](logging.md) | `tracing` integration | `init_tracing`, `Level`, `ACCOUNT_FIELD` |
| [TLS config](tls.md) | Transport security | `default_tls_config`, `DangerIgnoreVerifier` |

## Module to board mapping

```text
openclaw_weixin/
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

## Minimal end-to-end flow

The following stitches several boards together to show the typical
"login → persist → send → receive" order (illustrative; needs a real account):

```rust,no_run
use openclaw_weixin::prelude::*;

#[tokio::main]
async fn main() -> openclaw_weixin::Result<()> {
    // 1) Log in (see authentication.md)
    let api = WeixinApiClient::new(WeixinApiOptions::new(DEFAULT_BASE_URL))?;
    let qr = QrLogin::new(&api);
    let start = qr.start(None).await?;
    println!("Scan: {}", start.qrcode_url);
    let result = qr.wait(&start.session_key, std::time::Duration::from_secs(480),
        |_| async { Ok(String::new()) }, |_| async {}).await?;
    let session = result.session.expect("login failed");

    // 2) Persist credentials (see storage.md)
    let account_id = storage::accounts::normalize_account_id(&session.bot_id);
    storage::accounts::save_account(&account_id, &storage::accounts::AccountUpdate {
        token: Some(session.token),
        base_url: Some(session.base_url),
        user_id: Some(session.user_id),
    })?;
    storage::accounts::register_account_id(&account_id)?;

    // 3) Send via the high-level client (see client.md / messaging.md)
    let client = WeixinClientBuilder::new().account_id(&account_id).build().await?;
    client.sender().send_text(
        "someone@im.wechat", "hello", &SendOpts::default(),
    ).await?;

    Ok(())
}
```
