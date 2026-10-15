# Storage & config API

The storage and config module ([`../../src/storage/`](../../src/storage/)) handles
local persistence: account credentials, the state directory, long-poll cursors,
and the `openclaw.json` `channels.openclaw-weixin` config section. It corresponds
to TypeScript's `src/auth/accounts.ts`, `src/storage/state-dir.ts`,
`src/storage/sync-buf.ts`, and config reading.

## Module composition

| Submodule | File | Responsibility |
| --- | --- | --- |
| `accounts` | [`../../src/storage/accounts.rs`](../../src/storage/accounts.rs) | per-account credentials + index |
| `state_dir` | [`../../src/storage/state_dir.rs`](../../src/storage/state_dir.rs) | state dir resolution |
| `sync_buf` | [`../../src/storage/sync_buf.rs`](../../src/storage/sync_buf.rs) | `get_updates_buf` cursor |
| `config` | [`../../src/storage/config.rs`](../../src/storage/config.rs) | config section reading |

## `accounts` submodule

### Constants & core structs

```rust
pub const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
pub const CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";

/// Unified account data: token, base URL, pairing user id.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountData {
    pub token: Option<String>,
    pub saved_at: Option<String>,     // ISO-8601 timestamp
    pub base_url: Option<String>,
    pub user_id: Option<String>,
}
impl AccountData {
    pub fn is_configured(&self) -> bool;            // token is non-empty
    pub fn effective_base_url(&self) -> String;     // falls back to DEFAULT_BASE_URL
}

/// Account after merging stored credentials + config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAccount {
    pub account_id: String,
    pub base_url: String,
    pub cdn_base_url: String,
    pub token: Option<String>,
    pub enabled: bool,
    pub configured: bool,
    pub name: Option<String>,
    pub user_id: Option<String>,
    pub route_tag: Option<String>,
}

/// Partial update (only non-None fields overwrite stored data).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountUpdate {
    pub token: Option<String>,
    pub base_url: Option<String>,
    pub user_id: Option<String>,
}
```

### Account id normalization

```rust
/// "b0f5860fdecb@im.bot" -> "b0f5860fdecb-im-bot"
pub fn normalize_account_id(raw: &str) -> String;
/// reverse pattern match: "b0f5860fdecb-im-bot" -> Some("b0f5860fdecb@im.bot"); no match -> None
pub fn derive_raw_account_id(normalized_id: &str) -> Option<String>;
```

### Index & registration

```rust
/// Registered account ids (in registration order).
pub fn list_indexed_account_ids() -> Vec<String>;
pub fn list_account_ids() -> Vec<String>;                  // alias

/// Add account_id to the persistent index (no-op if already present).
pub fn register_account_id(account_id: &str) -> Result<()>;
/// Remove from the persistent index.
pub fn unregister_account_id(account_id: &str) -> Result<()>;
```

Index file: `<sdk_state_dir>/accounts.json` (JSON array).

### Read / write / resolve

```rust
/// Load account data (with legacy-install fallbacks).
pub fn load_account(account_id: &str) -> Option<AccountData>;

/// Persist account data (merge with existing file).
pub fn save_account(account_id: &str, update: &AccountUpdate) -> Result<AccountData>;

/// Resolve an account: merge stored credentials + local config.
pub fn resolve_account(account_id: &str) -> Result<StoredAccount>;
pub fn resolve_account_with(
    account_id: &str, overrides: Option<&BTreeMap<String, BTreeMap<String, serde_json::Value>>>
) -> Result<StoredAccount>;

/// Delete all files for an account (credentials, sync, context-tokens).
pub fn clear_account(account_id: &str) -> Result<()>;

/// Clear stale accounts for the same user_id after a login.
pub fn clear_stale_accounts_for_user_id(
    current_account_id: &str, user_id: &str, on_clear_context_tokens: impl FnMut(&str)
) -> Result<()>;
```

### `save_account` merge semantics

- `token`: only overwrites when `update.token` is **non-empty**; an empty string
  does **not** clear an existing token.
- `base_url`: stored only when non-empty.
- `user_id`: explicitly setting `""` clears it; `None` keeps the existing value.
- `saved_at` (ISO-8601) is always written.

> ⚠️ This "empty does not overwrite" behavior is deliberate: to clear a field,
> follow the rules (e.g. `user_id=""` clears; `token`/`base_url` empty keeps the
> old value), otherwise the old value is preserved.

## `state_dir` submodule

```rust
/// Resolve the local state directory (first non-empty wins):
///  1. OPENCLAW_STATE_DIR
///  2. CLAWDBOT_STATE_DIR
///  3. $XDG_STATE_HOME/openclaw (Unix only, when set)
///  4. $HOME/.openclaw
pub fn resolve_state_dir() -> PathBuf;

/// resolve_state_dir()/openclaw-weixin
pub fn sdk_state_dir() -> PathBuf;
/// sdk_state_dir()/accounts
pub fn accounts_dir() -> PathBuf;

/// Create the directory (and parents), return the path.
pub fn ensure_dir(path: &Path) -> std::io::Result<&Path>;
```

State directory layout:

```text
$OPENCLAW_STATE_DIR/openclaw-weixin/
├── accounts.json                     # registered account id index
├── accounts/
│   ├── <id>.json                     # token, baseUrl, userId
│   ├── <id>.sync.json                # getUpdates cursor
│   └── <id>.context-tokens.json      # reply context tokens
├── ref-messages.sqlite               # quote cache (see quote-cache.md)
└── debug-mode.json                   # per-account debug flag
```

## `sync_buf` submodule

Persists the `get_updates_buf` cursor so a restart can resume instead of replaying
history.

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncBufData { pub get_updates_buf: String }
impl SyncBufData { pub fn new(get_updates_buf: impl Into<String>) -> Self; }

/// <accounts_dir>/<account_id>.sync.json
pub fn get_sync_buf_path(account_id: &str) -> PathBuf;

/// Load cursor; fallback order: given path -> derived raw-id filename -> legacy single-account path.
pub fn load_get_updates_buf(path: &Path) -> Option<String>;
pub fn load_for_account(account_id: &str) -> Option<String>;

/// Save cursor (create parents), JSON-escaped value.
pub fn save_get_updates_buf(path: &Path, get_updates_buf: &str) -> std::io::Result<()>;
pub fn save_for_account(account_id: &str, get_updates_buf: &str) -> std::io::Result<()>;

/// Delete the account cursor.
pub fn clear_for_account(account_id: &str);
```

## `config` submodule

Reads the host `openclaw.json` `channels.openclaw-weixin` section (shares the source
with the TS plugin, but the Rust SDK has no host config object, so it reads from
disk directly).

```rust
pub const CHANNEL_CONFIG_SECTION: &str = "openclaw-weixin";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdkConfig {
    pub accounts: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    pub reply_progress_messages: bool,
    pub quote_cache: QuoteCacheRaw,
    pub channel_config_updated_at: Option<String>,
    pub route_tag: Option<serde_json::Value>,
    pub bot_agent: Option<String>,
}

pub struct QuoteCacheRaw { /* camelCase raw values */ }
impl QuoteCacheRaw {
    /// Normalize to storage::config::QuoteCachePolicy (invalid -> defaults).
    pub fn to_policy(&self) -> QuoteCachePolicy;
}

pub struct QuoteCachePolicy { /* enabled, retention_ms, max_messages_per_account, media_retention_ms, max_media_bytes_per_account, max_single_media_bytes */ }
impl Default for QuoteCachePolicy { /* see config.rs */ }
```

### Config reading functions

```rust
/// OPENCLAW_CONFIG first, else <state_dir>/openclaw.json.
pub fn resolve_config_path() -> PathBuf;

pub fn load_local_config() -> SdkConfig;
pub fn load_local_config_from(path: &Path) -> SdkConfig;   // missing/corrupt -> defaults
pub fn resolve_channel_config(overrides: Option<SdkConfig>) -> SdkConfig;

pub fn load_route_tag(account_id: &str) -> Option<String>;
pub fn load_bot_agent() -> Option<String>;
```

### Config keys (top-level `channels.openclaw-weixin`)

| Key | Type | Default | Purpose |
| --- | --- | --- | --- |
| `botAgent` | string | `OpenClaw` | observability label (not auth), sanitized |
| `routeTag` | string \| number | — | forwarded as `SKRouteTag` |
| `accounts.<id>.baseUrl` | string | `https://ilinkai.weixin.qq.com` | per-account API base URL |
| `accounts.<id>.cdnBaseUrl` | string | `https://novac2c.cdn.weixin.qq.com/c2c` | per-account CDN base URL |
| `accounts.<id>.enabled` | boolean | `true` | disable an account without deleting |
| `quoteCache` | object | see below | local quote cache policy |

`quoteCache` sub-keys: `enabled`, `retentionDays`(30), `maxMessagesPerAccount`(10000),
`mediaRetentionDays`(7), `maxMediaBytesPerAccount`(256 MiB), `maxSingleMediaBytes`
(25 MiB). Invalid values fall back to defaults (matching TS `positiveNumber` behavior).

## Usage examples

### Save & register an account after login

```rust,no_run
use openclaw_weixin::prelude::*;
use openclaw_weixin::storage::accounts::{self, AccountUpdate};

# fn example(session: QrSession) -> openclaw_weixin::Result<()> {
let account_id = accounts::normalize_account_id(&session.bot_id);
accounts::save_account(&account_id, &AccountUpdate {
    token: Some(session.token),
    base_url: Some(session.base_url),
    user_id: Some(session.user_id),
})?;
accounts::register_account_id(&account_id)?;
println!("saved account {}", account_id);
# Ok(())
# }
```

### List and resolve accounts

```rust,no_run
# use openclaw_weixin::prelude::*;
# fn example() -> openclaw_weixin::Result<()> {
for id in openclaw_weixin::storage::accounts::list_indexed_account_ids() {
    let acc = openclaw_weixin::storage::accounts::resolve_account(&id)?;
    println!("{} enabled={} base={}", acc.account_id, acc.enabled, acc.base_url);
}
# Ok(())
# }
```

### Persist the long-poll cursor

```rust,no_run
# use openclaw_weixin::prelude::*;
# use openclaw_weixin::storage::sync_buf;
# fn example(buf: &str) -> std::io::Result<()> {
sync_buf::save_for_account("acct", buf)?;
if let Some(next) = sync_buf::load_for_account("acct") {
    println!("resume cursor len = {}", next.len());
}
# Ok(())
# }
```

### Read per-account route tag

```rust,no_run
# use openclaw_weixin::prelude::*;
# fn example() {
let tag = openclaw_weixin::storage::config::load_route_tag("acct");
println!("route tag: {:?}", tag);
# }
```

## Design notes and caveats

1. **The account directory is shared with the TS plugin.** The state root and the
   exact subpaths match TS, so Rust and TS clients on the same machine share
   credentials.
2. **`save_account`'s "empty does not overwrite" semantics.** To clear a field, follow
   the rules (`user_id=""` clears; `token`/`base_url` empty keeps old); this is to
   avoid accidentally wiping a logged-in token.
3. **`resolve_account` failure is a hard error.** Empty `account_id`, missing file,
   or missing token all return `Err`; `WeixinClientBuilder::build()` relies on it.
4. **Cursor has multiple fallback paths.** `load_get_updates_buf` tries the raw-id
   filename and the legacy single-account path, so an upgrade migration won't lose
   the cursor.
5. **Missing/corrupt config is not fatal.** `load_local_config*` returns
   `SdkConfig::default()` on a missing or corrupt file — not an error.
6. **Environment variables override the state dir.** `OPENCLAW_STATE_DIR` /
   `CLAWDBOT_STATE_DIR` win; an empty string is ignored and falls through.
7. **`botAgent` is sanitized.** An invalid UA string is cleaned before being written
   into `base_info` (see [api-client.md](api-client.md)); the value from config
   follows the same rule.
