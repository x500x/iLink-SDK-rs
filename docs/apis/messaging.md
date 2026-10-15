# Messaging API

The messaging board covers how to **send** outbound messages and how to **receive
and interpret** inbound ones. It spans three submodules of
[`../../src/messaging/`](../../src/messaging/): `outbound` (sending), `context`
(inbound interpretation + reply context tokens), and `typing` (typing indicator).
The long-poll `get_updates` belongs to the low-level client (see
[api-client.md](api-client.md)); here we explain how to drive it and turn raw
responses into usable contexts.

## Outbound: `outbound` submodule

### `SendOpts`

```rust
#[derive(Debug, Clone, Default)]
pub struct SendOpts {
    pub context_token: Option<String>,   // quote/reply token from an inbound message
    pub quoted_message: Option<QuoteMessageRecord>,  // richer quote (text/thumbnails)
    pub typing_ticket: Option<String>,   // from GetConfigResp, enables typing heartbeat
    pub mention: bool,                   // @-mention the recipient
    pub tool: Option<ToolCallItem>,      // embed a tool-call item
    pub extra: Option<ExtraPayload>,     // plugin-injected arbitrary JSON
    pub plain_attachment: Option<PlainAttachment>,  // file name + local path
}
```

`SendOpts::default()` is the common case (plain text, no quote). Set
`context_token` from an inbound `MsgContext` to reply in-thread.

### `SendResult`

```rust
pub struct SendResult {
    pub message_id: String,    // echoed back after send
    pub success: bool,         // ret == 0
}
```

### `MessageSender`

```rust
pub struct MessageSender;   // holds AccountData + Arc<WeixinApiClient> + cdn_base_url

impl MessageSender {
    /// Construct from an account + API client.
    pub fn new(account: &StoredAccount, api: Arc<WeixinApiClient>) -> Self;

    pub fn account_id(&self) -> &str;

    pub async fn send_text(&self, to: &str, text: &str, opts: &SendOpts) -> Result<SendResult>;
    pub async fn send_item(&self, to: &str, item: &MessageItem, opts: &SendOpts) -> Result<SendResult>;
    pub async fn send_file(&self, to: &str, path: &Path, opts: &SendOpts) -> Result<SendResult>;
    pub async fn send_bytes(&self, to: &str, data: &[u8], opts: &SendBytesOpts) -> Result<SendResult>;
    pub async fn send_image_bytes(&self, to: &str, data: &[u8], opts: &SendOpts) -> Result<SendResult>;
}
```

#### `SendBytesOpts`

```rust
pub struct SendBytesOpts {
    pub filename: String,
    pub mime: String,           // decides image/video/file branch via MediaKind
    pub opts: SendOpts,
}
```

`send_file` / `send_bytes` route by MIME (`MediaKind::from_path`):
- `image/*` → upload image (CDN) + `ImageItem`
- `video/*` → upload video (CDN) + `VideoItem`
- otherwise → upload file (CDN) + `FileItem`

`send_text` builds a `MessageItem` of type `Text` with `text_item` content.

#### Send flow (high level)

1. Build a `WeixinMessage` from item(s) + `context_token` + `base_info`.
2. `send_message` to the backend.
3. Read back `new_msg_id` / `x_wechat_seq` as the `message_id` (zero fallback).
4. If `typing_ticket` was set, the sender concurrently drives a typing heartbeat
   until `message_id` lands (see [typing](#typing-typinghelper)).

### Usage: sending text and files

```rust,no_run
use ilink_sdk::prelude::*;

# async fn example(sender: &MessageSender) -> ilink_sdk::Result<()> {
// Reply in-thread to an inbound message whose context_token we captured
let opts = SendOpts { context_token: Some("ctx-abc".into()), ..Default::default() };
let r = sender.send_text("someone@im.wechat", "hello", &opts).await?;
println!("sent {}", r.message_id);

// Send a local file (auto-detects image/video/file branch)
let f = sender.send_file("someone@im.wechat", std::path::Path::new("./report.pdf"),
    &SendOpts::default()).await?;
println!("file sent {}", f.message_id);
# Ok(())
# }
```

## Inbound: `context` submodule

Inbound `get_updates` responses arrive as [`WeixinMessage`](api-client.md) with
`item_list` and (possibly) `context_token`. The `context` submodule turns them
into a usable `MsgContext` and manages the reply-context token lifecycle.

```rust
pub const CONTEXT_TOKEN_FIELD: &str = "context_token";
pub const MEDIA_KEY_FIELD: &str = "media";

/// Per-account resolver of the reply context_token (mirrors TS ContextTokenStore).
pub struct ContextTokenStore;
impl ContextTokenStore {
    pub fn new(account_id: &str) -> Self;
    pub fn remember(&self, msg_id: &str, token: &str);
    pub fn resolve(&self, raw: Option<&str>, msg_id: &str) -> Option<String>;
}

/// The parsed inbound message (plain + quoted media entries).
pub struct MsgContext {
    pub from_user_id: Option<String>,
    pub to_user_id: Option<String>,
    pub sender_id: Option<String>,
    pub message_id: Option<String>,
    pub content: String,
    pub items: Vec<MessageItem>,
    pub media: Vec<MediaEntry>,
    pub quoted_content: Option<String>,
    pub quoted_media: Vec<MediaEntry>,
}

pub struct MediaEntry {
    pub kind: String,        // "image" | "file" | "voice" | "video" | "ref"
    pub url: Option<String>,
    pub path: Option<String>,
    pub aes_key: Option<String>,
    pub filename: Option<String>,
    pub mime: Option<String>,
    pub caption: Option<String>,
    pub ref_message_id: Option<String>,
}

// Free helpers
pub fn is_media_item(item: &MessageItem) -> bool;
pub fn media_label(item: &MessageItem) -> Option<String>;
pub fn message_id(...) -> Option<String>;                       // extract id from item/ref
pub fn body_from_items(items: &[MessageItem]) -> String;        // concat text items
pub fn message_to_context(...) -> MsgContext;                   // parse a WeixinMessage
pub fn remember_context_token(...) -> Option<String>;           // remember token for msg_id
```

### `message_to_context` semantics

- Concatenates `TextItem.text` items (and inline `ref`/`PartialText`) into
  `content`.
- For each `ImageItem` / `FileItem` / `VoiceItem` / `VideoItem`, builds a
  `MediaEntry` (with CDN URL / aes_key / path). To fetch the raw bytes, pass the
  entry to the CDN download helpers (see [cdn.md](cdn.md)).
- For an inline `ref` item (quoted message), fills `quoted_content` /
  `quoted_media` and adds a `MediaEntry` of kind `"ref"`.
- `context_token` (for replying) is remembered via `ContextTokenStore` keyed by
  the inbound `message_id`.

### Usage: parse inbound and reply

```rust,no_run
use ilink_sdk::prelude::*;
use ilink_sdk::messaging::context::message_to_context;

# fn example(msg: ilink_sdk::api::types::WeixinMessage, sender: &MessageSender) {
// After get_updates returns `msg`
let ctx = message_to_context(msg, "account-id", "cdn-base-url");
println!("from={:?} text={}", ctx.from_user_id, ctx.content);
for m in &ctx.media {
    println!("media {} -> {:?}", m.kind, m.url);
}
// Reply in-thread using the remembered context_token
if let Some(token) = ctx.quoted_content.as_ref() { /* ... */ }
# }
```

## Typing: `typing` submodule

The typing indicator is a heartbeat: while a bot is "thinking", send periodic
`TypingStatus::Typing` requests; stop when the message completes.

```rust
pub struct Typing;       // holds account + api + cached typing_ticket

impl Typing {
    pub fn new(account: &StoredAccount, api: Arc<WeixinApiClient>) -> Self;
    pub async fn ensure_ticket(&self) -> Result<()>;   // fetch from get_config if missing
    pub fn ticket(&self) -> Option<String>;
    pub async fn notify_start(&self, to_user_id: &str) -> Result<()>;
    pub async fn notify_stop(&self, to_user_id: &str) -> Result<()>;
}

/// Convenience: run a typing heartbeat while `job` executes, cancel + stop on drop/err.
pub async fn send_typing<F, Fut>(
    account: &StoredAccount,
    api: Arc<WeixinApiClient>,
    to_user_id: &str,
    job: F,
) -> Result<()>
where F: FnOnce() -> Fut + Send, Fut: std::future::Future<Output = ()> + Send;

pub struct TypingHelper;   // low-level: drives the periodic loop
```

`send_typing` is the primary helper: it starts typing, runs `job` (your
generation/send), and ensures `notify_stop` happens even on error or cancellation.
The high-level `MessageSender` uses it internally when `SendOpts::typing_ticket`
is set.

### Usage

```rust,no_run
use ilink_sdk::prelude::*;
use ilink_sdk::messaging::typing::send_typing;

# async fn example(account: &StoredAccount, api: std::sync::Arc<WeixinApiClient>) -> ilink_sdk::Result<()> {
send_typing(account, api, "someone@im.wechat", || async {
    // ... long generation / tool call ...
}).await?;
# Ok(())
# }
```

## Design notes and caveats

1. **`context_token` is how you reply in-thread.** Persist it (via
   `remember_context_token` / `ContextTokenStore`) and pass it back through
   `SendOpts::context_token`; without it the message is a fresh thread.
2. **File branch chooses by MIME, not extension.** `send_bytes`/`send_file` use
   `MediaKind::from_path` — `image/*` and `video/*` get special items; everything
   else is `File`. A `.pdf` goes through the file branch.
3. **`SendResult.success` is `ret == 0`.** A non-zero `ret` is an `Error`, but
   `success=false` (e.g. absent `ret`) is not itself an error — read `message_id`
   before assuming failure.
4. **Typing is best-effort.** If `typing_ticket` is missing, `ensure_ticket` pulls
   it from `get_config`; if the backend rejects it, typing silently no-ops rather
   than failing the send.
5. **Media bytes are not downloaded by `message_to_context`.** It only records
   URLs/aes_keys; call the CDN download helpers to materialize files. The high-level
   `WeixinClient` (in the CLI) does this automatically.
6. **`get_updates` long-poll belongs to the low-level client.** This board explains
   *interpreting* its results; see [api-client.md](api-client.md) for the
   `get_updates` call itself.
