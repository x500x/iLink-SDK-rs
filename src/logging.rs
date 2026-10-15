//! Minimal structured logging facade.
//!
//! The SDK is `tracing`-based but works with no global subscriber installed
//! (records are dropped silently). Convenience macros are re-exported so
//! downstream applications can opt into `env_logger`, `tracing_subscriber`,
//! or nothing at all.
//!
//! When the `cli` feature is enabled, [`init_tracing`] installs a default
//! subscriber driven by the `RUST_LOG` environment variable.

pub use tracing::{debug, error, info, trace, warn};

/// A `tracing` span field carrying the account id, used by every SDK span.
pub const ACCOUNT_FIELD: &str = "account";

/// Logging level shared by the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn as_str(&self) -> &'static str {
        match self {
            Level::Trace => "trace",
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
        }
    }
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Level {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "trace" => Ok(Level::Trace),
            "debug" => Ok(Level::Debug),
            "info" => Ok(Level::Info),
            "warn" | "warning" => Ok(Level::Warn),
            "error" => Ok(Level::Error),
            other => Err(format!("invalid log level: {other}")),
        }
    }
}

/// Install a `tracing_subscriber` honouring `RUST_LOG` (defaulting to `info`).
///
/// No-op when the `cli` feature is disabled — in library mode the host
/// application owns the subscriber.
#[cfg(feature = "cli")]
pub fn init_tracing(level: Level) {
    use tracing_subscriber::EnvFilter;

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level.as_str()));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_ansi(std::env::var_os("NO_COLOR").is_none())
        .try_init();
}

#[cfg(not(feature = "cli"))]
pub fn init_tracing(_level: Level) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_levels() {
        assert_eq!("DEBUG".parse::<Level>().unwrap(), Level::Debug);
        assert_eq!("Warning".parse::<Level>().unwrap(), Level::Warn);
        assert!("nope".parse::<Level>().is_err());
    }

    #[test]
    fn init_is_idempotent() {
        init_tracing(Level::Info);
        init_tracing(Level::Info);
    }
}
