//! inbound/outbound messaging.

pub mod auth;
pub mod context;
pub mod markdown;
pub mod outbound;
pub mod quote_cache;
pub mod typing;

pub use auth::{
    local_token_list, PollOutcome, QrLogin, QrSession, QrStartResult, QrStatus, QrWaitResult,
    DEFAULT_ILINK_BOT_TYPE, MAX_QR_REFRESH_COUNT, QR_LONG_POLL_TIMEOUT_MS,
};
pub use context::{body_from_items, is_media_item, media_label, message_id};
pub use outbound::{MessageSender, SendOpts, SendResult};
pub use quote_cache::{
    account_media_dir_name, quote_media_subdir, Direction, QuoteCachePolicy, QuoteMessageInput,
    QuoteMessageRecord, QuoteStore,
};
pub use typing::{send_typing, Typing, TypingHelper};
