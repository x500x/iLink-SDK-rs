# Storage & state

The SDK persists everything to a local state directory — no cloud or registry
service. This page covers the layout, account storage, the sync cursor, and the
quote cache.

## State directory

[`resolve_state_dir`](../../src/storage/state_dir.rs) resolves the root in this
order (first non-empty wins):

1. `OPENCLAW_STATE_DIR`
2. `CLAWDBOT_STATE_DIR`
3. `$XDG_STATE_HOME/openclaw` (Unix, when set) — Rust SDK addition for desktop
   Linux; disable by setting `OPENCLAW_STATE_DIR`
4. `$HOME/.openclaw` (or `%USERPROFILE%\.openclaw` on Windows)

The SDK's own data lives under `<root>/openclaw-weixin/`:

```text
<root>/openclaw-weixin/
├── accounts.json                      # registered account ids (registration order)
├── accounts/
│   ├── <id>.json                      # token, baseUrl, userId, savedAt
│   ├── <id>.sync.json                 # getUpdates cursor
│   └── <id>.context-tokens.json       # reply context tokens
├── ref-messages.sqlite                # quote cache
└── debug-mode.json                    # per-account debug flag (host-managed)
```

This mirrors the TypeScript plugin's layout, so a Rust and a TypeScript client
can share credentials on one machine.

## Accounts

[`storage::accounts`](../../src/storage/accounts.rs) is the credential store.

### Account id normalization

Weixin account ids (`b0f5860fdecb@im.bot`) are turned into filesystem-safe keys
(`b0f5860fdecb-im-bot`) by [`normalize_account_id`]. The inverse is
[`derive_raw_account_id`] (`-im-bot` → `@im.bot`, `-im-wechat` → `@im.wechat`).

### Reading / writing

| Function | Purpose |
| --- | --- |
| `save_account(id, &AccountUpdate)` | Merge + persist token/base_url/user_id (0600 on Unix). |
| `register_account_id(id)` | Add `id` to the index (no-op if present). |
| `unregister_account_id(id)` | Remove `id` from the index. |
| `list_indexed_account_ids()` / `list_account_ids()` | All registered ids. |
| `load_account(id)` | Load `AccountData`, with legacy fallbacks. |
| `resolve_account(id)` | Merge stored data with `openclaw.json` → [`StoredAccount`]. |
| `clear_account(id)` | Delete all files for an account. |
| `clear_stale_accounts_for_user_id(...)` | Prune other accounts bound to the same Weixin user. |

`AccountUpdate` merges: a non-empty `token` overwrites; a non-empty `base_url`
overwrites; `user_id` is cleared when explicitly `""`, kept when `None`.

### `StoredAccount`

The resolved view of an account: `account_id`, `base_url`, `cdn_base_url`
(default [`CDN_BASE_URL`]), `token`, `enabled`, `configured`, `name`,
`user_id`, `route_tag`. `WeixinClient::account()` returns this.

## Sync cursor (long-poll resume)

[`storage::sync_buf`](../../src/storage/sync_buf.rs) persists the
`get_updates_buf` cursor so a restart resumes instead of replaying the backlog.

```rust,no_run
# fn run(account_id: &str) {
use ilink_sdk::storage::sync_buf::{load_for_account, save_for_account, get_sync_buf_path};

let cursor = load_for_account(account_id);            // Option<String>
if let Some(buf) = &cursor {
    let _ = save_for_account(account_id, buf);          // persist after each poll
}
let _ = get_sync_buf_path(account_id);                 // <accounts>/<id>.sync.json
# }
```

Load order for compatibility: normalized account file → raw-id file → legacy
single-account file.

## Quote cache

Newer Weixin clients send only a server message id for a quoted message. The
local quote cache ([`messaging::quote_cache`], `ref-messages.sqlite`) stores
each message's body and managed media so later quotes can restore context.

### Opening

```rust,no_run
# fn run() -> ilink_sdk::Result<()> {
use ilink_sdk::messaging::quote_cache::QuoteStore;

let store = QuoteStore::open_default()?;   // <root>/openclaw-weixin/ref-messages.sqlite
# Ok(())
# }
```

Or enable it on a [`WeixinClient`]:

```rust,no_run
# fn run(client: &mut ilink_sdk::WeixinClient) -> ilink_sdk::Result<()> {
client.enable_quote_cache()?;
let _ = client.quote_store();
# Ok(())
# }
```

### Records

- [`QuoteMessageRecord`] — a cached message (account, conversation, id,
  direction, body, media path/mime/name/size, `created_at`).
- [`QuoteMessageInput`] — what you pass to [`QuoteStore::put`] (same shape plus
  a `source_media_path` that gets registered into the managed media root).
- [`Direction`] — `Inbound` (default) or `Outbound`.

### Policy & defaults

The cache is governed by [`QuoteCachePolicy`]. Defaults (and the `openclaw.json`
keys that override them):

| Field | Default | `openclaw.json` key |
| --- | --- | --- |
| `enabled` | `true` | `quoteCache.enabled` |
| `retention_ms` | 30 days | `quoteCache.retentionDays` (days) |
| `max_messages_per_account` | 10,000 | `quoteCache.maxMessagesPerAccount` |
| `media_retention_ms` | 7 days | `quoteCache.mediaRetentionDays` (days) |
| `max_media_bytes_per_account` | 256 MiB | `quoteCache.maxMediaBytesPerAccount` |
| `max_single_media_bytes` | 25 MiB | `quoteCache.maxSingleMediaBytes` |

Invalid values fall back to their defaults (mirroring the TypeScript
`positiveNumber` helper). GC runs after every `GC_WRITE_INTERVAL` (100) writes
and on an hourly timer. Managed media lives under
`media/inbound/openclaw-weixin-quotes/<account>/` (`QUOTE_MEDIA_SUBDIR`).

### Disabling

Set `quoteCache.enabled: false` in the channel config, or construct the store
with `enabled: false`. A disabled cache stores nothing and returns misses.
