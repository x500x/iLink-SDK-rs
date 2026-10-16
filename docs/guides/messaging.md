# Messaging

> 中文版: [中文文档](../zh/guides/messaging.md)

This guide covers receiving messages (long poll), sending text and media,
typing indicators, and the conversation context token.

## Receiving messages (long poll)

`WeixinApiClient::get_updates` is the inbox. It long-polls the server, which
holds the request until a message arrives or the long-poll timeout (default
35s, [`DEFAULT_LONG_POLL_TIMEOUT_MS`]) is hit.

```rust,no_run
# async fn run(client: ilink_sdk::WeixinClient) -> ilink_sdk::Result<()> {
use ilink_sdk::api::types::{GetUpdatesReq, MessageItemType};
use ilink_sdk::storage::sync_buf::load_for_account;
use tokio_util::sync::CancellationToken;

let account_id = client.account_id().to_string();
let mut cursor = load_for_account(&account_id);

// Optional: cancel the long poll from another task. is_abort()-style errors
// still return an empty ret:0 with the same cursor, so the loop stays correct.
let cancel = CancellationToken::new();

loop {
    let resp = client.api().get_updates(
        &GetUpdatesReq { get_updates_buf: cursor.clone(), sync_buf: None },
        None,   // default 35s long-poll timeout
        Some(cancel.clone()),
    ).await?;

    if let Some(buf) = resp.get_updates_buf {
        cursor = Some(buf);
        ilink_sdk::storage::sync_buf::save_for_account(&account_id, &cursor.clone().unwrap_or_default());
    }

    for msg in resp.msgs.unwrap_or_default() {
        for item in msg.item_list.unwrap_or_default() {
            match item.r#type.and_then(MessageItemType::from_value) {
                Some(MessageItemType::Text) => { /* item.text_item.text */ }
                Some(MessageItemType::Voice) => { /* item.voice_item.text = transcript */ }
                Some(MessageItemType::Image) => { /* download via cdn */ }
                _ => {}
            }
        }
    }

    if cancel.is_cancelled() { break; }
}
# Ok(())
# }
```

- The cursor (`get_updates_buf`) is persisted with
  [`save_for_account`](../../src/storage/sync_buf.rs) so a restart resumes
  instead of replaying the backlog.
- A client-side timeout or external cancellation returns an **empty `ret: 0`
  response with the same cursor** — just loop. Other errors propagate as
  [`Error`].
- Use [`GetUpdatesResp::is_api_error`] / [`is_stale_token`] to detect business
  errors (e.g. `errcode == -14` stale token).

## Sending text

```rust,no_run
# async fn run(client: ilink_sdk::WeixinClient) -> ilink_sdk::Result<()> {
let to = "someone@im.wechat";

client.sender().send_text(
    to,
    "hello from Rust",
    &ilink_sdk::messaging::SendOpts {
        context_token: Some("ctx-abc".into()),   // echo the inbound token
        run_id: None,
    },
).await?;
# Ok(())
# }
```

`context_token` is **required to route the reply into the same conversation**.
A missing token is logged as a warning but does not block the send. Capturing
it from inbound messages is described below.

## Sending files / media

`send_file` uploads the local file to the CDN (AES-128-ECB encrypted) and then
sends a message referencing it. The media type (image / video / file) is
detected from the file extension.

```rust,no_run
# async fn run(client: ilink_sdk::WeixinClient) -> ilink_sdk::Result<()> {
use std::path::Path;
use ilink_sdk::storage::accounts::CDN_BASE_URL;

let to = "someone@im.wechat";
let cdn_base_url = client.account().cdn_base_url.clone();

client.sender().send_file(
    to,
    Path::new("./photo.png"),
    "look at this",            // optional caption (sent as a separate text item)
    &ilink_sdk::messaging::SendOpts::default(),
    &cdn_base_url,
).await?;
# Ok(())
# }
```

There are also convenience methods for raw bytes:

- `send_bytes(to, data, file_name, caption, opts, cdn_base_url)` — bytes as a
  generic file attachment.
- `send_image_bytes(to, data, extension, caption, opts, cdn_base_url)` —
  bytes as an image.
- `send_item(to, MessageItem, opts)` — send one pre-built item.

All media sends route through [`cdn::upload_media`], which calls
`get_upload_url`, encrypts with AES-128-ECB, and POSTs the ciphertext, then
returns the download parameter that becomes the `CdnMedia` reference.

## Typing indicators

Before sending a reply, you may show a typing indicator. First fetch a typing
ticket with `get_config`, then call `send_typing` (or use the
[`TypingHelper`]).

```rust,no_run
# async fn run(client: ilink_sdk::WeixinClient, user_id: &str) -> ilink_sdk::Result<()> {
use ilink_sdk::messaging::Typing;

let cfg = client.api().get_config(user_id, None).await?;
let ticket = cfg.typing_ticket.unwrap_or_default();

// An empty ticket is a no-op (the account may not support typing).
ilink_sdk::messaging::send_typing(client.api(), user_id, &ticket, Typing::Typing).await?;
// ... send the reply ...
ilink_sdk::messaging::send_typing(client.api(), user_id, &ticket, Typing::Cancel).await?;
# Ok(())
# }
```

- `Typing::Typing` → wire `status = 1`; `Typing::Cancel` → `status = 2`.
- An empty ticket short-circuits without contacting the network.

## Conversation context tokens

The backend issues a `context_token` per inbound message. **You must echo it on
every outbound send** into that conversation, otherwise the reply will not
thread correctly. The SDK gives you two tools:

### `ContextTokenStore` (persistent, multi-account)

```rust,no_run
# fn run() {
use ilink_sdk::messaging::context::ContextTokenStore;

let mut store = ContextTokenStore::new();       // defaults to the accounts dir
store.set("acct", "user@im.wechat", "ctx-xyz");  // memory + disk
let token = store.get("acct", "user@im.wechat"); // Some("ctx-xyz")
let _ = store.restore("acct");                   // reload persisted tokens
# }
```

On startup, call `restore(account_id)` to reload tokens written by a previous
run.

### Building a host-facing context

When an inbound message arrives, convert it into a [`MsgContext`] for your
application: it flattens the body, attaches downloaded media paths, and pulls
inline quote (`ref_msg`) metadata into `reply_to_*` fields.

```rust,no_run
# fn run(msg: ilink_sdk::api::types::WeixinMessage, account_id: &str, media: ilink_sdk::cdn::download::InboundMedia) {
use ilink_sdk::messaging::context::message_to_context;

let ctx = message_to_context(&msg, account_id, &media);
println!("body = {}", ctx.body);
println!("context_token = {:?}", ctx.context_token);
# }
```

- `message_to_context` derives the body via [`body_from_items`] (text first,
  then voice transcript, then a media placeholder label such as `[图片]`; these
  Chinese labels are emitted verbatim by the protocol).
- `remember_context_token(&mut store, &ctx)` stashes the token back into a
  `ContextTokenStore`.
- Inline quotes populate `reply_to_id`, `reply_to_body`, `reply_to_is_quote`.

## Inline quotes and the quote cache

Newer Weixin clients send only a server message id for a quoted message. To let
later quotes restore their original text/media, enable the local quote cache:

```rust,no_run
# fn run(client: &mut ilink_sdk::WeixinClient) -> ilink_sdk::Result<()> {
client.enable_quote_cache()?;        // opens ref-messages.sqlite
# Ok(())
# }
```

The cache ([`QuoteStore`]) stores each message's body and managed media locally,
keyed by account + conversation + message id, with retention and size caps (see
[Storage & state](storage.md)). `Direction::Inbound` and
`Direction::Outbound` are both supported.
