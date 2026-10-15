//! High-level client that composes the API client, account store, QR login,
//! media pipeline and quote cache into a single ergonomic handle.

use std::sync::Arc;

use crate::api::{WeixinApiClient, WeixinApiOptions};
use crate::error::Result;
use crate::messaging::auth::{QrLogin, QrSession};
use crate::messaging::outbound::MessageSender;
use crate::messaging::quote_cache::QuoteStore;
use crate::storage::accounts::{self, AccountData, AccountUpdate, StoredAccount};

/// Configuration for [`WeixinClient`].
#[derive(Debug, Clone, Default)]
pub struct WeixinConfig {
    /// Override the account id to operate on.
    ///
    /// When `None`, [`WeixinClient::load_account`] falls back to the single
    /// registered account, erroring when zero or several are present.
    pub account_id: Option<String>,
    /// Override the API base URL.
    pub base_url: Option<String>,
    /// Override the CDN base URL.
    pub cdn_base_url: Option<String>,
    /// Route tag forwarded as `SKRouteTag`.
    pub route_tag: Option<String>,
    /// Optional observability identifier sent as `bot_agent`.
    pub bot_agent: Option<String>,
    /// Per-request timeout override in milliseconds.
    pub timeout_ms: Option<u64>,
}

/// Builder for [`WeixinClient`].
#[derive(Debug, Default, Clone)]
pub struct WeixinClientBuilder {
    config: WeixinConfig,
}

impl WeixinClientBuilder {
    pub fn new() -> Self {
        WeixinClientBuilder::default()
    }

    /// Operate on a specific account id.
    pub fn account_id(mut self, id: impl Into<String>) -> Self {
        self.config.account_id = Some(id.into());
        self
    }

    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.config.base_url = Some(url.into());
        self
    }

    pub fn cdn_base_url(mut self, url: impl Into<String>) -> Self {
        self.config.cdn_base_url = Some(url.into());
        self
    }

    pub fn route_tag(mut self, tag: impl Into<String>) -> Self {
        self.config.route_tag = Some(tag.into());
        self
    }

    pub fn bot_agent(mut self, agent: impl Into<String>) -> Self {
        self.config.bot_agent = Some(agent.into());
        self
    }

    pub fn timeout_ms(mut self, ms: u64) -> Self {
        self.config.timeout_ms = Some(ms);
        self
    }

    /// Operate on a specific account id, if provided.
    pub fn account_id_opt(mut self, id: Option<String>) -> Self {
        self.config.account_id = id;
        self
    }

    pub fn base_url_opt(mut self, url: Option<String>) -> Self {
        self.config.base_url = url;
        self
    }

    pub fn cdn_base_url_opt(mut self, url: Option<String>) -> Self {
        self.config.cdn_base_url = url;
        self
    }
    /// Build the client, resolving credentials from the local account store.
    ///
    /// Fails when the account is unknown or has no stored token.
    pub async fn build(self) -> Result<WeixinClient> {
        WeixinClient::from_config(self.config).await
    }
}

struct Inner {
    account: StoredAccount,
    api: WeixinApiClient,
}

/// A single logged-in Weixin bot account.
#[derive(Clone)]
pub struct WeixinClient {
    inner: Arc<Inner>,
    //  is Send but not Sync, so this Arc is Send-only.
    // That is fine: the client is clone-shared, never shared behind &self.
    quote_store_opt: Option<Arc<QuoteStore>>,
}

impl WeixinClient {
    /// Construct from an explicit configuration.
    pub async fn from_config(config: WeixinConfig) -> Result<Self> {
        let account_id = config
            .account_id
            .clone()
            .or_else(|| accounts::list_indexed_account_ids().into_iter().next())
            .ok_or_else(|| {
                crate::error::Error::new(
                    crate::error::ErrorKind::Unknown,
                    "no accounts registered — run the QR-code login flow first",
                )
            })?;

        let mut account = accounts::resolve_account(&account_id)?;

        if let Some(base) = config.base_url.as_ref().filter(|u| !u.trim().is_empty()) {
            account.base_url = base.trim().to_string();
        }
        if let Some(cdn) = config
            .cdn_base_url
            .as_ref()
            .filter(|u| !u.trim().is_empty())
        {
            account.cdn_base_url = cdn.trim().to_string();
        }

        let token = account
            .token
            .clone()
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| {
                crate::error::Error::new(
                    crate::error::ErrorKind::Unknown,
                    format!("account {account_id} has no bot token — run the QR-code login flow"),
                )
            })?;

        let mut opts = WeixinApiOptions::new(account.base_url.clone()).with_token(token);
        if let Some(tag) = config.route_tag.as_ref().or(account.route_tag.as_ref()) {
            opts = opts.with_route_tag(tag.clone());
        }
        opts.cdn_base_url = Some(account.cdn_base_url.clone());
        if let Some(ms) = config.timeout_ms {
            opts = opts.with_timeout_ms(ms);
        }

        let mut api = WeixinApiClient::new(opts)?;
        if let Some(agent) = config.bot_agent.as_ref() {
            api = api.with_bot_agent(agent);
        }

        Ok(WeixinClient {
            inner: Arc::new(Inner { account, api }),
            quote_store_opt: None,
        })
    }

    /// Construct directly from a [`QrSession`], without touching the account
    /// store. Useful for bespoke persistence strategies.
    pub async fn from_session(session: QrSession, bot_agent: Option<String>) -> Result<Self> {
        let mut opts =
            WeixinApiOptions::new(session.base_url.clone()).with_token(session.token.clone());
        opts.cdn_base_url = Some(crate::storage::accounts::CDN_BASE_URL.to_string());
        let mut api = WeixinApiClient::new(opts)?;
        if let Some(agent) = bot_agent {
            api = api.with_bot_agent(agent);
        }
        let account = StoredAccount {
            account_id: session.bot_id.clone(),
            base_url: session.base_url,
            cdn_base_url: crate::storage::accounts::CDN_BASE_URL.to_string(),
            token: Some(session.token),
            enabled: true,
            configured: true,
            name: None,
            user_id: Some(session.user_id),
            route_tag: None,
        };
        Ok(WeixinClient {
            inner: Arc::new(Inner { account, api }),
            quote_store_opt: None,
        })
    }

    /// Persist credentials for `account_id` and return a client for it.
    pub async fn save_and_load(account_id: &str, update: AccountUpdate) -> Result<Self> {
        let data = accounts::save_account(account_id, &update)?;
        accounts::register_account_id(account_id)?;
        Self::from_account_data(account_id, data).await
    }

    async fn from_account_data(account_id: &str, _data: AccountData) -> Result<Self> {
        Self::from_config(WeixinConfig {
            account_id: Some(account_id.to_string()),
            ..Default::default()
        })
        .await
    }

    /// Resolved account information.
    pub fn account(&self) -> &StoredAccount {
        &self.inner.account
    }

    /// Effective account id.
    pub fn account_id(&self) -> &str {
        &self.inner.account.account_id
    }

    /// The bot token in use, if any.
    pub fn token(&self) -> Option<&str> {
        self.inner.account.token.as_deref()
    }

    /// Underlying API client (exposed for advanced callers).
    pub fn api(&self) -> &WeixinApiClient {
        &self.inner.api
    }

    /// QR-login builder bound to this account's client.
    pub fn qr_login(&self) -> QrLogin<'_> {
        QrLogin::new(&self.inner.api)
    }

    /// Outbound message helpers.
    pub fn sender(&self) -> MessageSender<'_> {
        MessageSender::new(&self.inner.api, self.account_id().to_string())
    }

    /// Enable the local quote cache so ID-only inbound quotes resolve.
    pub fn enable_quote_cache(&mut self) -> Result<()> {
        if self.quote_store_opt.is_none() {
            // SAFETY-equivalent: the store is only reachable through
            // , which hands out an immutable reference used
            // from a single task at a time in practice.
            // QuoteStore is Send but not Sync (rusqlite::Connection uses
            // RefCell internally). Clippy flags the Arc; that is expected
            // here because the store is never shared behind .
            #[allow(clippy::arc_with_non_send_sync)]
            let store = Arc::new(QuoteStore::open_default()?);
            self.quote_store_opt = Some(store);
        }
        Ok(())
    }

    /// The quote cache, when enabled.
    pub fn quote_store(&self) -> Option<&Arc<QuoteStore>> {
        self.quote_store_opt.as_ref()
    }

    /// Notify the backend this channel client is starting.
    pub async fn notify_start(&self) -> Result<()> {
        self.inner.api.notify_start().await.map(|_| ())
    }

    /// Notify the backend this channel client is stopping.
    pub async fn notify_stop(&self) -> Result<()> {
        self.inner.api.notify_stop().await.map(|_| ())
    }
}

impl std::fmt::Debug for WeixinClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeixinClient")
            .field("account_id", &self.account_id())
            .field("base_url", &self.inner.account.base_url)
            .field("configured", &self.inner.account.configured)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_defaults() {
        let b = WeixinClientBuilder::new();
        assert!(b.config.account_id.is_none());
        assert!(b.config.base_url.is_none());
    }

    #[test]
    fn builder_sets_fields() {
        let b = WeixinClientBuilder::new()
            .account_id("acct")
            .base_url("https://x")
            .cdn_base_url("https://c")
            .route_tag("r")
            .bot_agent("Bot/1")
            .timeout_ms(1234);
        assert_eq!(b.config.account_id.as_deref(), Some("acct"));
        assert_eq!(b.config.base_url.as_deref(), Some("https://x"));
        assert_eq!(b.config.cdn_base_url.as_deref(), Some("https://c"));
        assert_eq!(b.config.route_tag.as_deref(), Some("r"));
        assert_eq!(b.config.bot_agent.as_deref(), Some("Bot/1"));
        assert_eq!(b.config.timeout_ms, Some(1234));
    }

    #[test]
    fn config_default_has_no_account() {
        let c = WeixinConfig::default();
        assert!(c.account_id.is_none());
    }
}
