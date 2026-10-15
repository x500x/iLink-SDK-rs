//! Session guard: pauses all API calls for an account after a stale-token error.
//!
//! Ports `src/api/session-guard.ts`. When `getUpdates` reports
//! `errcode == -14` (or `ret == -14`) the bot token is stale, so every
//! subsequent request for that account is blocked for one hour.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{Error, ErrorKind};

/// Error code returned by the server when the bot token is stale / expired.
pub const STALE_TOKEN_ERRCODE: i64 = -14;

/// How long an account is paused after a stale-token error.
pub const SESSION_PAUSE_DURATION: Duration = Duration::from_secs(60 * 60);

fn registry() -> &'static Mutex<HashMap<String, PauseEntry>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, PauseEntry>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, Copy)]
struct PauseEntry {
    until_ms: u128,
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Pause all inbound/outbound API calls for `account_id` for one hour.
pub fn pause_session(account_id: &str) {
    let until = now_ms() + SESSION_PAUSE_DURATION.as_millis();
    let mut map = registry().lock().expect("session guard poisoned");
    map.insert(account_id.to_string(), PauseEntry { until_ms: until });
    tracing::info!(
        account = account_id,
        until_ms = until,
        duration_s = SESSION_PAUSE_DURATION.as_secs(),
        "session-guard: paused account"
    );
}

/// Returns `true` while the account is still inside its cooldown window.
///
/// Expired entries are removed lazily.
pub fn is_session_paused(account_id: &str) -> bool {
    let now = now_ms();
    let mut map = registry().lock().expect("session guard poisoned");
    match map.get(account_id) {
        None => false,
        Some(entry) if now >= entry.until_ms => {
            map.remove(account_id);
            false
        }
        Some(_) => true,
    }
}

/// Milliseconds remaining until the pause expires (0 when not paused).
pub fn get_remaining_pause_ms(account_id: &str) -> u128 {
    let now = now_ms();
    let mut map = registry().lock().expect("session guard poisoned");
    match map.get(account_id) {
        None => 0,
        Some(entry) if now >= entry.until_ms => {
            map.remove(account_id);
            0
        }
        Some(entry) => entry.until_ms - now,
    }
}

/// Throw if the session is currently paused. Call before any API request.
pub fn assert_session_active(account_id: &str) -> Result<(), Error> {
    if is_session_paused(account_id) {
        let remaining_min = get_remaining_pause_ms(account_id).div_ceil(60_000);
        return Err(Error::new(
            ErrorKind::Unknown,
            format!(
                "session paused for accountId={account_id}, {remaining_min} min remaining \
                 (errcode {STALE_TOKEN_ERRCODE})"
            ),
        ));
    }
    Ok(())
}

/// Drop a pause early, e.g. after a successful re-login.
pub fn resume_session(account_id: &str) {
    let mut map = registry().lock().expect("session guard poisoned");
    map.remove(account_id);
}

/// Clear all state. Intended for tests.
pub fn reset_session_pauses() {
    let mut map = registry().lock().expect("session guard poisoned");
    map.clear();
}

/// An owned handle to the pause state, so callers can hold a snapshot without
/// locking the global registry.
///
/// Prefer the free functions above; this struct exists for API symmetry with
/// the rest of the SDK.
#[derive(Debug, Clone)]
pub struct SessionGuard {
    account_id: String,
}

impl SessionGuard {
    /// Create a guard bound to `account_id`. This does not pause anything.
    pub fn new(account_id: impl Into<String>) -> Self {
        SessionGuard {
            account_id: account_id.into(),
        }
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn pause(&self) {
        pause_session(&self.account_id);
    }

    pub fn resume(&self) {
        resume_session(&self.account_id);
    }

    pub fn is_paused(&self) -> bool {
        is_session_paused(&self.account_id)
    }

    pub fn remaining_ms(&self) -> u128 {
        get_remaining_pause_ms(&self.account_id)
    }

    pub fn assert_active(&self) -> Result<(), Error> {
        assert_session_active(&self.account_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A process-unique account id so parallel tests never share state.
    fn account() -> String {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        format!(
            "acct-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        )
    }

    #[test]
    fn pause_then_expires() {
        let id = account();
        assert!(!is_session_paused(&id));
        pause_session(&id);
        assert!(is_session_paused(&id));
        assert!(get_remaining_pause_ms(&id) > 0);
        assert!(assert_session_active(&id).is_err());
        resume_session(&id);
        assert!(!is_session_paused(&id));
        assert_eq!(get_remaining_pause_ms(&id), 0);
        assert!(assert_session_active(&id).is_ok());
    }

    #[test]
    fn guard_object_mirrors_free_functions() {
        let g = SessionGuard::new(account());
        assert!(!g.is_paused());
        g.pause();
        assert!(g.is_paused());
        assert!(g.assert_active().is_err());
        g.resume();
        assert!(!g.is_paused());
        assert!(g.assert_active().is_ok());
    }

    #[test]
    fn accounts_are_independent() {
        let a = account();
        let b = account();
        pause_session(&a);
        assert!(is_session_paused(&a));
        assert!(!is_session_paused(&b));
    }
}
