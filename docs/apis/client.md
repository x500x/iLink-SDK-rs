# High-level client API

> 中文版: [中文文档](../zh/apis/client.md)

[`WeixinClient`](../../src/client/mod.rs) is the batteries-included entry point for a
logged-in account. It wraps account resolution, the low-level
[`WeixinApiClient`](api-client.md), QR login, message delivery, typing heartbeats,
and the local quote cache. It is the same surface the `weixin` CLI uses in
`listen` mode.

> `WeixinClient` is `Clone`, but **not** `Sync`. Clone it to hand off to worker
> tasks; do not share a single instance across `await` points on different threads
> without an `Arc`. Internally it owns a `WeixinApiClient` (which is `Sync`) plus a
> non-`Sync` `QuoteStore`.

## `WeixinConfig`

```rust
#[derive(Debug, Clone, Default)]
pub struct WeixinConfig {
    pub account_id: String,
    pub base_url: String,
    pub cdn_base_url: String,
    pub route_tag: Option<String>,
    pub bot_agent: Option<String>,
    pub api_timeout_ms: u64,
    pub quote_cache: storage::config::QuoteCachePolicy,
}
```

`WeixinConfig` is the fully-resolved configuration for a client. The builder
populates it; `WeixinClient` itself is cheaply cloneable and reads from it.

## `WeixinClientBuilder`

```rust
impl WeixinClientBuilder {
    pub fn new() -> Self;

    pub fn account_id(mut self, account_id: &str) -> Self;       // required (or from_session)
    pub fn base_url(mut self, url: &str) -> Self;                // override account base_url
    pub fn cdn_base_url(mut self, url: &str) -> Self;
    pub fn route_tag(mut self, tag: &str) -> Self;
    pub fn bot_agent(mut self, agent: &str) -> Self;
    pub fn timeout_ms(mut self, ms: u64) -> Self;

    /// Build from a resolved account_id (loads token/base_url from storage).
    pub async fn build(self) -> Result<WeixinClient>;

    /// Build fully offline from an explicitly-supplied session (QR login result).
    pub fn from_session(session: QrSession) -> Result<WeixinClient>;
}
```

- `account_id` may be either the `normalize_account_id` form (`b0f5860fdecb-im-bot`)
  or the raw `@im.bot` form; the builder resolves both via
  `storage::accounts::resolve_account`.
- `base_url` / `cdn_base_url` / `route_tag` / `bot_agent` override the stored
  account values.
- `build()` loads token + base URL from storage; `from_session()` constructs a
  client without touching persistent storage (handy right after QR login).

## `WeixinClient`

```rust
impl WeixinClient {
    /// Load the persisted client + credentials. Convenience over
    /// WeixinClientBuilder::new().account_id(id).build().await.
    pub async fn save_and_load(account_id: &str) -> Result<Self>;

    pub fn account(&self) -> &StoredAccount;
    pub fn account_id(&self) -> &str;
    pub fn token(&self) -> &str;
    pub fn api(&self) -> &WeixinApiClient;
    pub fn qr_login(&self) -> &QrLogin;
    pub fn sender(&self) -> &MessageSender;

    pub fn enable_quote_cache(&self) -> bool;
    pub fn quote_store(&self) -> Option<&QuoteStore>;

    /// Lifetime heartbeats (keep-alive for the backend session).
    pub async fn notify_start(&self) -> Result<NotifyStartResp>;
    pub async fn notify_stop(&self) -> Result<NotifyStopResp>;
}
```

`save_and_load` is the typical entry point after login: it reads the stored
`AccountData`, builds a `WeixinApiClient` with the token, and returns a ready
`WeixinClient`.

### Quote cache enablement

`enable_quote_cache()` reflects the resolved `quoteCache` config policy:
- `false` when `policy.enabled == false` or `maxMessagesPerAccount == 0` or
  `maxMediaBytesPerAccount == 0`
- `true` otherwise

When enabled, `quote_store()` returns `Some(&QuoteStore)`. Enable it explicitly
after construction if you want replies to carry proper `context_token` references.

## Usage examples

### Build from a stored account (listen mode)

```rust,no_run
use ilink_sdk::prelude::*;

# async fn example() -> ilink_sdk::Result<()> {
let client = WeixinClient::save_and_load("b0f5860fdecb-im-bot").await?;
println!("account {} token len {}", client.account_id(), client.token().len());
client.notify_start().await?;
# Ok(())
# }
```

### Build immediately after QR login (no persistence needed)

```rust,no_run
use ilink_sdk::prelude::*;

# fn example(session: QrSession) -> ilink_sdk::Result<()> {
// session is the result of QrLogin::wait
let client = WeixinClientBuilder::from_session(session)?;
let _ = client.sender().send_text("someone@im.wechat", "hi", &SendOpts::default());
# Ok(())
# }
```

### Override route tag / base URL

```rust,no_run
use ilink_sdk::prelude::*;

# async fn example() -> ilink_sdk::Result<()> {
let client = WeixinClientBuilder::new()
    .account_id("b0f5860fdecb-im-bot")
    .base_url("https://ilinkai.weixin.qq.com")
    .route_tag("region-a")
    .timeout_ms(20_000)
    .build().await?;
# Ok(())
# }
```

## Design notes and caveats

1. **`WeixinClient` is not `Sync`.** Clone it (`Clone` is cheap) before moving into
   concurrent tasks. The CLI clones a client per connection. Do not wrap the same
   instance in `Arc` and send across `await` points on distinct threads directly.
2. **`from_session` is offline.** It will not read or write credentials; use it
   only for the brief window right after QR login, then persist separately (see
   [storage.md](storage.md)).
3. **`save_and_load` fails if no token.** `resolve_account` returns an error when
   the token is missing, so `save_and_load` propagates it. Ensure you saved the
   account during login.
4. **`notify_start` / `notify_stop` are heartbeats.** Call `notify_start` on
   listen start and `notify_stop` on shutdown so the backend knows the session is
   alive/dead. Skipping them does not break messaging but is not recommended.
5. **Quote cache defaults to enabled.** If `quoteCache.enabled` is unset in config,
   the store is enabled with default policy: call `quote_store()` to get it.
