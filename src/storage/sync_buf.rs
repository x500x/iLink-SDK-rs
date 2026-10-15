//! Persistent `get_updates_buf` cursor.
//!
//! Mirrors `src/storage/sync-buf.ts`. The long-poll cursor survives restarts
//! so a reconnect resumes instead of replaying the whole backlog.

use std::path::{Path, PathBuf};

use super::state_dir::accounts_dir;

/// Path of the persistent cursor file for an account.
pub fn get_sync_buf_path(account_id: &str) -> PathBuf {
    accounts_dir().join(format!("{account_id}.sync.json"))
}

/// Legacy single-account cursor path, for installs predating multi-account.
fn legacy_sync_buf_path() -> PathBuf {
    super::state_dir::resolve_state_dir()
        .join("agents")
        .join("default")
        .join("sessions")
        .join(".openclaw-weixin-sync")
        .join("default.json")
}

/// Stored file shape.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncBufData {
    pub get_updates_buf: String,
}

impl SyncBufData {
    pub fn new(get_updates_buf: impl Into<String>) -> Self {
        SyncBufData {
            get_updates_buf: get_updates_buf.into(),
        }
    }
}

fn read_file(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    parsed
        .get("get_updates_buf")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Load the persisted cursor.
///
/// Fallback order:
/// 1. the path given (normalized account id, new installs),
/// 2. the raw-id filename derived by [`super::accounts::derive_raw_account_id`],
/// 3. the legacy single-account file.
pub fn load_get_updates_buf(path: &Path) -> Option<String> {
    if let Some(value) = read_file(path) {
        return Some(value);
    }
    // `abc-im-bot.sync.json` stems to `abc-im-bot.sync`, so strip twice.
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_suffix(".sync"))
        .or_else(|| path.file_stem().and_then(|s| s.to_str()))?;
    if let Some(raw_id) = super::accounts::derive_raw_account_id(stem) {
        if let Some(value) = read_file(&accounts_dir().join(format!("{raw_id}.sync.json"))) {
            return Some(value);
        }
    }
    read_file(&legacy_sync_buf_path())
}

/// Load the cursor for an account id directly.
pub fn load_for_account(account_id: &str) -> Option<String> {
    load_get_updates_buf(&get_sync_buf_path(account_id))
}

/// Persist the cursor, creating parent directories as needed.
pub fn save_get_updates_buf(path: &Path, get_updates_buf: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(
        path,
        format!("{{\"get_updates_buf\":\"{}\"}}", escape(get_updates_buf)),
    )
}

/// Persist the cursor for an account id directly.
pub fn save_for_account(account_id: &str, get_updates_buf: &str) -> std::io::Result<()> {
    save_get_updates_buf(&get_sync_buf_path(account_id), get_updates_buf)
}

/// Remove the cursor for an account id.
pub fn clear_for_account(account_id: &str) {
    let _ = std::fs::remove_file(get_sync_buf_path(account_id));
}

/// Minimal JSON string escaping for the cursor value.
fn escape(raw: &str) -> String {
    serde_json::to_string(raw)
        .map(|s| s[1..s.len() - 1].to_string())
        .unwrap_or_default()
}

/// Serialises tests that mutate the process-global OPENCLAW_STATE_DIR.
#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        // Explicit paths only: the state dir is process-global and would
        // race with other tests that read it.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("acct.sync.json");
        save_get_updates_buf(&path, "abc123").unwrap();
        assert_eq!(load_get_updates_buf(&path).as_deref(), Some("abc123"));
    }

    #[test]
    fn handles_special_characters() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.sync.json");
        let value = "a\"b\\c\n\td";
        save_get_updates_buf(&path, value).unwrap();
        assert_eq!(load_get_updates_buf(&path).as_deref(), Some(value));
    }

    #[test]
    fn missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            load_get_updates_buf(&dir.path().join("nope.sync.json")),
            None
        );
    }

    #[test]
    fn malformed_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.sync.json");
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(load_get_updates_buf(&path), None);
    }

    #[test]
    fn raw_id_fallback_derivation() {
        // The compat lookup keys off the file stem minus the `.sync` suffix.
        let normalized = get_sync_buf_path("abc-im-bot");
        let stem = normalized
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_suffix(".sync"))
            .expect("stem should end with .sync");
        assert_eq!(stem, "abc-im-bot");
        assert_eq!(
            super::super::accounts::derive_raw_account_id(stem).as_deref(),
            Some("abc@im.bot")
        );
    }
}
