//! Weixin (iLink) backend API client.
//!
//! Every request is JSON over HTTPS. Authenticated bot requests carry
//! `AuthorizationType: ilink_bot_token`, `Authorization: Bearer <token>`,
//! `X-WECHAT-UIN`, `iLink-App-Id` and `iLink-App-ClientVersion` headers plus
//! a `base_info` object. See `src/api/api.ts` in the TypeScript plugin.

pub mod client;
pub mod session_guard;
pub mod types;

pub use client::{
    build_base_info, build_client_version, parse_api_json, quote_lossless_ids, sanitize_bot_agent,
    WeixinApiClient, WeixinApiOptions, AUTHORIZATION_TYPE, DEFAULT_API_TIMEOUT_MS,
    DEFAULT_CONFIG_TIMEOUT_MS, DEFAULT_LONG_POLL_TIMEOUT_MS, ILINK_APP_ID,
};
pub use session_guard::{
    assert_session_active, get_remaining_pause_ms, is_session_paused, pause_session,
    reset_session_pauses, resume_session, SessionGuard, SESSION_PAUSE_DURATION,
    STALE_TOKEN_ERRCODE,
};
pub use types::*;
