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
/// The `RUST_LOG` value is parsed for its global level (`info`, `debug`, …);
/// per-target directives (`ilink_sdk=debug`) are accepted but ignored, which
/// avoids pulling the `env-filter` regex machinery into the binary.
///
/// No-op when the `cli` feature is disabled — in library mode the host
/// application owns the subscriber.
#[cfg(feature = "cli")]
pub fn init_tracing(level: Level) {
    let filter = std::env::var("RUST_LOG")
        .ok()
        .and_then(|raw| parse_global_level(&raw))
        .unwrap_or_else(|| level_filter(level));

    let _ = tracing_subscriber::fmt()
        .with_max_level(filter)
        .with_target(true)
        .with_ansi(std::env::var_os("NO_COLOR").is_none())
        .try_init();
}

/// Extract the global level from a `RUST_LOG` directive list.
#[cfg(feature = "cli")]
fn parse_global_level(raw: &str) -> Option<tracing_subscriber::filter::LevelFilter> {
    raw.split(',')
        .map(str::trim)
        .filter(|directive| !directive.is_empty() && !directive.contains('='))
        .find_map(|directive| directive.parse().ok())
}

#[cfg(feature = "cli")]
fn level_filter(level: Level) -> tracing_subscriber::filter::LevelFilter {
    use tracing_subscriber::filter::LevelFilter;
    match level {
        Level::Trace => LevelFilter::TRACE,
        Level::Debug => LevelFilter::DEBUG,
        Level::Info => LevelFilter::INFO,
        Level::Warn => LevelFilter::WARN,
        Level::Error => LevelFilter::ERROR,
    }
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
