//! # ilink-sdk
//!
//! A Rust SDK for the Tencent Weixin (iLink) backend protocol.
//!
//! This crate is a faithful, idiomatic port of the TypeScript
//! [`openclaw-weixin`](https://github.com/Tencent/openclaw-weixin) channel
//! plugin, with the OpenClaw gateway integration removed so it can be used as
//! a plain library: QR-code login, long-poll message delivery, and CDN media
//! upload/download are all available directly.
//!
//! ## Quick start
//!
//! ```no_run
//! use ilink_sdk::prelude::*;
//!
//! # async fn example() -> std::result::Result<(), Box<dyn std::error::Error>> {
//! let api = WeixinApiClient::new(WeixinApiOptions::new(DEFAULT_BASE_URL))?;
//!
//! // 1. Get a QR code and show it to the user.
//! let qr = ilink_sdk::messaging::QrLogin::new(&api);
//! let start = qr.start(None).await?;
//! println!("Scan: {}", start.qrcode_url);
//!
//! // 2. Wait for the user to confirm on their phone.
//! let result = qr
//!     .wait(
//!         &start.session_key,
//!         std::time::Duration::from_secs(480),
//!         |_retry| async {
//!             let mut line = String::new();
//!             std::io::stdin().read_line(&mut line)?;
//!             Ok(line.trim().to_string())
//!         },
//!         |_| async {},
//!     )
//!     .await?;
//!
//! let session = result.session.expect("login must succeed");
//! # Ok(())
//! # }
//! ```

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod api;
pub mod cdn;
pub mod client;
pub mod error;
pub mod ffi;
pub mod ffi_typed;
pub mod logging;
pub mod media;
pub mod messaging;
pub mod storage;
pub mod tls;
pub mod util;

pub use error::{Error, ErrorKind, Result};

/// Convenience re-exports for the common case.
pub mod prelude {
    pub use crate::api::types::*;
    pub use crate::api::{WeixinApiClient, WeixinApiOptions};
    pub use crate::client::{WeixinClient, WeixinClientBuilder, WeixinConfig};
    pub use crate::error::{Error, ErrorKind, Result};
    pub use crate::messaging::auth::QrLogin;
    pub use crate::storage::{CDN_BASE_URL, DEFAULT_BASE_URL};
}
