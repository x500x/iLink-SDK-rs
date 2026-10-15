//! Local persistence: base URLs, state directory, account store, sync buffer.

pub mod accounts;
pub mod config;
pub mod state_dir;
pub mod sync_buf;

pub use accounts::{
    clear_stale_accounts_for_user_id, derive_raw_account_id, list_account_ids,
    list_indexed_account_ids, load_account, register_account_id, unregister_account_id,
    AccountData, StoredAccount, CDN_BASE_URL, DEFAULT_BASE_URL,
};
pub use config::{
    load_bot_agent, load_route_tag, resolve_channel_config, SdkConfig, CHANNEL_CONFIG_SECTION,
};
pub use state_dir::resolve_state_dir;
pub use sync_buf::{get_sync_buf_path, load_get_updates_buf, save_get_updates_buf, SyncBufData};
