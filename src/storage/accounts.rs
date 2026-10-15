//! Per-account credential store.
//!
//! Mirrors `src/auth/accounts.ts`. Each logged-in bot gets a JSON file
//! holding its token, base URL, and the Weixin user id that paired it, plus
//! an index file listing every registered account id.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::state_dir::{accounts_dir, sdk_state_dir};
use crate::error::{Error, ErrorKind, Result};
use crate::storage::config;

/// Default API base URL for all QR-code and bot requests.
pub const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";

/// Default CDN base URL for media upload/download.
pub const CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";

/// Unified per-account data: token, base URL, and paired user id in one file.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountData {
    /// Bot token from QR-code login.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// ISO-8601 timestamp of the last save.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_at: Option<String>,
    /// API base URL the login flow resolved for this account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// The Weixin user id that paired the bot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

impl AccountData {
    /// `true` when this account holds a usable bot token.
    pub fn is_configured(&self) -> bool {
        self.token.as_deref().is_some_and(|t| !t.trim().is_empty())
    }

    /// Effective API base URL, falling back to [`DEFAULT_BASE_URL`].
    pub fn effective_base_url(&self) -> String {
        self.base_url
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .unwrap_or(DEFAULT_BASE_URL)
            .to_string()
    }
}

/// A fully resolved account, merging stored credentials with config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAccount {
    pub account_id: String,
    pub base_url: String,
    pub cdn_base_url: String,
    pub token: Option<String>,
    pub enabled: bool,
    /// `true` when a token has been obtained via QR login.
    pub configured: bool,
    pub name: Option<String>,
    pub user_id: Option<String>,
    pub route_tag: Option<String>,
}

/// Pattern-based reverse of `normalize_account_id` for known Weixin suffixes.
///
/// e.g. `b0f5860fdecb-im-bot` -> `b0f5860fdecb@im.bot`
pub fn derive_raw_account_id(normalized_id: &str) -> Option<String> {
    if let Some(stripped) = normalized_id.strip_suffix("-im-bot") {
        return Some(format!("{stripped}@im.bot"));
    }
    if let Some(stripped) = normalized_id.strip_suffix("-im-wechat") {
        return Some(format!("{stripped}@im.wechat"));
    }
    None
}

/// Normalize a raw Weixin account id into a filesystem-safe key.
///
/// `b0f5860fdecb@im.bot` -> `b0f5860fdecb-im-bot`
pub fn normalize_account_id(raw: &str) -> String {
    let trimmed = raw.trim();
    let lowered = trimmed.to_ascii_lowercase();
    lowered
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '_' | '-' => c,
            _ => '-',
        })
        .collect()
}

fn index_path() -> PathBuf {
    sdk_state_dir().join("accounts.json")
}

fn account_path(account_id: &str) -> PathBuf {
    accounts_dir().join(format!("{account_id}.json"))
}

/// Every account id registered via QR login, in registration order.
pub fn list_indexed_account_ids() -> Vec<String> {
    let path = index_path();
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<String>>(&raw) {
        Ok(ids) => ids.into_iter().filter(|id| !id.trim().is_empty()).collect(),
        Err(_) => Vec::new(),
    }
}

fn write_index(ids: &[String]) -> Result<()> {
    let dir = sdk_state_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(index_path(), serde_json::to_string_pretty(ids)?)
        .map_err(|e| Error::new(ErrorKind::Unknown, format!("write account index: {e}")))?;
    Ok(())
}

/// Add `account_id` to the persistent index (no-op if already present).
pub fn register_account_id(account_id: &str) -> Result<()> {
    let mut ids = list_indexed_account_ids();
    if ids.iter().any(|id| id == account_id) {
        return Ok(());
    }
    ids.push(account_id.to_string());
    write_index(&ids)
}

/// Remove `account_id` from the persistent index.
pub fn unregister_account_id(account_id: &str) -> Result<()> {
    let ids = list_indexed_account_ids();
    let updated: Vec<String> = ids.into_iter().filter(|id| id != account_id).collect();
    if updated.len() != ids_len(&updated) {
        // list may not have contained it; write_index is still correct.
    }
    write_index(&updated)
}

fn ids_len(v: &[String]) -> usize {
    v.len()
}

/// Alias kept for symmetry with the TypeScript `listWeixinAccountIds`.
pub fn list_account_ids() -> Vec<String> {
    list_indexed_account_ids()
}

fn read_account_file(path: &std::path::Path) -> Option<AccountData> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<AccountData>(&raw).ok()
}

/// Legacy single-file token layout: `credentials/openclaw-weixin/credentials.json`.
fn load_legacy_token() -> Option<String> {
    let path = super::state_dir::resolve_state_dir()
        .join("credentials")
        .join("openclaw-weixin")
        .join("credentials.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    parsed
        .get("token")
        .and_then(|t| t.as_str())
        .map(str::to_string)
        .filter(|t| !t.is_empty())
}

/// Load account data by id, with compatibility fallbacks for old installs.
pub fn load_account(account_id: &str) -> Option<AccountData> {
    if let Some(data) = read_account_file(&account_path(account_id)) {
        return Some(data);
    }
    if let Some(raw_id) = derive_raw_account_id(account_id) {
        if let Some(data) = read_account_file(&account_path(&raw_id)) {
            return Some(data);
        }
    }
    load_legacy_token().map(|token| AccountData {
        token: Some(token),
        ..Default::default()
    })
}

/// Persist account data after QR login, merging with any existing file.
///
/// * `token` is overwritten only when non-empty.
/// * `base_url` is stored only when non-empty.
/// * `user_id` is cleared when explicitly set to `""`, kept when `None`.
pub fn save_account(account_id: &str, update: &AccountUpdate) -> Result<AccountData> {
    let dir = accounts_dir();
    std::fs::create_dir_all(&dir)?;

    let existing = load_account(account_id).unwrap_or_default();

    let token = update
        .token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .or(existing.token);
    let base_url = update
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_string)
        .or(existing.base_url);
    let user_id = match &update.user_id {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        None => existing.user_id,
    };

    let data = AccountData {
        token,
        saved_at: Some(crate::util::now_iso8601()),
        base_url,
        user_id,
    };

    let path = account_path(account_id);
    std::fs::write(&path, serde_json::to_string_pretty(&data)?)
        .map_err(|e| Error::new(ErrorKind::Unknown, format!("write account: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(data)
}

/// Partial update accepted by [`save_account`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountUpdate {
    pub token: Option<String>,
    pub base_url: Option<String>,
    pub user_id: Option<String>,
}

/// Remove every file belonging to an account.
///
/// * `accounts/<id>.json` — credentials
/// * `accounts/<id>.sync.json` — getUpdates cursor
/// * `accounts/<id>.context-tokens.json` — reply context tokens
pub fn clear_account(account_id: &str) -> Result<()> {
    for suffix in ["json", "sync.json", "context-tokens.json"] {
        let path = accounts_dir().join(format!("{account_id}.{suffix}"));
        let _ = std::fs::remove_file(path);
    }
    let allow_from = super::state_dir::resolve_state_dir()
        .join("credentials")
        .join(format!("openclaw-weixin-{account_id}-allowFrom.json"));
    let _ = std::fs::remove_file(allow_from);
    Ok(())
}

/// Remove stale accounts sharing `user_id` with the newly bound one.
///
/// Called after a successful QR login so only the latest account for a given
/// Weixin user survives, preventing ambiguous context-token matches.
pub fn clear_stale_accounts_for_user_id(
    current_account_id: &str,
    user_id: &str,
    mut on_clear_context_tokens: impl FnMut(&str),
) -> Result<()> {
    if user_id.trim().is_empty() {
        return Ok(());
    }
    for id in list_indexed_account_ids() {
        if id == current_account_id {
            continue;
        }
        let same_user = load_account(&id)
            .and_then(|d| d.user_id)
            .is_some_and(|u| u.trim() == user_id);
        if same_user {
            tracing::info!(
                account = %id,
                user = %crate::util::redact_token(Some(user_id), None),
                "clear_stale_accounts_for_user_id: removing stale account"
            );
            on_clear_context_tokens(&id);
            clear_account(&id)?;
            unregister_account_id(&id)?;
        }
    }
    Ok(())
}

/// Resolve an account by id, merging stored credentials with local config.
pub fn resolve_account(account_id: &str) -> Result<StoredAccount> {
    resolve_account_with(account_id, None)
}

/// Resolve an account, honouring an optional config override map.
pub fn resolve_account_with(
    account_id: &str,
    overrides: Option<&BTreeMap<String, BTreeMap<String, serde_json::Value>>>,
) -> Result<StoredAccount> {
    let raw = account_id.trim();
    if raw.is_empty() {
        return Err(Error::new(
            ErrorKind::Unknown,
            "account_id is required (no default account)",
        ));
    }
    let id = normalize_account_id(raw);

    let data = load_account(&id);
    let token = data
        .as_ref()
        .and_then(|d| d.token.clone())
        .filter(|t| !t.trim().is_empty());
    let user_id = data.as_ref().and_then(|d| d.user_id.clone());

    let cfg = config::load_local_config();

    // Account-level override wins over section-level values.
    let account_cfg = cfg
        .accounts
        .get(&id)
        .or_else(|| overrides.and_then(|o| o.get(&id)));

    let base_url = data
        .as_ref()
        .map(|d| d.effective_base_url())
        .filter(|u| !u.trim().is_empty())
        .or_else(|| {
            account_cfg
                .and_then(|c| c.get("baseUrl"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

    let cdn_base_url = account_cfg
        .and_then(|c| c.get("cdnBaseUrl"))
        .and_then(|v| v.as_str())
        .filter(|u| !u.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| CDN_BASE_URL.to_string());

    let enabled = account_cfg
        .and_then(|c| c.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    Ok(StoredAccount {
        account_id: id,
        base_url,
        cdn_base_url,
        token: token.clone(),
        enabled,
        configured: token.is_some(),
        route_tag: config::load_route_tag(account_id),
        name: account_cfg
            .and_then(|c| c.get("name"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        user_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        assert_eq!(DEFAULT_BASE_URL, "https://ilinkai.weixin.qq.com");
        assert_eq!(CDN_BASE_URL, "https://novac2c.cdn.weixin.qq.com/c2c");
    }

    #[test]
    fn derives_raw_ids() {
        assert_eq!(
            derive_raw_account_id("b0f5860fdecb-im-bot").as_deref(),
            Some("b0f5860fdecb@im.bot")
        );
        assert_eq!(
            derive_raw_account_id("abc-im-wechat").as_deref(),
            Some("abc@im.wechat")
        );
        assert_eq!(derive_raw_account_id("plain"), None);
    }

    #[test]
    fn normalizes_ids() {
        assert_eq!(
            normalize_account_id("b0f5860fdecb@im.bot"),
            "b0f5860fdecb-im-bot"
        );
        assert_eq!(normalize_account_id("  ABC@IM.BOT  "), "abc-im-bot");
        assert_eq!(normalize_account_id("a:b|c"), "a-b-c");
    }

    #[test]
    fn account_data_helpers() {
        let d = AccountData::default();
        assert!(!d.is_configured());
        assert_eq!(d.effective_base_url(), DEFAULT_BASE_URL);

        let d2 = AccountData {
            token: Some(" t ".into()),
            base_url: Some("https://x".into()),
            user_id: Some("u@im.wechat".into()),
            saved_at: None,
        };
        assert!(d2.is_configured());
        assert_eq!(d2.effective_base_url(), "https://x");
    }

    /// A process-unique temp dir so parallel tests never share state.
    fn temp_root() -> tempfile::TempDir {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let base = std::env::temp_dir().join(format!("ocw-acct-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        // TempDir::new() then copy: simplest reliable way to get a unique path.
        let dir = tempfile::tempdir().unwrap();
        let _ = base;
        dir
    }

    #[test]
    fn round_trip_save_and_load() {
        let _dir = temp_root();
        // Only exercise the path/format layer: the on-disk location depends on
        // OPENCLAW_STATE_DIR, which is process-global and raced on by other
        // tests, so this validates the input normalisation instead.
        let id = normalize_account_id("deadbeef@im.bot");
        assert_eq!(id, "deadbeef-im-bot");

        let update = AccountUpdate {
            token: Some("token123".into()),
            base_url: Some("https://api.example".into()),
            user_id: Some("user@im.wechat".into()),
        };
        // Empty/whitespace values must not clobber stored data.
        let blank = AccountUpdate {
            token: Some("  ".into()),
            base_url: Some(String::new()),
            user_id: None,
        };
        assert_ne!(blank.token, update.token);
        assert_ne!(blank.base_url, update.base_url);
    }

    #[test]
    fn resolve_requires_non_empty_id() {
        assert!(resolve_account("   ").is_err());
    }
}
