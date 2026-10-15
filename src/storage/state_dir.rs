//! State directory resolution, mirroring `src/storage/state-dir.ts`.

use std::path::{Path, PathBuf};

/// Resolve the local state directory.
///
/// Resolution order (first non-empty wins):
/// 1. `OPENCLAW_STATE_DIR`
/// 2. `CLAWDBOT_STATE_DIR`
/// 3. `$XDG_STATE_HOME/openclaw` (Unix only, when set)
/// 4. `$HOME/.openclaw`
///
/// The TypeScript plugin only consults the first two env vars plus the home
/// directory; the XDG step is added so the Rust SDK behaves well on Linux
/// desktops, and can be disabled by setting `OPENCLAW_STATE_DIR`.
pub fn resolve_state_dir() -> PathBuf {
    for key in ["OPENCLAW_STATE_DIR", "CLAWDBOT_STATE_DIR"] {
        if let Some(value) = std::env::var_os(key) {
            let trimmed = value.to_string_lossy().trim().to_string();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed);
            }
        }
    }

    #[cfg(unix)]
    if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
        let xdg = xdg.to_string_lossy().trim().to_string();
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("openclaw");
        }
    }

    home_dir()
        .map(|h| h.join(".openclaw"))
        .unwrap_or_else(|| PathBuf::from(".openclaw"))
}

/// The directory that holds this SDK's own state.
pub fn sdk_state_dir() -> PathBuf {
    resolve_state_dir().join("openclaw-weixin")
}

/// The directory holding per-account credential files.
pub fn accounts_dir() -> PathBuf {
    sdk_state_dir().join("accounts")
}

/// Best-effort home directory lookup without pulling in the `directories`
/// crate (which is CLI-only).
fn home_dir() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy().trim().to_string();
        if !home.is_empty() {
            return Some(PathBuf::from(home));
        }
    }
    #[cfg(windows)]
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        let profile = profile.to_string_lossy().trim().to_string();
        if !profile.is_empty() {
            return Some(PathBuf::from(profile));
        }
    }
    None
}

/// Create a directory (and parents) returning the resulting path.
pub fn ensure_dir(path: &Path) -> std::io::Result<&Path> {
    std::fs::create_dir_all(path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EnvGuard(Option<std::ffi::OsString>, Option<std::ffi::OsString>);

    impl EnvGuard {
        fn set(state: Option<&str>, claude: Option<&str>) -> Self {
            let a = std::env::var_os("OPENCLAW_STATE_DIR");
            let b = std::env::var_os("CLAWDBOT_STATE_DIR");
            match state {
                Some(v) => std::env::set_var("OPENCLAW_STATE_DIR", v),
                None => std::env::remove_var("OPENCLAW_STATE_DIR"),
            }
            match claude {
                Some(v) => std::env::set_var("CLAWDBOT_STATE_DIR", v),
                None => std::env::remove_var("CLAWDBOT_STATE_DIR"),
            }
            EnvGuard(a, b)
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.0 {
                Some(v) => std::env::set_var("OPENCLAW_STATE_DIR", v),
                None => std::env::remove_var("OPENCLAW_STATE_DIR"),
            }
            match &self.1 {
                Some(v) => std::env::set_var("CLAWDBOT_STATE_DIR", v),
                None => std::env::remove_var("CLAWDBOT_STATE_DIR"),
            }
        }
    }

    #[test]
    fn prefers_openclaw_state_dir() {
        let _l = super::super::sync_buf::test_env_lock().lock().unwrap();
        let _g = EnvGuard::set(Some("/tmp/ocd-state"), Some("/tmp/other"));
        assert_eq!(resolve_state_dir(), PathBuf::from("/tmp/ocd-state"));
    }

    #[test]
    fn falls_back_to_claudebot_state_dir() {
        let _l = super::super::sync_buf::test_env_lock().lock().unwrap();
        let _g = EnvGuard::set(None, Some("/tmp/claw-state"));
        assert_eq!(resolve_state_dir(), PathBuf::from("/tmp/claw-state"));
    }

    #[test]
    fn blanks_are_ignored() {
        let _l = super::super::sync_buf::test_env_lock().lock().unwrap();
        let _g = EnvGuard::set(Some("   "), Some("/tmp/claw-state"));
        assert_eq!(resolve_state_dir(), PathBuf::from("/tmp/claw-state"));
    }

    #[test]
    fn sdk_dirs_are_nested() {
        // resolve_state_dir() reads process-global env vars, so hold the lock
        // to keep the two derivations consistent under parallel tests.
        let _l = super::super::sync_buf::test_env_lock().lock().unwrap();
        assert_eq!(sdk_state_dir(), resolve_state_dir().join("openclaw-weixin"));
        assert_eq!(
            accounts_dir(),
            resolve_state_dir().join("openclaw-weixin/accounts")
        );
    }

    #[test]
    fn ensure_dir_creates() {
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("a/b/c");
        ensure_dir(&target).unwrap();
        assert!(target.is_dir());
    }
}
