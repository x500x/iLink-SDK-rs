//! Local quote cache.
//!
//! Ports `src/messaging/quote-store.ts`. Newer Weixin clients send only a
//! server message id for a quoted message, so the required text and media
//! metadata are stored locally to let later quotes restore their context.
//!
//! The database lives at `<state_dir>/openclaw-weixin/ref-messages.sqlite`;
//! managed media is stored under `<state_dir>/media/inbound/openclaw-weixin-quotes/<account>/`.
//!
//! Disabled entirely when [`QuoteCachePolicy::enabled`] is `false`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OpenFlags};

use crate::error::{Error, Result};
use crate::storage::state_dir::{resolve_state_dir, sdk_state_dir};

/// Default retention for text records, 30 days.
pub const DEFAULT_RETENTION_DAYS: u64 = 30;
/// Default per-account message cap.
pub const DEFAULT_MAX_MESSAGES_PER_ACCOUNT: u64 = 10_000;
/// Default media retention, 7 days.
pub const DEFAULT_MEDIA_RETENTION_DAYS: u64 = 7;
/// Default per-account media budget, 256 MiB.
pub const DEFAULT_MAX_MEDIA_BYTES_PER_ACCOUNT: u64 = 256 * 1024 * 1024;
/// Default single-file media cap, 25 MiB.
pub const DEFAULT_MAX_SINGLE_MEDIA_BYTES: u64 = 25 * 1024 * 1024;

/// Media-store subdirectory owned and garbage-collected by this SDK.
pub const QUOTE_MEDIA_SUBDIR: &str = "media/inbound/openclaw-weixin-quotes";

/// Run GC after this many writes, in addition to the hourly timer.
const GC_WRITE_INTERVAL: u64 = 100;

/// Normalized quote-cache policy.
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
            retention_ms: DEFAULT_RETENTION_DAYS * 24 * 60 * 60 * 1000,
            max_messages_per_account: DEFAULT_MAX_MESSAGES_PER_ACCOUNT,
            media_retention_ms: DEFAULT_MEDIA_RETENTION_DAYS * 24 * 60 * 60 * 1000,
            max_media_bytes_per_account: DEFAULT_MAX_MEDIA_BYTES_PER_ACCOUNT,
            max_single_media_bytes: DEFAULT_MAX_SINGLE_MEDIA_BYTES,
        }
    }
}

/// One cached message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteMessageRecord {
    pub account_id: String,
    pub conversation_id: String,
    pub message_id: String,
    pub direction: Direction,
    pub body: String,
    pub media_path: Option<String>,
    pub media_mime: Option<String>,
    pub media_name: Option<String>,
    pub media_size: Option<i64>,
    pub created_at: i64,
}

/// Inbound vs outbound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Inbound,
    Outbound,
}

impl Direction {
    fn as_str(self) -> &'static str {
        match self {
            Direction::Inbound => "inbound",
            Direction::Outbound => "outbound",
        }
    }

    fn from_str(raw: &str) -> Self {
        match raw {
            "outbound" => Direction::Outbound,
            _ => Direction::Inbound,
        }
    }
}

/// Input accepted by [`QuoteStore::put`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuoteMessageInput {
    pub account_id: String,
    pub conversation_id: String,
    pub message_id: String,
    pub direction: Direction,
    pub body: String,
    /// Source media path, registered into the managed root on write.
    pub source_media_path: Option<String>,
    pub media_mime: Option<String>,
    pub media_name: Option<String>,
    pub created_at: i64,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Sanitize a string into a safe path segment.
fn safe_path_segment(raw: &str) -> String {
    let mut safe: String = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();

    // Collapse dot runs so `..`, `...` and friends cannot escape the directory.
    while safe.contains("..") {
        safe = safe.replace("..", "_");
    }
    // A lone `.` or a run of underscores is not a usable segment either.
    let degenerate = safe.is_empty() || safe == "." || safe.chars().all(|c| c == '_');
    if degenerate {
        "default".to_string()
    } else {
        safe
    }
}

/// Directory name for one account's managed media.
pub fn account_media_dir_name(account_id: &str) -> String {
    let readable: String = safe_path_segment(account_id).chars().take(48).collect();
    let digest = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        account_id.hash(&mut h);
        format!("{:016x}", h.finish())
    };
    format!("{readable}-{digest}")
}

/// Media-store subdirectory used for one account.
pub fn quote_media_subdir(account_id: &str) -> String {
    format!(
        "{QUOTE_MEDIA_SUBDIR}/{}",
        account_media_dir_name(account_id)
    )
}

fn default_media_root() -> PathBuf {
    resolve_state_dir().join(QUOTE_MEDIA_SUBDIR)
}

/// The SQLite-backed quote cache.
pub struct QuoteStore {
    conn: Connection,
    media_root: PathBuf,
    policy: QuoteCachePolicy,
    writes_since_gc: u64,
}

impl QuoteStore {
    /// Open (creating if needed) the default quote database.
    ///
    /// Returns `Ok(None)` when the policy disables the cache.
    pub fn open(policy: QuoteCachePolicy) -> Result<Option<QuoteStore>> {
        if !policy.enabled {
            return Ok(None);
        }
        let store = Self::open_at(policy, &sdk_state_dir(), &default_media_root())?;
        Ok(Some(store))
    }

    /// Open the default store with the default policy.
    pub fn open_default() -> Result<QuoteStore> {
        Ok(Self::open(QuoteCachePolicy::default())?.expect("default policy is enabled"))
    }

    /// Open a store at explicit locations.
    pub fn open_at(
        policy: QuoteCachePolicy,
        root_dir: &Path,
        media_root: &Path,
    ) -> Result<QuoteStore> {
        std::fs::create_dir_all(root_dir).map_err(io)?;
        std::fs::create_dir_all(media_root).map_err(io)?;

        let db_path = root_dir.join("ref-messages.sqlite");
        let conn = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(sqlite)?;

        // Set the busy timeout *before* touching the journal mode. Switching a
        // fresh database into WAL needs a lock on it, so with the default zero
        // timeout two callers racing to create the store would fail with
        // "database is locked" instead of one waiting for the other.
        conn.busy_timeout(std::time::Duration::from_millis(5000))
            .map_err(sqlite)?;

        enable_wal(&conn)?;

        conn.execute_batch(
            r#"
            PRAGMA synchronous = NORMAL;
            CREATE TABLE IF NOT EXISTS quote_messages (
                account_id TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                direction TEXT NOT NULL,
                body TEXT NOT NULL DEFAULT '',
                media_path TEXT,
                media_mime TEXT,
                media_name TEXT,
                media_size INTEGER,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (account_id, conversation_id, message_id)
            );
            CREATE INDEX IF NOT EXISTS quote_messages_account_created
                ON quote_messages (account_id, created_at);
            CREATE INDEX IF NOT EXISTS quote_messages_media_path
                ON quote_messages (media_path);
            "#,
        )
        .map_err(sqlite)?;

        Ok(QuoteStore {
            conn,
            media_root: media_root.to_path_buf(),
            policy,
            writes_since_gc: 0,
        })
    }

    /// Replace the policy and schedule a GC pass.
    pub fn set_policy(&mut self, policy: QuoteCachePolicy) {
        self.policy = policy;
        self.gc();
    }

    /// Current policy.
    pub fn policy(&self) -> &QuoteCachePolicy {
        &self.policy
    }

    /// Look up a cached message. Expired records are deleted on read.
    pub fn find(
        &self,
        account_id: &str,
        conversation_id: &str,
        message_id: &str,
    ) -> Option<QuoteMessageRecord> {
        if message_id.is_empty() {
            return None;
        }
        let row: Option<QuoteRow> = self
            .conn
            .query_row(
                "SELECT account_id, conversation_id, message_id, direction, body, \
                 media_path, media_mime, media_name, media_size, created_at \
                 FROM quote_messages \
                 WHERE account_id = ?1 AND conversation_id = ?2 AND message_id = ?3",
                params![account_id, conversation_id, message_id],
                |r| {
                    Ok(QuoteRow {
                        account_id: r.get(0)?,
                        conversation_id: r.get(1)?,
                        message_id: r.get(2)?,
                        direction: r.get(3)?,
                        body: r.get(4)?,
                        media_path: r.get(5)?,
                        media_mime: r.get(6)?,
                        media_name: r.get(7)?,
                        media_size: r.get(8)?,
                        created_at: r.get(9)?,
                    })
                },
            )
            .ok();

        let row = row?;
        let now = now_ms();
        if now.saturating_sub(row.created_at) > self.policy.retention_ms as i64 {
            self.delete_message(account_id, conversation_id, message_id);
            return None;
        }
        Some(row.into_record())
    }

    /// Insert or update a message, upserting the managed media reference.
    pub fn put(&mut self, input: &QuoteMessageInput) -> Result<()> {
        if input.message_id.is_empty()
            || (input.body.is_empty() && input.source_media_path.is_none())
        {
            return Ok(());
        }

        let managed = match &input.source_media_path {
            Some(path) => {
                self.register_managed_media(&input.account_id, path, input.media_mime.as_deref())?
            }
            None => None,
        };

        let created_at = if input.created_at > 0 {
            input.created_at
        } else {
            now_ms()
        };

        self.conn
            .execute(
                "INSERT INTO quote_messages (
                    account_id, conversation_id, message_id, direction, body,
                    media_path, media_mime, media_name, media_size, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(account_id, conversation_id, message_id) DO UPDATE SET
                    direction = excluded.direction,
                    body = excluded.body,
                    media_path = COALESCE(excluded.media_path, quote_messages.media_path),
                    media_mime = COALESCE(excluded.media_mime, quote_messages.media_mime),
                    media_name = COALESCE(excluded.media_name, quote_messages.media_name),
                    media_size = COALESCE(excluded.media_size, quote_messages.media_size),
                    created_at = excluded.created_at",
                params![
                    input.account_id,
                    input.conversation_id,
                    input.message_id,
                    input.direction.as_str(),
                    input.body,
                    managed.as_ref().map(|m| m.path.clone()),
                    input
                        .media_mime
                        .clone()
                        .or(managed.as_ref().and_then(|m| m.mime.clone())),
                    input
                        .media_name
                        .clone()
                        .or(managed.as_ref().map(|m| m.name.clone())),
                    managed.as_ref().map(|m| m.size),
                    created_at,
                ],
            )
            .map_err(sqlite)?;

        self.writes_since_gc += 1;
        if managed.is_some() {
            self.enforce_media_budget(&input.account_id);
        }
        if self.writes_since_gc >= GC_WRITE_INTERVAL {
            self.writes_since_gc = 0;
            self.gc();
        }
        Ok(())
    }

    /// Delete every message and managed media file for an account.
    pub fn delete_account(&mut self, account_id: &str) {
        let _ = self.conn.execute(
            "DELETE FROM quote_messages WHERE account_id = ?1",
            params![account_id],
        );
        let dir = self.media_root.join(account_media_dir_name(account_id));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

struct QuoteRow {
    account_id: String,
    conversation_id: String,
    message_id: String,
    direction: String,
    body: String,
    media_path: Option<String>,
    media_mime: Option<String>,
    media_name: Option<String>,
    media_size: Option<i64>,
    created_at: i64,
}

impl QuoteRow {
    fn into_record(self) -> QuoteMessageRecord {
        QuoteMessageRecord {
            account_id: self.account_id,
            conversation_id: self.conversation_id,
            message_id: self.message_id,
            direction: Direction::from_str(&self.direction),
            body: self.body,
            media_path: self.media_path,
            media_mime: self.media_mime,
            media_name: self.media_name,
            media_size: self.media_size,
            created_at: self.created_at,
        }
    }
}

struct ManagedMedia {
    path: String,
    mime: Option<String>,
    name: String,
    size: i64,
}

struct MediaRow {
    media_path: String,
    media_size: i64,
    newest_at: i64,
}

impl QuoteStore {
    /// Run a full garbage-collection pass.
    ///
    /// Drops expired messages, trims each account to its message cap, expires
    /// old media, enforces the per-account media budget, and finally removes
    /// media files that no row references any more.
    pub fn gc(&mut self) {
        let now = now_ms();
        let message_cutoff = now - self.policy.retention_ms as i64;
        let _ = self.conn.execute(
            "DELETE FROM quote_messages WHERE created_at < ?1",
            params![message_cutoff],
        );

        let accounts = self.list_accounts_with_rows();
        for account_id in &accounts {
            self.trim_to_cap(account_id);
            let media_cutoff = now - self.policy.media_retention_ms as i64;
            for media in self.list_media(account_id) {
                if media.newest_at < media_cutoff {
                    self.detach_media(&media.media_path);
                }
            }
            self.enforce_media_budget(account_id);
        }

        self.prune_orph_media_files();
    }

    fn list_accounts_with_rows(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self
            .conn
            .prepare("SELECT DISTINCT account_id FROM quote_messages")
        {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    fn trim_to_cap(&self, account_id: &str) {
        let _ = self.conn.execute(
            "DELETE FROM quote_messages
             WHERE rowid IN (
                 SELECT rowid FROM quote_messages
                 WHERE account_id = ?1
                 ORDER BY created_at DESC, rowid DESC
                 LIMIT -1 OFFSET ?2
             )",
            params![account_id, self.policy.max_messages_per_account as i64],
        );
    }

    fn list_media(&self, account_id: &str) -> Vec<MediaRow> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT media_path, MAX(COALESCE(media_size, 0)) AS media_size,
                    MAX(created_at) AS newest_at
             FROM quote_messages
             WHERE account_id = ?1 AND media_path IS NOT NULL
             GROUP BY media_path
             ORDER BY newest_at ASC",
        ) {
            if let Ok(rows) = stmt.query_map(params![account_id], |r| {
                Ok(MediaRow {
                    media_path: r.get(0)?,
                    media_size: r.get(1)?,
                    newest_at: r.get(2)?,
                })
            }) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    fn enforce_media_budget(&self, account_id: &str) {
        let rows = self.list_media(account_id);
        let mut total: i64 = rows.iter().map(|r| r.media_size.max(0)).sum();
        for row in &rows {
            if total <= self.policy.max_media_bytes_per_account as i64 {
                break;
            }
            self.detach_media(&row.media_path);
            total -= row.media_size.max(0);
        }
    }

    /// Remove the media link from every row, then delete the file if orphaned.
    fn detach_media(&self, media_path: &str) {
        let _ = self.conn.execute(
            "UPDATE quote_messages SET media_path = NULL WHERE media_path = ?1",
            params![media_path],
        );
        self.remove_media_if_orphaned(media_path);
    }

    fn remove_media_if_orphaned(&self, media_path: &str) {
        let still_used = self
            .conn
            .query_row(
                "SELECT 1 FROM quote_messages WHERE media_path = ?1 LIMIT 1",
                params![media_path],
                |r| r.get::<_, i64>(0),
            )
            .is_ok();
        if still_used {
            return;
        }
        if let Err(err) = std::fs::remove_file(media_path) {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %media_path, error = %err, "quote cache: failed to remove media");
            }
        }
    }

    /// Delete media files under the managed root that no row references.
    fn prune_orph_media_files(&self) {
        if !self.media_root.exists() {
            return;
        }
        let known = self.known_media_paths();
        let root = self.media_root.clone();
        visit_dir(&root, &known);
    }

    fn known_media_paths(&self) -> std::collections::HashSet<String> {
        let mut out = std::collections::HashSet::new();
        if let Ok(mut stmt) = self
            .conn
            .prepare("SELECT DISTINCT media_path FROM quote_messages WHERE media_path IS NOT NULL")
        {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    fn delete_message(&self, account_id: &str, conversation_id: &str, message_id: &str) {
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT media_path FROM quote_messages
                 WHERE account_id = ?1 AND conversation_id = ?2 AND message_id = ?3",
                params![account_id, conversation_id, message_id],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        let _ = self.conn.execute(
            "DELETE FROM quote_messages
             WHERE account_id = ?1 AND conversation_id = ?2 AND message_id = ?3",
            params![account_id, conversation_id, message_id],
        );
        if let Some(path) = existing {
            self.remove_media_if_orphaned(&path);
        }
    }

    /// Adopt a media file that already lives under this account's managed root.
    ///
    /// Files outside the managed root are refused, so the cache can never take
    /// ownership of an unrelated path.
    fn register_managed_media(
        &self,
        account_id: &str,
        source_path: &str,
        mime: Option<&str>,
    ) -> Result<Option<ManagedMedia>> {
        let account_dir = self.account_media_dir(account_id);
        let canonical_root =
            std::fs::canonicalize(&account_dir).unwrap_or_else(|_| account_dir.clone());
        let canonical_source = match std::fs::canonicalize(source_path) {
            Ok(p) => p,
            Err(err) => {
                tracing::warn!(path = %source_path, error = %err, "quote cache: refusing unmanaged media path");
                return Ok(None);
            }
        };
        if !canonical_source.starts_with(&canonical_root) {
            tracing::warn!(path = %source_path, "quote cache: refusing media outside the managed root");
            return Ok(None);
        }
        let meta = match std::fs::metadata(&canonical_source) {
            Ok(m) => m,
            Err(err) => {
                tracing::warn!(path = %source_path, error = %err, "quote cache: cannot stat media");
                return Ok(None);
            }
        };
        if !meta.is_file() || meta.len() > self.policy.max_single_media_bytes {
            return Ok(None);
        }
        Ok(Some(ManagedMedia {
            path: canonical_source.to_string_lossy().into_owned(),
            mime: mime.map(str::to_string),
            name: canonical_source
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            size: meta.len() as i64,
        }))
    }

    /// Path of an account's managed media directory.
    pub fn account_media_dir(&self, account_id: &str) -> std::path::PathBuf {
        self.media_root.join(account_media_dir_name(account_id))
    }

    /// Create and return the account's managed media directory.
    pub fn ensure_account_media_dir(&self, account_id: &str) -> Result<std::path::PathBuf> {
        let dir = self.account_media_dir(account_id);
        std::fs::create_dir_all(&dir).map_err(io)?;
        Ok(dir)
    }
}

fn visit_dir(dir: &std::path::Path, known: &std::collections::HashSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit_dir(&path, known);
        } else if !known.contains(&path.to_string_lossy().into_owned()) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn io(err: std::io::Error) -> Error {
    Error::from(err)
}

/// Switch the database into WAL mode.
///
/// `PRAGMA journal_mode = WAL` deliberately ignores `busy_timeout`: SQLite
/// returns `SQLITE_BUSY` immediately rather than waiting, because waiting could
/// deadlock against another connection that holds the lock while waiting for
/// this one. That means two processes (or threads) opening the same fresh
/// database at once make the loser fail with "database is locked".
///
/// Retry briefly instead, and treat "already in WAL" as success. Once one
/// caller has switched the database, every later `PRAGMA journal_mode = WAL`
/// returns `wal` without needing the exclusive lock.
fn enable_wal(conn: &Connection) -> Result<()> {
    const ATTEMPTS: u32 = 50;
    let mut last = String::new();
    for attempt in 0..ATTEMPTS {
        match conn.query_row("PRAGMA journal_mode = WAL", [], |row| {
            row.get::<_, String>(0)
        }) {
            Ok(mode) if mode.eq_ignore_ascii_case("wal") => return Ok(()),
            Ok(mode) => last = format!("journal_mode is {mode}, expected wal"),
            Err(err) => last = err.to_string(),
        }
        if attempt + 1 < ATTEMPTS {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
    Err(Error::unknown(format!("could not enable WAL: {last}")))
}

fn sqlite(err: rusqlite::Error) -> Error {
    Error::from(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BACKSLASH: char = '\\';

    /// Opening the same fresh database from several threads at once must not
    /// fail: switching a new database into WAL returns SQLITE_BUSY immediately
    /// (it deliberately ignores `busy_timeout`), so the opener has to retry.
    #[test]
    fn concurrent_open_does_not_fail() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let media = dir.path().join("media");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let root = root.clone();
                let media = media.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    QuoteStore::open_at(QuoteCachePolicy::default(), &root, &media)
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                })
            })
            .collect();

        let failures: Vec<String> = handles
            .into_iter()
            .filter_map(|h| h.join().unwrap().err())
            .collect();
        assert!(failures.is_empty(), "concurrent open failed: {failures:?}");
    }

    fn test_store() -> (tempfile::TempDir, QuoteStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = QuoteStore::open_at(
            QuoteCachePolicy::default(),
            &dir.path().join("root"),
            &dir.path().join("media"),
        )
        .unwrap();
        (dir, store)
    }

    fn input(body: &str, media: Option<&str>) -> QuoteMessageInput {
        QuoteMessageInput {
            account_id: "acct".into(),
            conversation_id: "u@im.wechat".into(),
            message_id: "1".into(),
            direction: Direction::Inbound,
            body: body.into(),
            source_media_path: media.map(str::to_string),
            media_mime: None,
            media_name: None,
            created_at: now_ms(),
        }
    }

    #[test]
    fn policy_defaults() {
        let p = QuoteCachePolicy::default();
        assert!(p.enabled);
        assert_eq!(p.max_messages_per_account, 10_000);
        assert_eq!(p.max_single_media_bytes, 25 * 1024 * 1024);
        assert_eq!(p.retention_ms, 30 * 24 * 60 * 60 * 1000);
    }

    #[test]
    fn disabled_policy_opens_nothing() {
        let policy = QuoteCachePolicy {
            enabled: false,
            ..QuoteCachePolicy::default()
        };
        assert!(QuoteStore::open(policy).unwrap().is_none());
    }

    #[test]
    fn put_and_find_round_trip() {
        let (_d, mut store) = test_store();
        store.put(&input("hello", None)).unwrap();
        let found = store.find("acct", "u@im.wechat", "1").unwrap();
        assert_eq!(found.body, "hello");
        assert_eq!(found.direction, Direction::Inbound);
        assert!(found.media_path.is_none());
    }

    #[test]
    fn upsert_updates_existing_row() {
        let (_d, mut store) = test_store();
        store.put(&input("first", None)).unwrap();
        let mut second = input("second", None);
        second.created_at = now_ms() + 1;
        store.put(&second).unwrap();

        let found = store.find("acct", "u@im.wechat", "1").unwrap();
        assert_eq!(found.body, "second");
        assert_eq!(found.created_at, second.created_at);

        let count: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM quote_messages WHERE message_id = '1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn empty_body_and_no_media_is_ignored() {
        let (_d, mut store) = test_store();
        store.put(&input("", None)).unwrap();
        assert!(store.find("acct", "u@im.wechat", "1").is_none());
    }

    #[test]
    fn empty_message_id_is_ignored() {
        let (_d, mut store) = test_store();
        let mut i = input("x", None);
        i.message_id = String::new();
        store.put(&i).unwrap();
        assert!(store.find("acct", "u@im.wechat", "").is_none());
    }

    #[test]
    fn find_misses_return_none() {
        let (_d, mut store) = test_store();
        store.put(&input("x", None)).unwrap();
        assert!(store.find("acct", "u@im.wechat", "999").is_none());
        assert!(store.find("other", "u@im.wechat", "1").is_none());
    }

    #[test]
    fn expired_records_are_deleted_on_read() {
        let dir = tempfile::tempdir().unwrap();
        let policy = QuoteCachePolicy {
            retention_ms: 1_000,
            ..QuoteCachePolicy::default()
        };
        let mut store =
            QuoteStore::open_at(policy, &dir.path().join("r"), &dir.path().join("m")).unwrap();
        // A timestamp far in the past is already outside the 1s window.
        let mut stale = input("old", None);
        stale.created_at = now_ms() - 60_000;
        store.put(&stale).unwrap();
        assert!(store.find("acct", "u@im.wechat", "1").is_none());

        // A fresh record inside the window is still returned.
        store.put(&input("fresh", None)).unwrap();
        assert_eq!(
            store.find("acct", "u@im.wechat", "1").unwrap().body,
            "fresh"
        );
    }

    #[test]
    fn media_outside_managed_root_is_refused() {
        let (_dir, mut store) = test_store();
        let outside = std::env::temp_dir().join("outside.png");
        std::fs::write(&outside, b"not-yours").unwrap();

        store
            .put(&input("pic", Some(outside.to_string_lossy().as_ref())))
            .unwrap();
        let found = store.find("acct", "u@im.wechat", "1").unwrap();
        assert!(found.media_path.is_none(), "must not adopt foreign media");
    }

    #[test]
    fn media_inside_managed_root_is_registered() {
        let (_dir, mut store) = test_store();
        let media_dir = store.ensure_account_media_dir("acct").unwrap();
        let file = media_dir.join("quote.png");
        std::fs::write(&file, b"imagedata").unwrap();

        let mut i = input("", Some(file.to_string_lossy().as_ref()));
        i.media_mime = Some("image/png".into());
        store.put(&i).unwrap();

        let found = store.find("acct", "u@im.wechat", "1").unwrap();
        assert_eq!(found.media_mime.as_deref(), Some("image/png"));
        assert_eq!(found.media_size, Some(9));
        assert!(found.media_path.is_some());
        assert!(file.exists(), "a live row references the file");
    }

    #[test]
    fn delete_account_removes_rows_and_media() {
        let (_d, mut store) = test_store();
        let media_dir = store.ensure_account_media_dir("acct").unwrap();
        let file = media_dir.join("m.png");
        std::fs::write(&file, b"data").unwrap();
        store
            .put(&input("", Some(file.to_string_lossy().as_ref())))
            .unwrap();
        assert!(file.exists());

        store.delete_account("acct");
        assert!(store.find("acct", "u@im.wechat", "1").is_none());
        assert!(!file.exists(), "media must be removed with the account");
    }

    #[test]
    fn gc_trims_beyond_message_cap() {
        let dir = tempfile::tempdir().unwrap();
        let policy = QuoteCachePolicy {
            max_messages_per_account: 2,
            ..QuoteCachePolicy::default()
        };
        let mut store =
            QuoteStore::open_at(policy, &dir.path().join("r"), &dir.path().join("m")).unwrap();

        let base = now_ms();
        for n in 1..=5i64 {
            let mut i = input("x", None);
            i.message_id = n.to_string();
            // 1s apart so ordering is unambiguous even if the loop is fast.
            i.created_at = base + n * 1_000;
            store.put(&i).unwrap();
        }

        // The cap is enforced by gc(), exactly like the TypeScript `runGc`.
        store.gc();

        for n in 1..=5i64 {
            let still_there = store.find("acct", "u@im.wechat", &n.to_string()).is_some();
            if n >= 4 {
                assert!(still_there, "id {n} should have survived (newest 2)");
            } else {
                assert!(!still_there, "id {n} should have been trimmed");
            }
        }
    }

    #[test]
    fn safe_path_segment_mangles_unsafe_input() {
        assert_eq!(safe_path_segment("Abc"), "abc");
        let raw = format!("a/{}c", BACKSLASH);
        // Both separators collapse to '_', so the result is a__c.
        assert_eq!(safe_path_segment(&raw), "a__c");
        assert_eq!(safe_path_segment(".."), "default");
        assert_eq!(safe_path_segment(""), "default");
        assert_eq!(safe_path_segment("  "), "default");
    }

    #[test]
    fn account_media_dir_name_is_stable_and_distinct() {
        assert_eq!(
            account_media_dir_name("acct"),
            account_media_dir_name("acct")
        );
        assert_ne!(account_media_dir_name("a"), account_media_dir_name("b"));
    }

    #[test]
    fn quote_media_subdir_shape() {
        assert!(quote_media_subdir("acct").starts_with(QUOTE_MEDIA_SUBDIR));
        assert!(quote_media_subdir("acct").contains("acct-"));
    }

    #[test]
    fn direction_round_trip() {
        assert_eq!(
            Direction::from_str(Direction::Inbound.as_str()),
            Direction::Inbound
        );
        assert_eq!(
            Direction::from_str(Direction::Outbound.as_str()),
            Direction::Outbound
        );
        assert_eq!(Direction::from_str("garbage"), Direction::Inbound);
    }
}
