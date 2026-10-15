//! General-purpose helpers: ID generation and log redaction.

pub mod random;
pub mod redact;
pub mod time;

pub use random::{file_key, generate_id, random_u32, temp_file_name};
pub use redact::{
    redact_body, redact_json_fields, redact_token, redact_url, truncate, DEFAULT_BODY_MAX_LEN,
    DEFAULT_TOKEN_PREFIX_LEN,
};
pub use time::{now_iso8601, now_millis};
