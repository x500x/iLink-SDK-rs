# iLink SDK — Rust SDK documentation

A Rust SDK for the Tencent **Weixin (iLink)** backend protocol: QR-code
login, long-poll message delivery, and CDN media upload/download, packaged as
a plain library with no embedded HTTP server or gateway dependency.

This `docs/` folder complements the top-level [`README.md`](../README.md) with
practical guides and a module reference.

> 中文版（翻译）见 [`docs/zh/README.md`](zh/README.md) — 中文文档：
> [Chinese (docs/zh)](zh/README.md).

| Guide | What it covers |
| --- | --- |
| [Getting started](getting-started.md) | Add the crate, run your first QR login + send. |
| [Concepts & architecture](concepts.md) | Module map, data flow, the key types. |
| [Authentication (QR login)](guides/authentication.md) | The QR-code login flow and session handling. |
| [Messaging](guides/messaging.md) | Long-poll receive, sending text/media, typing, context tokens. |
| [Media (CDN)](guides/media.md) | AES-128-ECB upload/download, voice transcoding. |
| [Storage & state](guides/storage.md) | Accounts, state directory, sync cursor, quote cache. |
| [Errors & retries](reference/errors.md) | `ErrorKind` classification and retry policies. |
| [Configuration](reference/configuration.md) | `openclaw.json`, environment variables, feature flags. |
| [CLI](reference/cli.md) | The bundled `weixin` binary. |
| [C ABI (dll / so / dylib)](reference/ffi.md) | Call the SDK from C/C++/Go via the shared/static library. |
| [API client reference](reference/api-client.md) | The low-level `WeixinApiClient`. |
| [API handbook](reference/api.md) | Exhaustive catalog of every public type, function, and constant. |
| [API by board](apis/README.md) | Detailed, per-area guides for each API surface. |

> Prefer the [API handbook](reference/api.md) when you need the exact signature
> of a specific public item; it mirrors `src/lib.rs` and every module file. For
> narrative, usage-first explanations, see the [API by board](apis/README.md)
> guides.

## Crate layout

```text
ilink_sdk/
├── api            HTTP client + wire types + session guard
├── cdn            AES-128-ECB crypto + CDN upload/download
├── client         high-level WeixinClient / builder
├── error          Error / ErrorKind / Result
├── ffi            C ABI (dll/so/dylib) JSON dispatch
├── logging        tracing subscriber helpers (cli feature)
├── media          MIME detection
├── messaging      auth (QR), outbound, inbound context, typing, quote cache
├── storage        accounts, state dir, sync buf, config
├── tls            rustls config
└── util           random / redact / time helpers
```

## Minimum requirements

| | Requirement |
| --- | --- |
| Rust | `1.82` or newer (edition 2021) |
| Async runtime | Tokio (`rt-multi-thread`) |
| Feature flags | `cli` is enabled by default |

```bash
cargo build --no-default-features   # library + C ABI, no CLI
cargo build --release               # also emits ilink_sdk.dll / .so / .dylib + .lib / .a
cargo doc --no-deps --open          # build this reference locally
```
