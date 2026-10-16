# Quote cache API

> 中文版: [中文文档](../zh/apis/quote-cache.md)

The quote cache ([`../../src/messaging/quote_cache.rs`](../../src/messaging/quote_cache.rs))
is a local SQLite store of recent messages so the SDK (and the host) can attach
proper reply context: `@-mention` text, quoted message text, and inline media
thumbnails. It mirrors TypeScript's `src/quote/quote-cache.ts`. The high-level
`WeixinClient` exposes it via `quote_store()`.

> There are **two** `QuoteCachePolicy` types in the crate:
> `storage::config::QuoteCachePolicy` (raw config) and
> `messaging::quote_cache::QuoteCachePolicy` (normalized runtime policy, used here).
> This board documents the latter.

## `QuoteCachePolicy`

```rust
#[derive(Debug, Clone)]
pub struct QuoteCachePolicy {
    pub enabled: bool,
    pub retention_days: u64,                   // message row retention
    pub max_messages_per_account: u64,         // cap rows per account
    pub media_retention_days: u64,             // downloaded media retention
    pub max_media_bytes_per_account: u64,      // media disk budget
    pub max_single_media_bytes: u64,           // per-file size cap
}
impl Default for QuoteCachePolicy { /* see below */ }
```

### Defaults

```rust
pub const DEFAULT_RETENTION_DAYS: u64 = 30;
pub const DEFAULT_MAX_MESSAGES_PER_ACCOUNT: u64 = 10_000;
pub const DEFAULT_MEDIA_RETENTION_DAYS: u64 = 7;
pub const DEFAULT_MAX_MEDIA_BYTES_PER_ACCOUNT: u64 = 256 * 1024 * 1024;  // 256 MiB
pub const DEFAULT_MAX_SINGLE_MEDIA_BYTES: u64 = 25 * 1024 * 1024;        // 25 MiB
pub const QUOTE_MEDIA_SUBDIR: &str = "quote-media";
```

## `Direction`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction { Inbound = 0, Outbound = 1 }
```

## `QuoteMessageInput`

The data written on each message:

```rust
#[derive(Debug, Clone)]
pub struct QuoteMessageInput {
    pub message_id: String,
    pub account_id: String,
    pub user_id: String,
    pub sender_id: String,
    pub direction: Direction,
    pub timestamp_ms: u128,
    pub text: Option<String>,                 // plain text (from body_from_items)
    pub media_kind: Option<String>,           // "image" | "video" | "file" | "voice"
    pub media_path: Option<String>,           // local downloaded path
    pub media_mime: Option<String>,
    pub media_filename: Option<String>,
    pub media_caption: Option<String>,
    pub tool_call_id: Option<String>,
    pub raw: Option<String>,                  // raw JSON for fidelity
}
```

## `QuoteMessageRecord`

The row returned by `find` / `get`: equals `QuoteMessageInput` plus `rowid` and
computed `age_days`. Used to build `SendOpts::quoted_message` (text + thumbnails)
so the reply shows the quoted snippet in the Weixin UI.

## `QuoteStore`

```rust
#[derive(Clone)]
pub struct QuoteStore;   // wraps a connection pool + policy + media dir

impl QuoteStore {
    /// Open at the default path (sdk_state_dir()/ref-messages.sqlite).
    pub fn open_default(policy: &QuoteCachePolicy) -> Result<Self>;

    /// Open at a custom directory (sqlite + quote-media live under it).
    pub fn open_at(dir: &Path, policy: &QuoteCachePolicy) -> Result<Self>;

    /// Open at the resolved SDK state dir with the given policy.
    pub fn open(policy: &QuoteCachePolicy) -> Result<Self>;

    pub fn set_policy(&self, policy: &QuoteCachePolicy);  // live update (re-evaluates enabled)
    pub fn policy(&self) -> QuoteCachePolicy;

    /// Insert/update a message row (upsert by message_id).
    pub fn put(&self, input: &QuoteMessageInput) -> Result<()>;

    /// Lookup by message_id (Option). Includes media path if present.
    pub fn find(&self, account_id: &str, message_id: &str) -> Result<Option<QuoteMessageRecord>>;

    /// Remove all rows + media for one account.
    pub fn delete_account(&self, account_id: &str) -> Result<()>;

    /// Garbage-collect expired rows/media per the policy. Safe to call periodically.
    pub fn gc(&self) -> Result<()>;

    /// Directory where downloaded media is stored.
    pub fn account_media_dir(&self, account_id: &str) -> PathBuf;
}
```

### Opening

- `open_default()` = `open_at(sdk_state_dir(), policy)`. `sdk_state_dir()` resolves
  `OPENCLAW_STATE_DIR` / `CLAWDBOT_STATE_DIR` / `~/.openclaw/openclaw-weixin`.
- `policy.enabled == false` (or `max_messages_per_account == 0` or
  `max_media_bytes_per_account == 0`) → the store still opens but `put`/`find`
  become no-ops and `enable_quote_cache()` on the client returns `false`.

### `gc` details

`gc` runs three maintenance steps, each guarded by the policy:

1. **Row retention.** Delete rows older than `retention_days` per account.
2. **Per-account row cap.** When a single account exceeds
   `max_messages_per_account`, delete its oldest rows beyond the cap.
3. **Media retention + disk budget.** Delete media files older than
   `media_retention_days`; then, while an account's media dir exceeds
   `max_media_bytes_per_account`, delete its oldest media files (each exceeding
   `max_single_media_bytes` is skipped so one huge file can't evict everything).

All `gc` steps are best-effort: a single failure is logged and does not abort the
others.

## Usage examples

### Open and record an inbound message

```rust,no_run
use ilink_sdk::prelude::*;
use ilink_sdk::messaging::quote_cache::{
    QuoteStore, QuoteCachePolicy, QuoteMessageInput, Direction,
};

# fn example() -> ilink_sdk::Result<()> {
let store = QuoteStore::open_default(&QuoteCachePolicy::default())?;
let input = QuoteMessageInput {
    message_id: "msg-123".into(),
    account_id: "acct".into(),
    user_id: "user".into(),
    sender_id: "user@im.wechat".into(),
    direction: Direction::Inbound,
    timestamp_ms: crate::util::time::now_millis(),
    text: Some("hello".into()),
    media_kind: None,
    media_path: None,
    media_mime: None,
    media_filename: None,
    media_caption: None,
    tool_call_id: None,
    raw: None,
};
store.put(&input)?;

if let Some(rec) = store.find("acct", "msg-123")? {
    println!("cached text: {:?}", rec.text);
}
# Ok(())
# }
```

### Periodic GC (daemon-style)

```rust,no_run
use ilink_sdk::prelude::*;
use ilink_sdk::messaging::quote_cache::QuoteStore;

# fn example(store: &QuoteStore) -> ilink_sdk::Result<()> {
store.gc()?;   // call every few minutes from a background task
# Ok(())
# }
```

## Design notes and caveats

1. **Two `QuoteCachePolicy` types exist.** `storage::config::QuoteCachePolicy` is the
   raw config shape; `messaging::quote_cache::QuoteCachePolicy` is the normalized
   runtime one. Pass the latter into `QuoteStore`.
2. **Disabled store is a silent no-op.** When `enabled == false` (or the zero caps),
   `put`/`find`/`gc` do nothing and return `Ok(None)`/`Ok(())` — no error, no disk
   writes. `WeixinClient::enable_quote_cache()` reflects exactly this.
3. **Media is stored locally.** `media_path` points under `account_media_dir`;
   `gc` physically deletes files past `media_retention_days` / the disk budget.
4. **`raw` preserves fidelity.** Store the original `WeixinMessage` JSON in `raw` if
   you need to reconstruct items exactly; `find` returns it unchanged.
5. **`find` is per-account.** Always pass the same `account_id` used in `put`;
   cross-account lookups return `None`.
6. **`gc` is safe to call often.** It is incremental and bounded; the CLI calls it
   periodically. A failed step does not abort the others.
