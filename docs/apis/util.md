# Utilities API

The util module ([`../../src/util/`](../../src/util/)) provides ID generation,
diagnostic redaction, and time helpers, used by the SDK internally and available to
callers.

## Module composition

| Submodule | File | Responsibility |
| --- | --- | --- |
| `random` | [`../../src/util/random.rs`](../../src/util/random.rs) | unique IDs and random values |
| `redact` | [`../../src/util/redact.rs`](../../src/util/redact.rs) | redaction for diagnostic output |
| `time` | [`../../src/util/time.rs`](../../src/util/time.rs) | timestamps |

The `util` module's public re-exports are in
[`../../src/util/mod.rs`](../../src/util/mod.rs).

## `random` submodule

```rust
/// Prefixed unique id: "<prefix>:<ms-timestamp>-<4-byte-hex>"
pub fn generate_id(prefix: &str) -> String;

/// Temp filename: "<prefix>-<ms-timestamp>-<4-byte-hex><ext>"
pub fn temp_file_name(prefix: &str, ext: &str) -> String;

/// 16 random bytes, rendered as 32 hex chars (CDN filekey).
pub fn file_key() -> String;

/// Random u32, rendered as a decimal string (X-WECHAT-UIN).
pub fn random_u32() -> u32;
```

Format examples:

- `generate_id("openclaw-weixin")` → `"openclaw-weixin:1761000000123-a1b2c3d4"`
- `temp_file_name("weixin-remote", ".png")` →
  `"weixin-remote-1761000000123-a1b2c3d4.png"`
- `file_key()` → 32 hex chars

All functions use the cryptographically-secure RNG behind `rand::rng()` (the
default `rand 0.9` backend in practice).

Example:

```rust,no_run
use openclaw_weixin::util::random::{generate_id, temp_file_name, file_key};

# fn example() {
let id = generate_id("openclaw-weixin");
let name = temp_file_name("weixin-out", ".jpg");
let key = file_key();
println!("{}, {}, {}", id, name, key);
# }
```

## `redact` submodule

Diagnostics are redacted before logging to avoid leaking tokens, authorization
headers, or signed query strings.

```rust
pub const DEFAULT_TOKEN_PREFIX_LEN: usize = 6;
pub const DEFAULT_BODY_MAX_LEN: usize = 200;

/// Truncate a string, appending a length note when over (e.g. "abc…(len=18)"); empty -> empty.
pub fn truncate(s: &str, max: usize) -> String;

/// Redact a token/secret: keep the first prefix_len chars + the total length; missing -> "(none)".
pub fn redact_token(token: Option<&str>, prefix_len: Option<usize>) -> String;

/// Redact sensitive JSON fields, then truncate for logging.
/// Equivalent to redact_json_fields(body), then truncate to max_len (default 200).
pub fn redact_body(body: &str, max_len: Option<usize>) -> String;

/// Replace values of sensitive JSON keys with "<redacted>" (does not corrupt other text).
pub fn redact_json_fields(input: &str) -> String;

/// Strip the query string (often contains signatures/tokens), keep scheme/host/path.
pub fn redact_url(url: &str) -> String;
```

### Sensitive fields

`redact_json_fields` rewrites only the values of these JSON keys (using a small
hand-written scanner that won't damage text that merely *looks* like a key):

`context_token`, `bot_token`, `token`, `authorization`, `Authorization`.

### Example

```rust,no_run
use openclaw_weixin::util::redact::{redact_token, redact_body, redact_url};

# fn example() {
assert_eq!(redact_token(None, None), "(none)");
assert_eq!(redact_token(Some("abcdef1234"), None), "abcdef…(len=10)");

let body = r#"{"context_token":"secret","token":"t","other":"keep"}"#;
let out = redact_body(body, None);
assert!(out.contains(r#""context_token":"<redacted>""#));
assert!(out.contains(r#""other":"keep""#));

assert_eq!(
    redact_url("https://ilinkai.weixin.qq.com/ilink/bot/getupdates?token=abc"),
    "https://ilinkai.weixin.qq.com/ilink/bot/getupdates?<redacted>"
);
# }
```

- `truncate` counts by **characters** (not bytes), so Chinese etc. truncate correctly
  and report the true length.
- `redact_json_fields` only replaces on exact `"key" : "value"` key/value structure;
  text mentioning "the token is here" is not accidentally replaced.

## `time` submodule

```rust
/// Current Unix millisecond timestamp (u128).
pub fn now_millis() -> u128;

/// Current ISO-8601 (UTC) string with millisecond precision (matches JS toISOString).
pub fn now_iso8601() -> String;
```

`now_iso8601` uses Howard Hinnant's `civil_from_days` algorithm to convert "days
since 1970-01-01" into `(year, month, day)`, then assembles
`YYYY-MM-DDTHH:MM:SS.mmmZ`:

```rust,no_run
use openclaw_weixin::util::time::{now_iso8601, now_millis};

# fn example() {
let s = now_iso8601();
let ms = now_millis();
println!("{} ({} ms)", s, ms);
// e.g.: 2026-01-01T00:00:00.000Z
# }
```

## Design notes and caveats

1. **`generate_id` / `temp_file_name` include a millisecond timestamp and could
   collide under extreme same-machine concurrency** (collision probability is very
   low thanks to the 4-byte random suffix). Add your own salt if you need global
   uniqueness.
2. **`redact_token` keeps the first 6 chars by default.** Enough to tell tokens apart
   in logs without leaking the sensitive part; override via `prefix_len`.
3. **`redact_url` falls back to truncation on invalid URLs.** An unparseable URL is
   `truncate(url, 80)` rather than an error.
4. **`now_millis` returns `u128`.** No overflow risk before year 2262 (far beyond);
   when interfacing with backend uint64 timestamps, store as `String` to avoid
   truncation (see [api-client.md](api-client.md)).
5. **Time conversion supports pre-1970 dates.** `civil_from_days` is correct for
   negative day counts (before 1970), handy for boundary tests.
