# Concepts & architecture

> 中文版: [中文文档](zh/concepts.md)

This page explains the pieces of the SDK and how data flows through them, so
the per-module guides are easier to follow.

## Layers

The crate is split into three layers. Most applications only use the top one.

```text
  WeixinClient  ── high-level handle (client module)
  WeixinApiClient ── raw HTTP transport + headers + JSON (api module)
  storage / cdn / messaging / media ── helpers used by the layers above
```

| Layer | Types | Responsibility |
| --- | --- | --- |
| High level | [`WeixinClient`] / [`WeixinClientBuilder`] | Resolve an account, expose `sender()`, `qr_login()`, the quote cache, start/stop notifications. |
| Low level | [`WeixinApiClient`] / [`WeixinApiOptions`] | Build headers, attach `base_info`, apply timeouts/cancellation, deserialize wire types. |
| Helpers | `cdn`, `messaging`, `storage`, `media` | Crypto, login flow, persistence, MIME detection. |

## Two client types

### `WeixinApiClient`

A thin async HTTP client built on `reqwest`. Each call maps to one CGI endpoint:

- `get_updates` — the long-poll inbox
- `send_message` — send one message item
- `get_upload_url` — pre-sign a CDN upload
- `get_config` — fetch the typing ticket
- `send_typing` — show/cancel the typing indicator
- `notify_start` / `notify_stop` — lifecycle pings
- `get_text` / `post_text_unauthenticated` — QR status polling paths

It is `Clone` (the transport is shared behind an `Arc`) and accepts a custom
[`HttpTransport`] for testing. See
[API client reference](reference/api-client.md).

### `WeixinClient`

The human-friendly handle built from a stored account. It wraps a
`WeixinApiClient` and a resolved [`StoredAccount`], and exposes:

- `sender()` → a [`MessageSender`] for text/media
- `qr_login()` → a [`QrLogin`] bound to the same transport
- `enable_quote_cache()` → turn on the local quote cache
- `notify_start()` / `notify_stop()` → lifecycle pings
- `account()`, `account_id()`, `token()`, `api()` → introspection

`WeixinClient` is `Clone` and cheap to share. It is designed to be owned by a
single task (the quote cache is `Send` but not `Sync`); clone it to hand it to
another task instead of sharing it by `&`.

## How messages flow

### Outbound

```text
MessageSender::send_text ─┐
MessageSender::send_file ─┤
                          ├─► build a WeixinMessage
                          │      ├─ text/items only?  ─► api.send_message
                          │      └─ media? ─► cdn::upload_* (get_upload_url
                          │                    ─► AES-128-ECB encrypt
                          │                    ─► POST ciphertext to the CDN)
                          │                 ─► api.send_message (with a CdnMedia reference)
```

### Inbound

```text
api.get_updates ─► WeixinMessage
                      ├─ body_from_items()  ─► text / voice transcript / media placeholders
                      ├─ download_item_media() ─► CDN download + AES-128-ECB decrypt
                      └─ message_to_context() ─► MsgContext (host-facing struct)
```

## Core data types

| Type | Location | Meaning |
| --- | --- | --- |
| [`WeixinMessage`] | `api::types` | The unified message (`item_list` is a list of `MessageItem`). |
| [`MessageItem`] | `api::types` | A single content item: text / image / video / file / voice / tool call. |
| [`CdnMedia`] | `api::types` | A CDN reference: `encrypt_query_param` + `aes_key`. |
| [`QrSession`] | `messaging::auth` | Credentials from a successful QR login. |
| [`StoredAccount`] | `storage::accounts` | A resolved account (token + URLs + flags). |
| [`MsgContext`] | `messaging::context` | Host-facing inbound context (body, media, quotes). |
| [`SendOpts`] | `messaging::outbound` | Common send options (`context_token`, `run_id`). |
| [`SendResult`] | `messaging::outbound` | Client id + server-assigned message id. |

## Lossless uint64 ids

Weixin message ids can exceed `i64::MAX`. They arrive on the wire as JSON
numbers; the SDK quotes the `message_id` / `msg_id` / `svr_id` fields before
parsing (see [`parse_api_json`]) so they survive as `String` without loss.
**Always treat message ids as strings** — never cast them to `u64`.

## Session guard

When `getUpdates` returns `errcode == -14` (invalid token), the SDK's
[session guard][`session_guard`] pauses every API call for that account for one
hour. This is automatic and process-wide; check
[`is_session_paused`](../src/api/session_guard.rs) before retrying.

## Threading model

- `WeixinApiClient` is `Clone + Send + Sync`.
- `WeixinClient` is `Clone + Send` but **not** `Sync` (the quote cache uses a
  `RefCell`-backed SQLite connection). Clone it per task.
- The QR registry and the session-guard registry are process-wide `Mutex` maps.

## Relationship to the TypeScript plugin

This is an independent, idiomatic Rust port of the TypeScript `openclaw-weixin`
channel plugin, **without** the OpenClaw gateway integration. There is no local
HTTP server, no webhook and no IPC — every operation is a function call. The
wire protocol (endpoints, headers, fields, timeouts, retry semantics) is
deliberately faithful; behavioural differences are covered by tests.
