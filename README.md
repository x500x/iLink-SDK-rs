# openclaw-weixin (Rust SDK)

[![License: MIT](https://img.shields.io/badge/license-MIT-green)](./LICENSE)

A Rust SDK for the Tencent **Weixin (iLink)** backend protocol.

This crate is a faithful, idiomatic port of the TypeScript
[`openclaw-weixin`](https://github.com/Tencent/openclaw-weixin) channel
plugin, with the OpenClaw gateway integration removed. What is left is a plain
library you can embed in any Rust application:

- QR-code login with local credential storage
- Long-poll message delivery (`getUpdates`)
- Text, image, video, file and voice messages
- CDN media upload / download with AES-128-ECB decryption
- Typing indicators
- A local quote cache for ID-only inbound quotes

**No embedded HTTP server.** Everything is a function call into the crate;
there is no daemon, no webhook endpoint, and no IPC to stand up.

| | Requirement |
| --- | --- |
| Rust | `1.82` or newer (edition 2021) |
| Tokio | required (async runtime) |
| Weixin backend | the same iLink endpoints the TypeScript plugin uses |

## Quick start

```toml
[dependencies]
openclaw-weixin = "2.4"
tokio = { version = "1", features = ["full"] }
```

```rust,no_run
use openclaw_weixin::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Start a QR login and show the URL.
    let api = WeixinApiClient::new(WeixinApiOptions::new(DEFAULT_BASE_URL))?;
    let qr = openclaw_weixin::messaging::QrLogin::new(&api);
    let start = qr.start(None).await?;
    println!("Scan this: {}", start.qrcode_url);

    // 2. Wait for the user to confirm on their phone.
    let result = qr
        .wait(&start.session_key, std::time::Duration::from_secs(480), |_retry| async {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            Ok(line.trim().to_string())
        }, |_| async {})
        .await?;

    let session = result.session.expect("login must succeed");
    let account_id = openclaw_weixin::storage::accounts::normalize_account_id(&session.bot_id);

    openclaw_weixin::storage::accounts::save_account(
        &account_id,
        &openclaw_weixin::storage::accounts::AccountUpdate {
            token: Some(session.token),
            base_url: Some(session.base_url),
            user_id: Some(session.user_id),
        },
    )?;
    openclaw_weixin::storage::accounts::register_account_id(&account_id)?;

    // 3. Send a message back.
    let client = WeixinClientBuilder::new().account_id(&account_id).build().await?;
    client
        .sender()
        .send_text(
            "someone@im.wechat",
            "hello from Rust",
            &openclaw_weixin::messaging::SendOpts::default(),
        )
        .await?;

    Ok(())
}
```

## Design notes

### Faithful to the wire protocol

The request and response shapes, headers, field semantics, timeouts and
retry policies mirror the TypeScript implementation. Where the two differ, it
is deliberate and covered by a test:

| Area | TypeScript | Rust |
| --- | --- | --- |
| uint64 message ids | rewritten to strings before `JSON.parse` | quoted before `serde_json` parses |
| `getUpdates` timeout | returns `ret: 0` + the same cursor | identical |
| `sendMessage` `ret` | error only when non-zero | identical |
| AES-128-ECB | `node:crypto` | per-block ECB (see below) |
| Quote cache | `node:sqlite` | `rusqlite` (bundled SQLite) |

**ECB is implemented block-by-block on purpose.** `aes` has no standalone ECB
mode, and CBC with a zero IV is *not* equivalent: it XORs each plaintext block
against the previous ciphertext block, so repeated plaintext blocks produce
different ciphertext. The Weixin backend expects true ECB, so
[`encrypt_aes_ecb`](src/cdn/aes_ecb.rs) transforms each 16-byte block
independently. A test asserts byte-for-byte equality with Node's
`aes-128-ecb` output.

### Storage locations

All state lives under the same directory the TypeScript plugin uses, so a
Rust and a TypeScript client can share credentials on one machine:

```text
$OPENCLAW_STATE_DIR/openclaw-weixin/
├── accounts.json                     # registered account ids
├── accounts/
│   ├── <id>.json                     # token, baseUrl, userId
│   ├── <id>.sync.json                # getUpdates cursor
│   └── <id>.context-tokens.json      # reply context tokens
├── ref-messages.sqlite               # quote cache
└── debug-mode.json                   # per-account debug flag
```

Override the root with `OPENCLAW_STATE_DIR` (or `CLAWDBOT_STATE_DIR`).
Config is read from `$OPENCLAW_CONFIG`, else `<state_dir>/openclaw.json`,
section `channels.openclaw-weixin`.

### Error handling

Every fallible call returns [`Result<T>`](src/error.rs). Transport failures
carry an [`ErrorKind`] so callers can branch on the cause instead of string
matching:

```rust
match client.sender().send_text(to, text, &opts).await {
    Ok(result) => println!("sent {}", result.message_id),
    Err(err) => match err.kind() {
        ErrorKind::Timeout | ErrorKind::Cancelled => { /* retry or stop cleanly */ }
        ErrorKind::Dns | ErrorKind::Tcp => { /* back off and retry */ }
        _ => { /* log and surface to the user */ }
    },
}
```

`err.is_abort()` is the check the monitor loop uses: it is true for a
client-side timeout *or* an external cancellation, matching the TypeScript
`AbortError` test.

## CLI

A small binary wraps the library so behaviour can be exercised end to end
before writing code:

```bash
cargo run -- login                 # QR login, saves credentials
cargo run -- accounts              # list registered accounts
cargo run -- status                # resolved account settings
cargo run -- send <@im.wechat> "hi" --context-token <token>
cargo run -- send-file <@im.wechat> ./photo.png --caption "look"
cargo run -- listen                # long-poll and print inbound messages
cargo run -- logout <account-id>
```

## Configuration

Optional settings in `channels.openclaw-weixin` of `openclaw.json`:

| Key | Type | Default | Purpose |
| --- | --- | --- | --- |
| `botAgent` | string | `OpenClaw` | Observability identifier sent as `base_info.bot_agent`. Not an authentication credential. |
| `routeTag` | string \| number | — | Forwarded as the `SKRouteTag` header. |
| `accounts.<id>.baseUrl` | string | `https://ilinkai.weixin.qq.com` | Per-account API base URL. |
| `accounts.<id>.cdnBaseUrl` | string | `https://novac2c.cdn.weixin.qq.com/c2c` | Per-account CDN base URL. |
| `accounts.<id>.enabled` | boolean | `true` | Disable an account without deleting it. |
| `quoteCache` | object | see below | Local quote cache policy. |

`botAgent` accepts UA-style `Name/Version` tokens with optional
`(comment)`. Invalid tokens are dropped; an empty result falls back to
`OpenClaw`. Values longer than 256 bytes are truncated by dropping trailing
tokens.

Quote-cache keys: `enabled`, `retentionDays` (30),
`maxMessagesPerAccount` (10000), `mediaRetentionDays` (7),
`maxMediaBytesPerAccount` (256 MiB), `maxSingleMediaBytes` (25 MiB). Invalid
values fall back to their defaults.

## Feature flags

| Feature | Default | What it enables |
| --- | --- | --- |
| `cli` | on | the `weixin` binary (`clap`, `qrcode`, `directories`, `tracing-subscriber`) |

```bash
cargo build --no-default-features   # library only, no CLI
```

## Testing

```bash
cargo test                 # unit + integration
cargo test --no-default-features   # library only
```

- Unit tests cover the crypto (including a byte-for-byte Node comparison),
  `<SECRET_2885aeee>`, markdown
  survival (whose behaviour is defined by the TypeScript test suite rather
  than guessed), and the quote-cache GC.
- Integration tests in [`tests/api_integration.rs`](tests/api_integration.rs)
  run the real HTTP path against a `wiremock` server and assert on headers,
  request bodies, timeout handling and cancellation.

## Documentation

| Need | Start here |
| --- | --- |
| Wire protocol | the upstream [`docs/protocol.md`](https://github.com/Tencent/openclaw-weixin/blob/main/docs/protocol.md) |
| Module map | [`src/lib.rs`](src/lib.rs) |
| Errors | [`src/error.rs`](src/error.rs) |
| Media | [`src/cdn`](src/cdn) |

## Relationship to the TypeScript plugin

This is an independent implementation, not a binding. The TypeScript plugin
implements the OpenClaw channel contract (gateway lifecycle, reply
dispatching, hook integration, pairing); this SDK intentionally omits all of
that so it has no dependency on a gateway. If you need a full channel plugin,
use the TypeScript project. If you want to talk to the Weixin backend from
Rust — a service, a CLI, a bot, a test harness — use this.

## License

MIT, matching the upstream
[openclaw-weixin](https://github.com/Tencent/openclaw-weixin) project.
See [LICENSE](./LICENSE) for the full text.

Copyright (C) 2026 Tencent. All rights reserved.
