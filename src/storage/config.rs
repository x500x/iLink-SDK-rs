//! Local channel configuration.
//!
//! The TypeScript plugin reads `channels.openclaw-weixin` out of the host's
//! `openclaw.json`. The Rust SDK has no host config object, so it reads the
//! same section directly from disk. [`SdkConfig`] also lets callers inject an
//! in-memory override instead.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::state_dir::resolve_state_dir;

/// Config section that belongs to this channel.
pub const CHANNEL_CONFIG_SECTION: &str = "openclaw-weixin";

/// Resolve the path of the config file the host is expected to maintain.
///
/// `OPENCLAW_CONFIG` wins, then `<state_dir>/openclaw.json`.
pub fn resolve_config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("OPENCLAW_CONFIG") {
        let trimmed = path.to_string_lossy().trim().to_string();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    resolve_state_dir().join("openclaw.json")
}

/// Quote-cache policy, all values already normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteCachePolicy {
    pub enabled: bool,
    pub retention_ms: u64,
    pub max_messages_per_account: u64,
    pub media_retention_ms: u64,
    pub max_media_bytes_per_account: u64,
    pub max_single_media_bytes: u64,
}

impl Default for QuoteCachePolicy {
    fn default() -> Self {
        QuoteCachePolicy {
            enabled: true,
            retention_ms: 30 * 24 * 60 * 60 * 1000,
            max_messages_per_account: 10_000,
            media_retention_ms: 7 * 24 * 60 * 60 * 1000,
            max_media_bytes_per_account: 256 * 1024 * 1024,
            max_single_media_bytes: 25 * 1024 * 1024,
        }
    }
}

/// The resolved `channels.openclaw-weixin` section.
///
/// Field names are camelCase on the wire, matching `openclaw.json`'s schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdkConfig {
    /// Per-account overrides keyed by normalized account id.
    #[serde(default)]
    pub accounts: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    /// Whether structured tool-call progress messages are sent.
    #[serde(default = "default_true")]
    pub reply_progress_messages: bool,
    /// Local quote-cache policy.
    #[serde(default)]
    pub quote_cache: QuoteCacheRaw,
    /// Bumped on each successful login so the gateway reloads from disk.
    #[serde(default)]
    pub channel_config_updated_at: Option<String>,
    /// Optional route tag.
    #[serde(default)]
    pub route_tag: Option<serde_json::Value>,
    /// Optional observability identifier.
    #[serde(default)]
    pub bot_agent: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Raw quote-cache values as parsed from config; invalid values fall back
/// to defaults exactly like the TypeScript `positiveNumber` helper.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteCacheRaw {
    pub enabled: Option<bool>,
    pub retention_days: Option<serde_json::Value>,
    pub max_messages_per_account: Option<serde_json::Value>,
    pub media_retention_days: Option<serde_json::Value>,
    pub max_media_bytes_per_account: Option<serde_json::Value>,
    pub max_single_media_bytes: Option<serde_json::Value>,
}

impl QuoteCacheRaw {
    /// Normalize into a [`QuoteCachePolicy`].
    pub fn to_policy(&self) -> QuoteCachePolicy {
        let d = QuoteCachePolicy::default();
        QuoteCachePolicy {
            enabled: self.enabled.unwrap_or(d.enabled),
            retention_ms: positive_ms(self.retention_days.as_ref(), d.retention_ms),
            max_messages_per_account: positive_u64(
                self.max_messages_per_account.as_ref(),
                d.max_messages_per_account,
            ),
            media_retention_ms: positive_ms(
                self.media_retention_days.as_ref(),
                d.media_retention_ms,
            ),
            max_media_bytes_per_account: positive_u64(
                self.max_media_bytes_per_account.as_ref(),
                d.max_media_bytes_per_account,
            ),
            max_single_media_bytes: positive_u64(
                self.max_single_media_bytes.as_ref(),
                d.max_single_media_bytes,
            ),
        }
    }
}

fn positive_u64(value: Option<&serde_json::Value>, fallback: u64) -> u64 {
    value
        .and_then(|v| v.as_f64())
        .filter(|n| n.is_finite() && *n > 0.0)
        .map(|n| n.floor().max(1.0) as u64)
        .unwrap_or(fallback)
}

fn positive_ms(days: Option<&serde_json::Value>, fallback: u64) -> u64 {
    let days = positive_u64(days, fallback / (24 * 60 * 60 * 1000));
    days.saturating_mul(24 * 60 * 60 * 1000)
}

/// Read the `channels.openclaw-weixin` section from disk.
///
/// Missing or malformed config yields [`SdkConfig::default`].
pub fn load_local_config() -> SdkConfig {
    load_local_config_from(&resolve_config_path())
}

/// Read the channel section from a specific file.
pub fn load_local_config_from(path: &std::path::Path) -> SdkConfig {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return SdkConfig::default();
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return SdkConfig::default();
    };
    parsed
        .get("channels")
        .and_then(|c| c.get(CHANNEL_CONFIG_SECTION))
        .and_then(|s| serde_json::from_value::<SdkConfig>(s.clone()).ok())
        .unwrap_or_default()
}

/// Merge an in-memory config over the one on disk.
pub fn resolve_channel_config(overrides: Option<SdkConfig>) -> SdkConfig {
    match overrides {
        None => load_local_config(),
        Some(over) => over,
    }
}

/// Cached section lookup for headers that need a route tag without a full
/// config object (mirrors `loadConfigRouteTag`).
pub fn load_route_tag(account_id: &str) -> Option<String> {
    let path = resolve_config_path();
    let Ok(raw) = std::fs::read_to_string(path) else {
        return None;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return None;
    };
    let section = parsed
        .get("channels")
        .and_then(|c| c.get(CHANNEL_CONFIG_SECTION))?;

    if !account_id.trim().is_empty() {
        if let Some(tag) = section
            .get("accounts")
            .and_then(|a| a.get(account_id))
            .and_then(|a| a.get("routeTag"))
        {
            return value_to_route_tag(tag);
        }
    }
    value_to_route_tag(section.get("routeTag")?)
}

/// Read `botAgent` from the channel section, if present.
pub fn load_bot_agent() -> Option<String> {
    let path = resolve_config_path();
    let raw = std::fs::read_to_string(path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    parsed
        .get("channels")?
        .get(CHANNEL_CONFIG_SECTION)?
        .get("botAgent")?
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

fn value_to_route_tag(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Number(n) => n.as_i64().map(|i| i.to_string()),
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let p = QuoteCacheRaw::default().to_policy();
        assert!(p.enabled);
        assert_eq!(p.max_messages_per_account, 10_000);
        assert_eq!(p.retention_ms, 30 * 24 * 60 * 60 * 1000);
        assert_eq!(p.max_single_media_bytes, 25 * 1024 * 1024);
    }

    #[test]
    fn invalid_values_fall_back() {
        let raw = QuoteCacheRaw {
            enabled: Some(false),
            retention_days: Some(serde_json::json!(-5)),
            max_messages_per_account: Some(serde_json::json!("x")),
            media_retention_days: Some(serde_json::json!(1.5)),
            max_media_bytes_per_account: Some(serde_json::json!(1024)),
            max_single_media_bytes: Some(serde_json::json!(2048)),
        };
        let p = raw.to_policy();
        assert!(!p.enabled);
        assert_eq!(p.retention_ms, 30 * 24 * 60 * 60 * 1000);
        assert_eq!(p.max_messages_per_account, 10_000);
        assert_eq!(p.media_retention_ms, 24 * 60 * 60 * 1000);
        assert_eq!(p.max_media_bytes_per_account, 1024);
        assert_eq!(p.max_single_media_bytes, 2048);
    }

    #[test]
    fn parses_section_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("openclaw.json");
        std::fs::write(
            &path,
            r#"{"channels":{"openclaw-weixin":{"botAgent":"Bot/1.0","routeTag":"r1","replyProgressMessages":false}}}"#,
        )
        .unwrap();
        let cfg = load_local_config_from(&path);
        assert_eq!(cfg.bot_agent.as_deref(), Some("Bot/1.0"));
        assert_eq!(cfg.route_tag, Some(serde_json::json!("r1")));
        // The wire field is camelCase; replyProgressMessages: false deserializes.
        assert!(!cfg.reply_progress_messages);
    }

    #[test]
    fn missing_file_yields_defaults() {
        let cfg = load_local_config_from(std::path::Path::new("/nonexistent/x.json"));
        assert_eq!(cfg, SdkConfig::default());
    }

    #[test]
    fn malformed_file_yields_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(load_local_config_from(&path), SdkConfig::default());
    }
}
