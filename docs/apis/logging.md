# Logging API

> 中文版: [中文文档](../zh/apis/logging.md)

The logging module ([`../../src/logging.rs`](../../src/logging.rs)) is built on
`tracing`. The SDK emits `tracing` spans and events internally, but **without an
installed global subscriber these are silently dropped** — so a downstream app can
choose `env_logger`, `tracing_subscriber`, or none at all.

When the `cli` feature is enabled, `init_tracing` installs a default subscriber that
honors `RUST_LOG`.

## Re-exported macros

```rust
pub use tracing::{debug, error, info, trace, warn};
```

Callers can use `info!("...")` / `warn!(...)` just like `tracing`, without an extra
`tracing` dependency (assuming the SDK's `tracing` version is available in the
dependency graph, as it usually is transitively).

## Constants & types

```rust
/// The account id field name carried by every SDK span.
pub const ACCOUNT_FIELD: &str = "account";

/// Log level used by the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level { Trace, Debug, Info, Warn, Error }
impl Level {
    pub fn as_str(&self) -> &'static str;
}
impl std::fmt::Display for Level { /* delegates to as_str */ }
impl std::str::FromStr for Level { type Err = String; }   // "warning" -> Warn
```

## `init_tracing`

```rust
#[cfg(feature = "cli")]
pub fn init_tracing(level: Level);

#[cfg(not(feature = "cli"))]
pub fn init_tracing(_level: Level) {}   // no-op when cli is disabled
```

`init_tracing` only compiles the real subscriber installation when the `cli` feature
is on; otherwise it is an empty function. It honors the `RUST_LOG` environment
variable (default level set by the passed `level`, typically `Info`).

## Usage examples

### CLI application (cli feature enabled)

```rust,no_run
use ilink_sdk::logging::{init_tracing, Level};

fn main() {
    init_tracing(Level::Info);   // honors RUST_LOG, default Info
    // ...rest of the logic; SDK-internal info!/warn! now visible
}
```

Run it:

```bash
RUST_LOG=debug cargo run --bin weixin listen
```

### Library application (no cli feature, bring your own tracing subscriber)

```rust,no_run
use ilink_sdk::logging::ACCOUNT_FIELD;
use tracing::info;

# fn example() {
// Install your own subscriber (tracing-subscriber / opentelemetry / …)
info!(account = "acct", "processing inbound message");
let _ = ACCOUNT_FIELD;
# }
```

SDK-internal spans (e.g. `getUpdates` polling, CDN upload) carry the `account`
field, making it easy to filter by account.

## Design notes and caveats

1. **No subscriber installed = no error.** All SDK logging is emitted through
   `tracing`; without a global subscriber the records are dropped, which does not
   affect business correctness.
2. **`init_tracing` depends on the `cli` feature.** In a pure library build
   (`--no-default-features`) it is an empty function; to have the SDK install a
   subscriber for you, build with default features.
3. **`ACCOUNT_FIELD` correlates across spans.** If your own `tracing` code records the
   account id under the same field name, the logging system can aggregate SDK
   internal events with yours by account.
4. **Sensitive data in logs is redacted.** Where the SDK logs response bodies, URLs,
   or tokens, it redacts via `util::redact` first (see [util.md](util.md)), so
   enabling `debug` won't leak credentials.
5. **Don't manually build a subscriber on a hot path.** A process should install the
   global subscriber exactly once; `init_tracing` handles re-installation per
   `tracing-subscriber`'s default behavior.
