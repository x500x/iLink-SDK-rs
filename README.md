# iLink SDK (ilink-sdk)

[![License: MIT](https://img.shields.io/badge/license-MIT-green)](./LICENSE)
[![CI](https://github.com/x500x/iLink-SDK-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/x500x/iLink-SDK-rs/actions/workflows/ci.yml)

> 中文版: [README.zh.md](README.zh.md)

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
ilink-sdk = "2.4"
tokio = { version = "1", features = ["full"] }
```

```rust,no_run
use ilink_sdk::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Start a QR login and show the URL.
    let api = WeixinApiClient::new(WeixinApiOptions::new(DEFAULT_BASE_URL))?;
    let qr = ilink_sdk::messaging::QrLogin::new(&api);
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
    let account_id = ilink_sdk::storage::accounts::normalize_account_id(&session.bot_id);

    ilink_sdk::storage::accounts::save_account(
        &account_id,
        &ilink_sdk::storage::accounts::AccountUpdate {
            token: Some(session.token),
            base_url: Some(session.base_url),
            user_id: Some(session.user_id),
        },
    )?;
    ilink_sdk::storage::accounts::register_account_id(&account_id)?;

    // 3. Send a message back.
    let client = WeixinClientBuilder::new().account_id(&account_id).build().await?;
    client
        .sender()
        .send_text(
            "someone@im.wechat",
            "hello from Rust",
            &ilink_sdk::messaging::SendOpts::default(),
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

The `cli` feature (on by default) builds a `weixin` binary that wraps the
whole library so behaviour can be exercised — and scripted — end to end before
writing code:

```bash
cargo run -- login                 # QR login, saves credentials
cargo run -- accounts              # list registered accounts
cargo run -- status                # resolved account settings
cargo run -- send <@im.wechat> "hi" --context-token <token>
cargo run -- send-file <@im.wechat> ./photo.png --caption "look"
cargo run -- send-item <@im.wechat> '{"type":1,"text_item":{"text":"hi"}}'
cargo run -- listen                # long-poll, save media, print inbound messages
cargo run -- typing <@im.wechat> on
cargo run -- notify start
cargo run -- upload <@im.wechat> ./photo.png --media-type image
cargo run -- download --param <p> --aes-key <k> -o ./out.jpg
cargo run -- quote policy && cargo run -- quote gc
cargo run -- context set <@im.wechat> <token>
cargo run -- sync show
cargo run -- session status
cargo run -- mime ./photo.png
cargo run -- logout <account-id>
```

Every command accepts `--json` for machine-readable output and the global
routing flags `--account`, `--base-url`, `--cdn-base-url`, `--route-tag`,
`--bot-agent`, `--timeout-ms`. See [docs/reference/cli.md](docs/reference/cli.md)
for the complete command reference.

## Shared library (dll / so / dylib)

The crate is built as `rlib` **and** `cdylib` **and** `staticlib`, so
`cargo build --release` produces a native shared/static library next to the
Rust rlib:

| Platform | Output |
| --- | --- |
| Windows | `ilink_sdk.dll`, `ilink_sdk.dll.lib` (import), `ilink_sdk.lib` (static) |
| Linux | `libilink_sdk.so`, `libilink_sdk.a` |
| macOS | `libilink_sdk.dylib`, `libilink_sdk.a` |

The whole SDK is reachable from C/C++/Go/… through one JSON-in / JSON-out call
declared in [`include/ilink_sdk.h`](include/ilink_sdk.h):

```c
char *out = ilink_call("send.text",
                       "{\"to\":\"someone@im.wechat\",\"text\":\"hi\"}");
puts(out);                 /* {"ok":true,"result":{...}} */
ilink_string_free(out);
```

A runnable example lives in
[`examples/c/ilink_demo.c`](examples/c/ilink_demo.c); the operation catalogue is
in [docs/reference/ffi.md](docs/reference/ffi.md).

**Link the shared library by default.** Windows binaries are built with
`+crt-static`, so they do not require the Visual C++ Redistributable. The
Windows *static* library is MSVC-only and needs `-DILINK_SDK_STATIC` when
included; from MinGW or Go's cgo use the DLL instead. See
[docs/reference/ffi.md](docs/reference/ffi.md#linking) for the exact flags.

CI builds and tests on Linux, macOS and Windows (x86_64 and aarch64), and
publishes the libraries as
release artifacts (see [`.github/workflows/ci.yml`](.github/workflows/ci.yml)
and [`.github/workflows/release.yml`](.github/workflows/release.yml)). On macOS
the dylib's install name is rewritten to `@rpath/libilink_sdk.dylib` so it can
be bundled into an app bundle.

## Language bindings

Besides Rust and C, thin bindings are provided for Go, Python, C++, Java and
Kotlin. They are all wrappers over the same JSON C ABI, so nothing is
reimplemented per language. Each binding exposes a universal `call(op, params)`
for every operation **and** typed models for the common flows — see
[`bindings/`](bindings/README.md).

Python, Go, Java and Kotlin packages bundle the native library, so installing the
binding is all that is needed (no Rust toolchain, no manual library setup):

```bash
pip install ilink-sdk                    # Python
go get github.com/x500x/iLink-SDK-rs/bindings/go/ilink   # Go (pure Go, no cgo)
```

| Language | Directory | Install | Notes |
| --- | --- | --- | --- |
| C | [`include/ilink_sdk.h`](include/ilink_sdk.h) | link the library | header + [`examples/c`](examples/c) |
| C++ | [`bindings/cpp`](bindings/cpp) | `find_package(ilink-sdk)` | header-only, RAII, typed models |
| Rust | crate root | `cargo add ilink-sdk` | native |
| Python | [`bindings/python`](bindings/python) | `pip install ilink-sdk` | `ctypes`, no dependencies, dataclasses |
| Go | [`bindings/go`](bindings/go) | `go get .../bindings/go/ilink` | pure Go (purego), typed structs |
| Java | [`bindings/java`](bindings/java) | Maven `io.ilink:ilink-sdk` | JNA, typed models |
| Kotlin | [`bindings/kotlin`](bindings/kotlin) | Gradle `io.ilink:ilink-sdk` | JNA, typed data classes |

```python
# Python
import ilink_sdk
ilink_sdk.call("mime.info", {"path": "photo.png"})
ilink_sdk.Client().mime_info("photo.png").mime          # typed helper
```

```go
// Go
ilink.Call("mime.info", map[string]any{"path": "photo.png"})
info, _ := ilink.NewClient(ilink.Config{}).MimeInfo("photo.png")  // typed helper
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

The C ABI in `src/ffi.rs` is always compiled; it has no extra dependencies.

```bash
cargo build --no-default-features   # library + C ABI, no CLI
```

## Release profile

`[profile.release]` is tuned for small, self-contained binaries:

- `lto = true`, `codegen-units = 1`, `strip = true`;
- `opt-level = "s"` — the hot paths are network/TLS bound, and `ring`'s
  assembly is unaffected, so this costs little CPU while shrinking the
  artifacts by ~30% (a 1 MiB AES-ECB round trip is ~20% slower, which is
  ~12 ms for a 25 MiB media file). Set `opt-level = 3` if you prefer maximum
  throughput over size;
- on Windows, `+crt-static` (see [`.cargo/config.toml`](.cargo/config.toml))
  removes the Visual C++ Redistributable dependency.

HTTP/2 is disabled (the client configures no ALPN and negotiates HTTP/1.1),
and the CLI parses `RUST_LOG` for its global level instead of pulling in the
`env-filter` regex engine; both keep the binaries smaller.

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

MIT. This Rust SDK is a port of the upstream
[openclaw-weixin](https://github.com/Tencent/openclaw-weixin) project and
retains its copyright notice.

- Upstream `openclaw-weixin`: Copyright (C) 2026 Tencent. All rights reserved.
- Rust port (`iLink-SDK-rs`): Copyright (C) 2026 x500x. All rights reserved.

See [LICENSE](./LICENSE) for the full text.
