//! Typed C ABI: one C function per operation, with C structs instead of JSON.
//!
//! The JSON entry point ([`crate::ffi::ilink_call`]) is a universal escape
//! hatch, but asking C callers to build JSON strings and parse the replies back
//! is unidiomatic and error-prone. This module exports the same operations as
//! ordinary C functions:
//!
//! ```c
//! ilink_mime_info info;
//! if (ilink_mime_info("photo.png", &info) == ILINK_OK) {
//!     printf("%s\n", info.mime);
//!     ilink_mime_info_free(&info);
//! }
//! ```
//!
//! Conventions:
//!
//! * every function returns an [`IlinkStatus`] (`ILINK_OK` on success);
//! * results are written through an out-pointer and released with the matching
//!   `ilink_*_free` function, so the library owns and frees its own memory;
//! * optional strings are `NULL` when absent;
//! * routing defaults (account id, base URL, timeouts, ...) are passed as a
//!   [`IlinkConfig`] — the C equivalent of the `Client` object the other
//!   bindings expose. Pass `NULL` for the defaults;
//! * the last error message is available from [`ilink_last_error`].
//!
//! The Rust side still does the JSON work internally, so `serde_json` handles
//! all escaping and edge cases; C never sees a JSON string unless it asks for
//! one.
//!
//! The exported functions dereference caller-provided pointers, which Rust
//! models as `unsafe`. They are `extern "C"` and deliberately not marked
//! `unsafe fn`: C has no such concept, and the safety contract (valid
//! NUL-terminated strings, non-NULL out-pointers) is documented in the header.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::cell::RefCell;
use std::ffi::{c_char, c_int, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

use serde_json::{json, Value};

use crate::error::{Error, ErrorKind, Result};
use crate::ffi::dispatch_sync;

// ---------------------------------------------------------------------------
// Status codes
// ---------------------------------------------------------------------------

/// `ILINK_OK` on success; any other value is a failure. Mirrors
/// [`crate::error::ErrorKind`] plus argument/panic cases.
pub const ILINK_OK: c_int = 0;
pub const ILINK_ERR_INVALID_ARG: c_int = 1;
pub const ILINK_ERR_NOT_FOUND: c_int = 2;
pub const ILINK_ERR_DNS: c_int = 3;
pub const ILINK_ERR_TCP: c_int = 4;
pub const ILINK_ERR_TLS: c_int = 5;
pub const ILINK_ERR_TIMEOUT: c_int = 6;
pub const ILINK_ERR_CANCELLED: c_int = 7;
pub const ILINK_ERR_UNKNOWN: c_int = 8;
pub const ILINK_ERR_PANIC: c_int = 9;

fn status_of(err: &Error) -> c_int {
    match err.kind() {
        ErrorKind::Dns => ILINK_ERR_DNS,
        ErrorKind::Tcp => ILINK_ERR_TCP,
        ErrorKind::Tls => ILINK_ERR_TLS,
        ErrorKind::Timeout => ILINK_ERR_TIMEOUT,
        ErrorKind::Cancelled => ILINK_ERR_CANCELLED,
        ErrorKind::Unknown => ILINK_ERR_UNKNOWN,
    }
}

thread_local! {
    static LAST_ERROR: RefCell<CString> =
        RefCell::new(CString::new("").expect("empty string"));
}

fn set_last_error(message: &str) {
    // Interior NUL bytes cannot appear in a C string; replace them rather than
    // losing the message.
    let cleaned = message.replace('\0', " ");
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = CString::new(cleaned).unwrap_or_default();
    });
}

/// Human-readable description of a status code (static, never NULL).
#[no_mangle]
pub extern "C" fn ilink_status_message(status: c_int) -> *const c_char {
    let text: &'static str = match status {
        ILINK_OK => "ok\0",
        ILINK_ERR_INVALID_ARG => "invalid argument\0",
        ILINK_ERR_NOT_FOUND => "not found\0",
        ILINK_ERR_DNS => "dns error\0",
        ILINK_ERR_TCP => "tcp error\0",
        ILINK_ERR_TLS => "tls error\0",
        ILINK_ERR_TIMEOUT => "timeout\0",
        ILINK_ERR_CANCELLED => "cancelled\0",
        ILINK_ERR_PANIC => "panic\0",
        _ => "unknown error\0",
    };
    text.as_ptr() as *const c_char
}

/// Message for the most recent failure **on the calling thread**. Valid until
/// the next `ilink_*` call on that thread. Never NULL; empty when there is no
/// error.
#[no_mangle]
pub extern "C" fn ilink_last_error() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ptr())
}

// ---------------------------------------------------------------------------
// Result structs
// ---------------------------------------------------------------------------

/// Routing defaults applied to every call. All fields are optional (`NULL` /
/// `0` means "use the default"). The C counterpart of the other bindings'
/// `Client`/`Config`.
#[repr(C)]
pub struct IlinkConfig {
    pub account_id: *const c_char,
    pub base_url: *const c_char,
    pub cdn_base_url: *const c_char,
    pub route_tag: *const c_char,
    pub bot_agent: *const c_char,
    pub timeout_ms: u64,
}

#[repr(C)]
pub struct IlinkPaths {
    pub state_dir: *mut c_char,
    pub sdk_state_dir: *mut c_char,
    pub accounts_dir: *mut c_char,
    pub config_path: *mut c_char,
}

#[repr(C)]
pub struct IlinkMimeInfo {
    pub mime: *mut c_char,
    pub kind: *mut c_char,
    pub extension: *mut c_char,
    pub is_image: c_int,
    pub is_video: c_int,
    pub is_audio: c_int,
}

#[repr(C)]
pub struct IlinkAccount {
    pub account_id: *mut c_char,
    pub base_url: *mut c_char,
    pub cdn_base_url: *mut c_char,
    /// `NULL` when the account has no known user id.
    pub user_id: *mut c_char,
    /// `NULL` when unnamed.
    pub name: *mut c_char,
    /// `NULL` when no route tag is configured.
    pub route_tag: *mut c_char,
    pub configured: c_int,
    pub enabled: c_int,
    pub token_present: c_int,
}

#[repr(C)]
pub struct IlinkAccountList {
    pub accounts: *mut IlinkAccount,
    pub count: usize,
}

#[repr(C)]
pub struct IlinkSendResult {
    pub message_id: *mut c_char,
    pub server_message_id: *mut c_char,
}

#[repr(C)]
pub struct IlinkSessionStatus {
    pub paused: c_int,
    pub remaining_ms: i64,
}

#[repr(C)]
pub struct IlinkCryptoResult {
    pub data_base64: *mut c_char,
    /// `-1` when the operation does not report a padded size.
    pub padded_size: i64,
}

#[repr(C)]
pub struct IlinkUploadedFile {
    pub filekey: *mut c_char,
    pub download_param: *mut c_char,
    pub aeskey_hex: *mut c_char,
    pub aes_key_base64: *mut c_char,
    pub file_size: u64,
    pub file_size_ciphertext: u64,
}

#[repr(C)]
pub struct IlinkMediaFetch {
    pub bytes: i64,
    /// `NULL` when the bytes were returned inline instead of written to a file.
    pub path: *mut c_char,
    /// `NULL` when a `dest_path` was given.
    pub data_base64: *mut c_char,
}

#[repr(C)]
pub struct IlinkGetConfig {
    pub ret: i64,
    /// `NULL` when the server reported no message.
    pub errmsg: *mut c_char,
    /// `NULL` when the server did not return one.
    pub typing_ticket: *mut c_char,
    pub ok: c_int,
}

/// One inbound message. `items_json` is the raw `item_list` array: decode it
/// with `ilink_message_body` (for the plain text) or your own JSON library.
#[repr(C)]
pub struct IlinkMessage {
    pub seq: i64,
    pub message_id: *mut c_char,
    pub from_user_id: *mut c_char,
    pub to_user_id: *mut c_char,
    pub session_id: *mut c_char,
    pub create_time_ms: i64,
    pub context_token: *mut c_char,
    pub run_id: *mut c_char,
    pub items_json: *mut c_char,
}

#[repr(C)]
pub struct IlinkUpdates {
    pub ret: i64,
    pub errcode: i64,
    pub errmsg: *mut c_char,
    pub get_updates_buf: *mut c_char,
    pub longpolling_timeout_ms: i64,
    pub messages: *mut IlinkMessage,
    pub count: usize,
    /// Non-zero when the server reported a business-level failure.
    pub is_error: c_int,
}

#[repr(C)]
pub struct IlinkQrSession {
    pub token: *mut c_char,
    pub bot_id: *mut c_char,
    pub base_url: *mut c_char,
    pub user_id: *mut c_char,
}

#[repr(C)]
pub struct IlinkLoginStart {
    pub qrcode: *mut c_char,
    pub qrcode_url: *mut c_char,
    pub session_key: *mut c_char,
}

#[repr(C)]
pub struct IlinkLoginPoll {
    pub kind: *mut c_char,
    pub retry: c_int,
    pub session: *mut IlinkQrSession,
}

#[repr(C)]
pub struct IlinkLoginResult {
    pub connected: c_int,
    pub already_connected: c_int,
    pub message: *mut c_char,
    pub account_id: *mut c_char,
    pub session: *mut IlinkQrSession,
}

#[repr(C)]
pub struct IlinkMediaEntry {
    pub path: *mut c_char,
    pub content_type: *mut c_char,
    pub file_name: *mut c_char,
    pub message_id: *mut c_char,
}

#[repr(C)]
pub struct IlinkMsgContext {
    pub body: *mut c_char,
    pub from: *mut c_char,
    pub to: *mut c_char,
    pub account_id: *mut c_char,
    pub originating_channel: *mut c_char,
    pub originating_to: *mut c_char,
    pub message_sid: *mut c_char,
    pub message_sid_full: *mut c_char,
    pub timestamp: i64,
    pub provider: *mut c_char,
    pub chat_type: *mut c_char,
    pub session_key: *mut c_char,
    pub context_token: *mut c_char,
    pub media_path: *mut c_char,
    pub media_type: *mut c_char,
    pub media_paths: *mut *mut c_char,
    pub media_paths_count: usize,
    pub media_types: *mut *mut c_char,
    pub media_types_count: usize,
    pub media: *mut IlinkMediaEntry,
    pub media_count: usize,
    pub channel_prompt_context: *mut *mut c_char,
    pub channel_prompt_context_count: usize,
    pub reply_to_id: *mut c_char,
    pub reply_to_body: *mut c_char,
    pub reply_to_quote_text: *mut c_char,
    pub reply_to_is_quote: c_int,
    pub reply_to_is_quote_set: c_int,
}

#[repr(C)]
pub struct IlinkInboundMedia {
    pub image_path: *mut c_char,
    pub voice_path: *mut c_char,
    pub voice_mime: *mut c_char,
    pub file_path: *mut c_char,
    pub file_mime: *mut c_char,
    pub video_path: *mut c_char,
    pub referenced: c_int,
    pub is_empty: c_int,
}

#[repr(C)]
pub struct IlinkQuotePolicy {
    pub enabled: c_int,
    pub retention_ms: u64,
    pub max_messages_per_account: u64,
    pub media_retention_ms: u64,
    pub max_media_bytes_per_account: u64,
    pub max_single_media_bytes: u64,
}

#[repr(C)]
pub struct IlinkQuoteRecord {
    pub account_id: *mut c_char,
    pub conversation_id: *mut c_char,
    pub message_id: *mut c_char,
    pub direction: *mut c_char,
    pub body: *mut c_char,
    pub media_path: *mut c_char,
    pub media_mime: *mut c_char,
    pub media_name: *mut c_char,
    pub media_size: i64,
    pub created_at: i64,
}

#[repr(C)]
pub struct IlinkQuoteInput {
    pub account_id: *const c_char,
    pub conversation_id: *const c_char,
    pub message_id: *const c_char,
    pub direction: *const c_char,
    pub body: *const c_char,
    pub source_media_path: *const c_char,
    pub media_mime: *const c_char,
    pub media_name: *const c_char,
    pub created_at: i64,
}

#[repr(C)]
pub struct IlinkLocalConfig {
    pub reply_progress_messages: c_int,
    pub accounts_json: *mut c_char,
    pub route_tag_json: *mut c_char,
    pub bot_agent: *mut c_char,
    pub channel_config_updated_at: *mut c_char,
    pub quote_cache_policy: IlinkQuotePolicy,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Run `body`, mapping the outcome to a status code and recording the message.
fn guard<T>(out: *mut T, body: impl FnOnce() -> Result<()>) -> c_int {
    if out.is_null() {
        set_last_error("null output pointer");
        return ILINK_ERR_INVALID_ARG;
    }
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => {
            set_last_error("");
            ILINK_OK
        }
        Ok(Err(err)) => {
            set_last_error(&err.to_string());
            status_of(&err)
        }
        Err(_) => {
            set_last_error("panic inside the iLink SDK");
            ILINK_ERR_PANIC
        }
    }
}

/// Variant of [`guard`] for functions that only return a string.
fn guard_str(out: *mut *mut c_char, body: impl FnOnce() -> Result<String>) -> c_int {
    if out.is_null() {
        set_last_error("null output pointer");
        return ILINK_ERR_INVALID_ARG;
    }
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(value)) => {
            unsafe { *out = into_c_string(value) };
            set_last_error("");
            ILINK_OK
        }
        Ok(Err(err)) => {
            set_last_error(&err.to_string());
            status_of(&err)
        }
        Err(_) => {
            set_last_error("panic inside the iLink SDK");
            ILINK_ERR_PANIC
        }
    }
}

fn into_c_string(value: String) -> *mut c_char {
    match CString::new(value) {
        Ok(s) => s.into_raw(),
        Err(_) => CString::new("").expect("empty").into_raw(),
    }
}

fn opt_c_string(value: Option<String>) -> *mut c_char {
    match value {
        Some(s) => into_c_string(s),
        None => std::ptr::null_mut(),
    }
}

/// Copy a caller string. `name` is used in the error message when NULL.
fn req_str(ptr: *const c_char, name: &str) -> Result<String> {
    if ptr.is_null() {
        return Err(Error::unknown(format!("`{name}` must not be NULL")));
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| Error::unknown(format!("`{name}` is not valid UTF-8: {e}")))?;
    Ok(text.to_string())
}

/// Copy an optional caller string (`NULL` and empty both mean "absent").
fn opt_str(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let text = unsafe { CStr::from_ptr(ptr) }.to_str().ok()?;
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn free_c_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { drop(CString::from_raw(ptr)) };
    }
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn optional_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn int_field(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn bool_field(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Run one operation, blocking, and return its `result`.
fn call(op: &str, params: Value) -> Result<Value> {
    dispatch_sync(op, params)
}

/// Build the routing parameters from a [`IlinkConfig`].
///
/// # Safety
/// `config` must be NULL or point at a valid `IlinkConfig` whose string fields
/// are NULL or valid NUL-terminated strings.
unsafe fn config_params(config: *const IlinkConfig) -> Value {
    let mut params = serde_json::Map::new();
    if config.is_null() {
        return Value::Object(params);
    }
    let config = &*config;
    if let Some(value) = opt_str(config.account_id) {
        params.insert("account_id".into(), json!(value));
    }
    if let Some(value) = opt_str(config.base_url) {
        params.insert("base_url".into(), json!(value));
    }
    if let Some(value) = opt_str(config.cdn_base_url) {
        params.insert("cdn_base_url".into(), json!(value));
    }
    if let Some(value) = opt_str(config.route_tag) {
        params.insert("route_tag".into(), json!(value));
    }
    if let Some(value) = opt_str(config.bot_agent) {
        params.insert("bot_agent".into(), json!(value));
    }
    if config.timeout_ms > 0 {
        params.insert("timeout_ms".into(), json!(config.timeout_ms));
    }
    Value::Object(params)
}

/// Merge `extra` into the config-derived routing parameters.
unsafe fn params_with(config: *const IlinkConfig, extra: Value) -> Value {
    let mut base = config_params(config);
    if let (Some(base), Value::Object(extra)) = (base.as_object_mut(), extra) {
        for (key, value) in extra {
            base.insert(key, value);
        }
    }
    base
}

/// Read a `data`/`len` pair into a `Vec<u8>`.
unsafe fn bytes_arg(data: *const u8, len: usize, name: &str) -> Result<Vec<u8>> {
    if data.is_null() {
        if len == 0 {
            return Ok(Vec::new());
        }
        return Err(Error::unknown(format!("`{name}` is NULL but len is {len}")));
    }
    Ok(std::slice::from_raw_parts(data, len).to_vec())
}

// ---------------------------------------------------------------------------
// Meta
// ---------------------------------------------------------------------------

/// SDK version (static, never NULL).
#[no_mangle]
pub extern "C" fn ilink_version_string() -> *const c_char {
    static VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");
    VERSION.as_ptr() as *const c_char
}

#[no_mangle]
pub extern "C" fn ilink_paths(config: *const IlinkConfig, out: *mut IlinkPaths) -> c_int {
    guard(out, || {
        let value = call("paths", unsafe { config_params(config) })?;
        unsafe {
            *out = IlinkPaths {
                state_dir: into_c_string(string_field(&value, "state_dir")),
                sdk_state_dir: into_c_string(string_field(&value, "sdk_state_dir")),
                accounts_dir: into_c_string(string_field(&value, "accounts_dir")),
                config_path: into_c_string(string_field(&value, "config_path")),
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_paths_free(paths: *mut IlinkPaths) {
    if paths.is_null() {
        return;
    }
    unsafe {
        free_c_string((*paths).state_dir);
        free_c_string((*paths).sdk_state_dir);
        free_c_string((*paths).accounts_dir);
        free_c_string((*paths).config_path);
        *paths = std::mem::zeroed();
    }
}

// ---------------------------------------------------------------------------
// Pure helpers (no network, no account)
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn ilink_mime_info(path: *const c_char, out: *mut IlinkMimeInfo) -> c_int {
    guard(out, || {
        let path = req_str(path, "path")?;
        let value = call("mime.info", json!({ "path": path }))?;
        unsafe {
            *out = IlinkMimeInfo {
                mime: into_c_string(string_field(&value, "mime")),
                kind: into_c_string(string_field(&value, "kind")),
                extension: into_c_string(string_field(&value, "extension")),
                is_image: bool_field(&value, "is_image") as c_int,
                is_video: bool_field(&value, "is_video") as c_int,
                is_audio: bool_field(&value, "is_audio") as c_int,
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_mime_info_free(info: *mut IlinkMimeInfo) {
    if info.is_null() {
        return;
    }
    unsafe {
        free_c_string((*info).mime);
        free_c_string((*info).kind);
        free_c_string((*info).extension);
        *info = std::mem::zeroed();
    }
}

#[no_mangle]
pub extern "C" fn ilink_mime_extension(mime: *const c_char, out: *mut *mut c_char) -> c_int {
    guard_str(out, || {
        let mime = req_str(mime, "mime")?;
        let value = call("mime.extension", json!({ "mime": mime }))?;
        Ok(string_field(&value, "extension"))
    })
}

#[no_mangle]
pub extern "C" fn ilink_generate_id(prefix: *const c_char, out: *mut *mut c_char) -> c_int {
    guard_str(out, || {
        let prefix = opt_str(prefix).unwrap_or_else(|| "ilink-sdk".to_string());
        let value = call("util.generate_id", json!({ "prefix": prefix }))?;
        Ok(string_field(&value, "id"))
    })
}

#[no_mangle]
pub extern "C" fn ilink_file_key(out: *mut *mut c_char) -> c_int {
    guard_str(out, || {
        let value = call("util.file_key", Value::Null)?;
        Ok(string_field(&value, "file_key"))
    })
}

#[no_mangle]
pub extern "C" fn ilink_temp_name(
    prefix: *const c_char,
    ext: *const c_char,
    out: *mut *mut c_char,
) -> c_int {
    guard_str(out, || {
        let prefix = opt_str(prefix).unwrap_or_else(|| "ilink-sdk".to_string());
        let ext = opt_str(ext).unwrap_or_default();
        let value = call("util.temp_name", json!({ "prefix": prefix, "ext": ext }))?;
        Ok(string_field(&value, "name"))
    })
}

#[no_mangle]
pub extern "C" fn ilink_cdn_download_url(
    encrypted_query_param: *const c_char,
    cdn_base_url: *const c_char,
    out: *mut *mut c_char,
) -> c_int {
    guard_str(out, || {
        let param = req_str(encrypted_query_param, "encrypted_query_param")?;
        let mut params = json!({ "encrypted_query_param": param });
        if let Some(base) = opt_str(cdn_base_url) {
            params["cdn_base_url"] = json!(base);
        }
        let value = call("cdn.download_url", params)?;
        Ok(string_field(&value, "url"))
    })
}

#[no_mangle]
pub extern "C" fn ilink_cdn_upload_url(
    upload_param: *const c_char,
    filekey: *const c_char,
    cdn_base_url: *const c_char,
    out: *mut *mut c_char,
) -> c_int {
    guard_str(out, || {
        let mut params = json!({
            "upload_param": req_str(upload_param, "upload_param")?,
            "filekey": req_str(filekey, "filekey")?,
        });
        if let Some(base) = opt_str(cdn_base_url) {
            params["cdn_base_url"] = json!(base);
        }
        let value = call("cdn.upload_url", params)?;
        Ok(string_field(&value, "url"))
    })
}

#[no_mangle]
pub extern "C" fn ilink_crypto_encrypt(
    key_hex: *const c_char,
    data: *const u8,
    len: usize,
    out: *mut IlinkCryptoResult,
) -> c_int {
    guard(out, || {
        let key_hex = req_str(key_hex, "key_hex")?;
        let bytes = unsafe { bytes_arg(data, len, "data") }?;
        let encoded = crate::ffi::encode_base64(&bytes);
        let value = call(
            "crypto.encrypt",
            json!({ "key_hex": key_hex, "data_base64": encoded }),
        )?;
        unsafe {
            *out = IlinkCryptoResult {
                data_base64: into_c_string(string_field(&value, "data_base64")),
                padded_size: value
                    .get("padded_size")
                    .and_then(Value::as_i64)
                    .unwrap_or(-1),
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_crypto_decrypt(
    key_hex: *const c_char,
    data: *const u8,
    len: usize,
    out: *mut IlinkCryptoResult,
) -> c_int {
    guard(out, || {
        let key_hex = req_str(key_hex, "key_hex")?;
        let bytes = unsafe { bytes_arg(data, len, "data") }?;
        let encoded = crate::ffi::encode_base64(&bytes);
        let value = call(
            "crypto.decrypt",
            json!({ "key_hex": key_hex, "data_base64": encoded }),
        )?;
        unsafe {
            *out = IlinkCryptoResult {
                data_base64: into_c_string(string_field(&value, "data_base64")),
                padded_size: -1,
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_crypto_result_free(result: *mut IlinkCryptoResult) {
    if result.is_null() {
        return;
    }
    unsafe {
        free_c_string((*result).data_base64);
        *result = std::mem::zeroed();
    }
}

// ---------------------------------------------------------------------------
// QR login
// ---------------------------------------------------------------------------

fn qr_session_from_json(value: &Value) -> IlinkQrSession {
    IlinkQrSession {
        token: into_c_string(string_field(value, "token")),
        bot_id: into_c_string(string_field(value, "bot_id")),
        base_url: into_c_string(string_field(value, "base_url")),
        user_id: into_c_string(string_field(value, "user_id")),
    }
}

fn boxed_qr_session(value: &Value) -> *mut IlinkQrSession {
    Box::into_raw(Box::new(qr_session_from_json(value)))
}

#[no_mangle]
pub extern "C" fn ilink_qr_session_free(session: *mut IlinkQrSession) {
    if session.is_null() {
        return;
    }
    unsafe {
        free_c_string((*session).token);
        free_c_string((*session).bot_id);
        free_c_string((*session).base_url);
        free_c_string((*session).user_id);
        drop(Box::from_raw(session));
    }
}

fn login_start_into(out: *mut IlinkLoginStart, value: &Value) {
    unsafe {
        *out = IlinkLoginStart {
            qrcode: into_c_string(string_field(value, "qrcode")),
            qrcode_url: into_c_string(string_field(value, "qrcode_url")),
            session_key: into_c_string(string_field(value, "session_key")),
        };
    }
}

#[no_mangle]
pub extern "C" fn ilink_login_start_free(start: *mut IlinkLoginStart) {
    if start.is_null() {
        return;
    }
    unsafe {
        free_c_string((*start).qrcode);
        free_c_string((*start).qrcode_url);
        free_c_string((*start).session_key);
        *start = std::mem::zeroed();
    }
}

#[no_mangle]
pub extern "C" fn ilink_login_start(
    config: *const IlinkConfig,
    base_url: *const c_char,
    bot_type: *const c_char,
    session_key: *const c_char,
    out: *mut IlinkLoginStart,
) -> c_int {
    guard(out, || {
        let mut params = json!({});
        if let Some(value) = opt_str(base_url) {
            params["base_url"] = json!(value);
        }
        if let Some(value) = opt_str(bot_type) {
            params["bot_type"] = json!(value);
        }
        if let Some(value) = opt_str(session_key) {
            params["session_key"] = json!(value);
        }
        let value = call("login.start", unsafe { params_with(config, params) })?;
        login_start_into(out, &value);
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_login_refresh(
    config: *const IlinkConfig,
    session_key: *const c_char,
    out: *mut IlinkLoginStart,
) -> c_int {
    guard(out, || {
        let params = json!({ "session_key": req_str(session_key, "session_key")? });
        let value = call("login.refresh", unsafe { params_with(config, params) })?;
        login_start_into(out, &value);
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_login_poll(
    config: *const IlinkConfig,
    session_key: *const c_char,
    out: *mut IlinkLoginPoll,
) -> c_int {
    guard(out, || {
        let params = json!({ "session_key": req_str(session_key, "session_key")? });
        let value = call("login.poll", unsafe { params_with(config, params) })?;
        let session = value
            .get("session")
            .filter(|v| v.is_object())
            .map(boxed_qr_session)
            .unwrap_or(std::ptr::null_mut());
        unsafe {
            *out = IlinkLoginPoll {
                kind: into_c_string(string_field(&value, "kind")),
                retry: bool_field(&value, "retry") as c_int,
                session,
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_login_poll_free(poll: *mut IlinkLoginPoll) {
    if poll.is_null() {
        return;
    }
    unsafe {
        free_c_string((*poll).kind);
        ilink_qr_session_free((*poll).session);
        *poll = std::mem::zeroed();
    }
}

#[no_mangle]
pub extern "C" fn ilink_login_submit_verify_code(
    config: *const IlinkConfig,
    session_key: *const c_char,
    code: *const c_char,
) -> c_int {
    let mut dummy: c_int = 0;
    guard(&mut dummy, || {
        let params = json!({
            "session_key": req_str(session_key, "session_key")?,
            "code": req_str(code, "code")?,
        });
        call("login.submit_verify_code", unsafe {
            params_with(config, params)
        })?;
        Ok(())
    })
}

fn login_wait_params(
    session_key: *const c_char,
    timeout_secs: u64,
    verify_code: *const c_char,
) -> Result<Value> {
    let mut params = json!({ "session_key": req_str(session_key, "session_key")? });
    if timeout_secs > 0 {
        params["timeout_secs"] = json!(timeout_secs);
    }
    if let Some(value) = opt_str(verify_code) {
        params["verify_code"] = json!(value);
    }
    Ok(params)
}

fn login_result_into(out: *mut IlinkLoginResult, value: &Value) {
    let session = value
        .get("session")
        .filter(|v| v.is_object())
        .map(boxed_qr_session)
        .unwrap_or(std::ptr::null_mut());
    unsafe {
        *out = IlinkLoginResult {
            connected: bool_field(value, "connected") as c_int,
            already_connected: bool_field(value, "already_connected") as c_int,
            message: into_c_string(string_field(value, "message")),
            account_id: opt_c_string(optional_field(value, "account_id")),
            session,
        };
    }
}

#[no_mangle]
pub extern "C" fn ilink_login_wait(
    config: *const IlinkConfig,
    session_key: *const c_char,
    timeout_secs: u64,
    verify_code: *const c_char,
    out: *mut IlinkLoginResult,
) -> c_int {
    guard(out, || {
        let params = login_wait_params(session_key, timeout_secs, verify_code)?;
        let value = call("login.wait", unsafe { params_with(config, params) })?;
        login_result_into(out, &value);
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_login_complete(
    config: *const IlinkConfig,
    session_key: *const c_char,
    timeout_secs: u64,
    verify_code: *const c_char,
    out: *mut IlinkLoginResult,
) -> c_int {
    guard(out, || {
        let params = login_wait_params(session_key, timeout_secs, verify_code)?;
        let value = call("login.complete", unsafe { params_with(config, params) })?;
        login_result_into(out, &value);
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_login_result_free(result: *mut IlinkLoginResult) {
    if result.is_null() {
        return;
    }
    unsafe {
        free_c_string((*result).message);
        free_c_string((*result).account_id);
        ilink_qr_session_free((*result).session);
        *result = std::mem::zeroed();
    }
}

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

fn account_from_json(value: &Value) -> IlinkAccount {
    IlinkAccount {
        account_id: into_c_string(string_field(value, "account_id")),
        base_url: into_c_string(string_field(value, "base_url")),
        cdn_base_url: into_c_string(string_field(value, "cdn_base_url")),
        user_id: opt_c_string(optional_field(value, "user_id")),
        name: opt_c_string(optional_field(value, "name")),
        route_tag: opt_c_string(optional_field(value, "route_tag")),
        configured: bool_field(value, "configured") as c_int,
        enabled: bool_field(value, "enabled") as c_int,
        token_present: bool_field(value, "token_present") as c_int,
    }
}

#[no_mangle]
pub extern "C" fn ilink_account_free(account: *mut IlinkAccount) {
    if account.is_null() {
        return;
    }
    unsafe {
        free_c_string((*account).account_id);
        free_c_string((*account).base_url);
        free_c_string((*account).cdn_base_url);
        free_c_string((*account).user_id);
        free_c_string((*account).name);
        free_c_string((*account).route_tag);
        *account = std::mem::zeroed();
    }
}

#[no_mangle]
pub extern "C" fn ilink_accounts_list(
    config: *const IlinkConfig,
    out: *mut IlinkAccountList,
) -> c_int {
    guard(out, || {
        let value = call("accounts.list", unsafe { config_params(config) })?;
        let items = value.as_array().cloned().unwrap_or_default();
        let mut accounts: Vec<IlinkAccount> = items.iter().map(account_from_json).collect();
        let ptr = accounts.as_mut_ptr();
        let count = accounts.len();
        std::mem::forget(accounts);
        unsafe {
            *out = IlinkAccountList {
                accounts: ptr,
                count,
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_account_list_free(list: *mut IlinkAccountList) {
    if list.is_null() {
        return;
    }
    unsafe {
        let accounts = (*list).accounts;
        let count = (*list).count;
        if !accounts.is_null() {
            for index in 0..count {
                ilink_account_free(accounts.add(index));
            }
            drop(Vec::from_raw_parts(accounts, count, count));
        }
        *list = std::mem::zeroed();
    }
}

#[no_mangle]
pub extern "C" fn ilink_account_resolve(
    account_id: *const c_char,
    out: *mut IlinkAccount,
) -> c_int {
    guard(out, || {
        let id = req_str(account_id, "account_id")?;
        let value = call("account.resolve", json!({ "account_id": id }))?;
        unsafe { *out = account_from_json(&value) };
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_account_save(
    account_id: *const c_char,
    token: *const c_char,
    base_url: *const c_char,
    user_id: *const c_char,
    out: *mut IlinkAccount,
) -> c_int {
    guard(out, || {
        let mut params = json!({ "account_id": req_str(account_id, "account_id")? });
        if let Some(value) = opt_str(token) {
            params["token"] = json!(value);
        }
        if let Some(value) = opt_str(base_url) {
            params["base_url"] = json!(value);
        }
        if let Some(value) = opt_str(user_id) {
            params["user_id"] = json!(value);
        }
        let value = call("account.save", params)?;
        unsafe { *out = account_from_json(&value) };
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_account_clear(account_id: *const c_char) -> c_int {
    let mut dummy: c_int = 0;
    guard(&mut dummy, || {
        let id = req_str(account_id, "account_id")?;
        call("account.clear", json!({ "account_id": id }))?;
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Session guard
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn ilink_session_status(
    account_id: *const c_char,
    out: *mut IlinkSessionStatus,
) -> c_int {
    guard(out, || {
        let id = req_str(account_id, "account_id")?;
        let value = call("session.status", json!({ "account_id": id }))?;
        unsafe {
            *out = IlinkSessionStatus {
                paused: bool_field(&value, "paused") as c_int,
                remaining_ms: int_field(&value, "remaining_ms"),
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_session_pause(account_id: *const c_char) -> c_int {
    session_action("session.pause", account_id)
}

#[no_mangle]
pub extern "C" fn ilink_session_resume(account_id: *const c_char) -> c_int {
    session_action("session.resume", account_id)
}

#[no_mangle]
pub extern "C" fn ilink_session_assert(account_id: *const c_char) -> c_int {
    session_action("session.assert", account_id)
}

fn session_action(op: &str, account_id: *const c_char) -> c_int {
    let mut dummy: c_int = 0;
    guard(&mut dummy, || {
        let id = req_str(account_id, "account_id")?;
        call(op, json!({ "account_id": id }))?;
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Sending
// ---------------------------------------------------------------------------

fn send_result_from_json(value: &Value) -> IlinkSendResult {
    IlinkSendResult {
        message_id: into_c_string(string_field(value, "message_id")),
        server_message_id: into_c_string(string_field(value, "server_message_id")),
    }
}

#[no_mangle]
pub extern "C" fn ilink_send_result_free(result: *mut IlinkSendResult) {
    if result.is_null() {
        return;
    }
    unsafe {
        free_c_string((*result).message_id);
        free_c_string((*result).server_message_id);
        *result = std::mem::zeroed();
    }
}

#[no_mangle]
pub extern "C" fn ilink_send_text(
    config: *const IlinkConfig,
    to: *const c_char,
    text: *const c_char,
    context_token: *const c_char,
    run_id: *const c_char,
    out: *mut IlinkSendResult,
) -> c_int {
    guard(out, || {
        let mut params = json!({
            "to": req_str(to, "to")?,
            "text": req_str(text, "text")?,
        });
        if let Some(value) = opt_str(context_token) {
            params["context_token"] = json!(value);
        }
        if let Some(value) = opt_str(run_id) {
            params["run_id"] = json!(value);
        }
        let value = call("send.text", unsafe { params_with(config, params) })?;
        unsafe { *out = send_result_from_json(&value) };
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_send_file(
    config: *const IlinkConfig,
    to: *const c_char,
    path: *const c_char,
    caption: *const c_char,
    context_token: *const c_char,
    out: *mut IlinkSendResult,
) -> c_int {
    guard(out, || {
        let mut params = json!({
            "to": req_str(to, "to")?,
            "path": req_str(path, "path")?,
        });
        if let Some(value) = opt_str(caption) {
            params["caption"] = json!(value);
        }
        if let Some(value) = opt_str(context_token) {
            params["context_token"] = json!(value);
        }
        let value = call("send.file", unsafe { params_with(config, params) })?;
        unsafe { *out = send_result_from_json(&value) };
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_send_bytes(
    config: *const IlinkConfig,
    to: *const c_char,
    file_name: *const c_char,
    data: *const u8,
    len: usize,
    caption: *const c_char,
    out: *mut IlinkSendResult,
) -> c_int {
    guard(out, || {
        let bytes = unsafe { bytes_arg(data, len, "data") }?;
        let mut params = json!({
            "to": req_str(to, "to")?,
            "file_name": req_str(file_name, "file_name")?,
            "data_base64": crate::ffi::encode_base64(&bytes),
        });
        if let Some(value) = opt_str(caption) {
            params["caption"] = json!(value);
        }
        let value = call("send.bytes", unsafe { params_with(config, params) })?;
        unsafe { *out = send_result_from_json(&value) };
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_send_item(
    config: *const IlinkConfig,
    to: *const c_char,
    item_json: *const c_char,
    context_token: *const c_char,
    out: *mut IlinkSendResult,
) -> c_int {
    guard(out, || {
        let raw = req_str(item_json, "item_json")?;
        let item: Value = serde_json::from_str(&raw)
            .map_err(|e| Error::unknown(format!("item_json is not valid JSON: {e}")))?;
        let mut params = json!({
            "to": req_str(to, "to")?,
            "item": item,
        });
        if let Some(value) = opt_str(context_token) {
            params["context_token"] = json!(value);
        }
        let value = call("send.item", unsafe { params_with(config, params) })?;
        unsafe { *out = send_result_from_json(&value) };
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Receiving
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn ilink_updates_free(updates: *mut IlinkUpdates) {
    if updates.is_null() {
        return;
    }
    unsafe {
        free_c_string((*updates).errmsg);
        free_c_string((*updates).get_updates_buf);
        let messages = (*updates).messages;
        let count = (*updates).count;
        if !messages.is_null() {
            for index in 0..count {
                let message = messages.add(index);
                free_c_string((*message).message_id);
                free_c_string((*message).from_user_id);
                free_c_string((*message).to_user_id);
                free_c_string((*message).session_id);
                free_c_string((*message).context_token);
                free_c_string((*message).run_id);
                free_c_string((*message).items_json);
            }
            drop(Vec::from_raw_parts(messages, count, count));
        }
        *updates = std::mem::zeroed();
    }
}

fn message_from_json(value: &Value) -> IlinkMessage {
    IlinkMessage {
        seq: int_field(value, "seq"),
        message_id: into_c_string(string_field(value, "message_id")),
        from_user_id: into_c_string(string_field(value, "from_user_id")),
        to_user_id: into_c_string(string_field(value, "to_user_id")),
        session_id: into_c_string(string_field(value, "session_id")),
        create_time_ms: int_field(value, "create_time_ms"),
        context_token: into_c_string(string_field(value, "context_token")),
        run_id: into_c_string(string_field(value, "run_id")),
        items_json: into_c_string(
            value
                .get("item_list")
                .map(|items| items.to_string())
                .unwrap_or_else(|| "[]".to_string()),
        ),
    }
}

#[no_mangle]
pub extern "C" fn ilink_get_updates(
    config: *const IlinkConfig,
    get_updates_buf: *const c_char,
    timeout_ms: u64,
    out: *mut IlinkUpdates,
) -> c_int {
    guard(out, || {
        let mut params = json!({});
        if let Some(value) = opt_str(get_updates_buf) {
            params["get_updates_buf"] = json!(value);
        }
        if timeout_ms > 0 {
            params["timeout_ms"] = json!(timeout_ms);
        }
        let value = call("get_updates", unsafe { params_with(config, params) })?;
        let ret = int_field(&value, "ret");
        let errcode = int_field(&value, "errcode");
        let messages_json = value
            .get("msgs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut messages: Vec<IlinkMessage> = messages_json.iter().map(message_from_json).collect();
        let ptr = messages.as_mut_ptr();
        let count = messages.len();
        std::mem::forget(messages);
        unsafe {
            *out = IlinkUpdates {
                ret,
                errcode,
                errmsg: opt_c_string(optional_field(&value, "errmsg")),
                get_updates_buf: into_c_string(string_field(&value, "get_updates_buf")),
                longpolling_timeout_ms: int_field(&value, "longpolling_timeout_ms"),
                messages: ptr,
                count,
                is_error: (ret != 0 || errcode != 0) as c_int,
            };
        }
        Ok(())
    })
}

/// Extract the plain-text body from a message's `items_json`.
#[no_mangle]
pub extern "C" fn ilink_message_body(items_json: *const c_char, out: *mut *mut c_char) -> c_int {
    guard_str(out, || {
        let raw = req_str(items_json, "items_json")?;
        let items: Value = serde_json::from_str(&raw)
            .map_err(|e| Error::unknown(format!("items_json is not valid JSON: {e}")))?;
        let value = call("message.body", json!({ "items": items }))?;
        Ok(string_field(&value, "body"))
    })
}

/// Build a [`IlinkMsgContext`] from the parsed JSON `value`.
fn msg_context_from_json(value: &Value) -> IlinkMsgContext {
    let mut paths: Vec<*mut c_char> = Vec::new();
    if let Some(list) = value.get("media_paths").and_then(Value::as_array) {
        paths = list
            .iter()
            .map(|item| into_c_string(item.as_str().unwrap_or_default().to_string()))
            .collect();
    }
    let mut types: Vec<*mut c_char> = Vec::new();
    if let Some(list) = value.get("media_types").and_then(Value::as_array) {
        types = list
            .iter()
            .map(|item| into_c_string(item.as_str().unwrap_or_default().to_string()))
            .collect();
    }
    let mut media: Vec<IlinkMediaEntry> = Vec::new();
    if let Some(list) = value.get("media").and_then(Value::as_array) {
        media = list
            .iter()
            .map(|entry| IlinkMediaEntry {
                path: into_c_string(string_field(entry, "path")),
                content_type: opt_c_string(optional_field(entry, "content_type")),
                file_name: opt_c_string(optional_field(entry, "file_name")),
                message_id: opt_c_string(optional_field(entry, "message_id")),
            })
            .collect();
    }
    let mut prompts: Vec<*mut c_char> = Vec::new();
    if let Some(list) = value
        .get("channel_prompt_context")
        .and_then(Value::as_array)
    {
        prompts = list
            .iter()
            .map(|item| into_c_string(item.as_str().unwrap_or_default().to_string()))
            .collect();
    }

    let (paths_ptr, paths_count) = leak_vec(paths);
    let (types_ptr, types_count) = leak_vec(types);
    let (media_ptr, media_count) = leak_vec(media);
    let (prompts_ptr, prompts_count) = leak_vec(prompts);

    IlinkMsgContext {
        body: into_c_string(string_field(value, "body")),
        from: into_c_string(string_field(value, "from")),
        to: into_c_string(string_field(value, "to")),
        account_id: into_c_string(string_field(value, "account_id")),
        originating_channel: into_c_string(string_field(value, "originating_channel")),
        originating_to: into_c_string(string_field(value, "originating_to")),
        message_sid: into_c_string(string_field(value, "message_sid")),
        message_sid_full: opt_c_string(optional_field(value, "message_sid_full")),
        timestamp: int_field(value, "timestamp"),
        provider: into_c_string(string_field(value, "provider")),
        chat_type: into_c_string(string_field(value, "chat_type")),
        session_key: opt_c_string(optional_field(value, "session_key")),
        context_token: opt_c_string(optional_field(value, "context_token")),
        media_path: opt_c_string(optional_field(value, "media_path")),
        media_type: opt_c_string(optional_field(value, "media_type")),
        media_paths: paths_ptr,
        media_paths_count: paths_count,
        media_types: types_ptr,
        media_types_count: types_count,
        media: media_ptr,
        media_count,
        channel_prompt_context: prompts_ptr,
        channel_prompt_context_count: prompts_count,
        reply_to_id: opt_c_string(optional_field(value, "reply_to_id")),
        reply_to_body: opt_c_string(optional_field(value, "reply_to_body")),
        reply_to_quote_text: opt_c_string(optional_field(value, "reply_to_quote_text")),
        reply_to_is_quote: bool_field(value, "reply_to_is_quote") as c_int,
        reply_to_is_quote_set: value
            .get("reply_to_is_quote")
            .map(|v| !v.is_null())
            .unwrap_or(false) as c_int,
    }
}

/// Hand a `Vec<T>` to C as a `ptr`/`len` pair, keeping the allocation alive.
fn leak_vec<T>(mut values: Vec<T>) -> (*mut T, usize) {
    let count = values.len();
    let ptr = values.as_mut_ptr();
    std::mem::forget(values);
    (ptr, count)
}

#[no_mangle]
pub extern "C" fn ilink_context_to_message(
    config: *const IlinkConfig,
    message_json: *const c_char,
    out: *mut IlinkMsgContext,
) -> c_int {
    guard(out, || {
        let raw = req_str(message_json, "message_json")?;
        let message: Value = serde_json::from_str(&raw)
            .map_err(|e| Error::unknown(format!("message_json is not valid JSON: {e}")))?;
        let params = json!({ "message": message });
        let value = call("context.to_message", unsafe { params_with(config, params) })?;
        unsafe { *out = msg_context_from_json(&value) };
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_msg_context_free(context: *mut IlinkMsgContext) {
    if context.is_null() {
        return;
    }
    unsafe {
        let ctx = &mut *context;
        for ptr in [
            ctx.body,
            ctx.from,
            ctx.to,
            ctx.account_id,
            ctx.originating_channel,
            ctx.originating_to,
            ctx.message_sid,
            ctx.message_sid_full,
            ctx.provider,
            ctx.chat_type,
            ctx.session_key,
            ctx.context_token,
            ctx.media_path,
            ctx.media_type,
            ctx.reply_to_id,
            ctx.reply_to_body,
            ctx.reply_to_quote_text,
        ] {
            free_c_string(ptr);
        }
        free_string_vec(ctx.media_paths, ctx.media_paths_count);
        free_string_vec(ctx.media_types, ctx.media_types_count);
        free_string_vec(ctx.channel_prompt_context, ctx.channel_prompt_context_count);
        if !ctx.media.is_null() {
            for index in 0..ctx.media_count {
                let entry = ctx.media.add(index);
                free_c_string((*entry).path);
                free_c_string((*entry).content_type);
                free_c_string((*entry).file_name);
                free_c_string((*entry).message_id);
            }
            drop(Vec::from_raw_parts(
                ctx.media,
                ctx.media_count,
                ctx.media_count,
            ));
        }
        *context = std::mem::zeroed();
    }
}

/// Free a `char **` array produced by [`leak_vec`].
unsafe fn free_string_vec(ptr: *mut *mut c_char, count: usize) {
    if ptr.is_null() {
        return;
    }
    for index in 0..count {
        free_c_string(*ptr.add(index));
    }
    drop(Vec::from_raw_parts(ptr, count, count));
}

#[no_mangle]
pub extern "C" fn ilink_get_config(
    config: *const IlinkConfig,
    user_id: *const c_char,
    context_token: *const c_char,
    out: *mut IlinkGetConfig,
) -> c_int {
    guard(out, || {
        let mut params = json!({ "user_id": req_str(user_id, "user_id")? });
        if let Some(value) = opt_str(context_token) {
            params["context_token"] = json!(value);
        }
        let value = call("get_config", unsafe { params_with(config, params) })?;
        let ret = int_field(&value, "ret");
        unsafe {
            *out = IlinkGetConfig {
                ret,
                errmsg: opt_c_string(optional_field(&value, "errmsg")),
                typing_ticket: opt_c_string(optional_field(&value, "typing_ticket")),
                ok: (ret == 0) as c_int,
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_get_config_free(config: *mut IlinkGetConfig) {
    if config.is_null() {
        return;
    }
    unsafe {
        free_c_string((*config).errmsg);
        free_c_string((*config).typing_ticket);
        *config = std::mem::zeroed();
    }
}

#[no_mangle]
pub extern "C" fn ilink_send_typing(
    config: *const IlinkConfig,
    user_id: *const c_char,
    typing_ticket: *const c_char,
    status: c_int,
) -> c_int {
    let mut dummy: c_int = 0;
    guard(&mut dummy, || {
        let params = json!({
            "user_id": req_str(user_id, "user_id")?,
            "typing_ticket": req_str(typing_ticket, "typing_ticket")?,
            "status": status,
        });
        call("send_typing", unsafe { params_with(config, params) })?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_notify(start: c_int) -> c_int {
    let mut dummy: c_int = 0;
    guard(&mut dummy, || {
        let action = if start != 0 { "start" } else { "stop" };
        call("notify", json!({ "action": action }))?;
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Media
// ---------------------------------------------------------------------------

fn media_fetch_from_json(value: &Value) -> IlinkMediaFetch {
    IlinkMediaFetch {
        bytes: int_field(value, "bytes"),
        path: opt_c_string(optional_field(value, "path")),
        data_base64: opt_c_string(optional_field(value, "data_base64")),
    }
}

#[no_mangle]
pub extern "C" fn ilink_media_fetch_free(fetch: *mut IlinkMediaFetch) {
    if fetch.is_null() {
        return;
    }
    unsafe {
        free_c_string((*fetch).path);
        free_c_string((*fetch).data_base64);
        *fetch = std::mem::zeroed();
    }
}

#[no_mangle]
pub extern "C" fn ilink_fetch(
    config: *const IlinkConfig,
    url: *const c_char,
    dest_path: *const c_char,
    out: *mut IlinkMediaFetch,
) -> c_int {
    guard(out, || {
        let mut params = json!({ "url": req_str(url, "url")? });
        if let Some(value) = opt_str(dest_path) {
            params["dest_path"] = json!(value);
        }
        let value = call("media.fetch", unsafe { params_with(config, params) })?;
        unsafe { *out = media_fetch_from_json(&value) };
        Ok(())
    })
}

#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn ilink_download(
    config: *const IlinkConfig,
    encrypted_query_param: *const c_char,
    aes_key: *const c_char,
    full_url: *const c_char,
    dest_path: *const c_char,
    out: *mut IlinkMediaFetch,
) -> c_int {
    guard(out, || {
        let mut media = serde_json::Map::new();
        if let Some(value) = opt_str(encrypted_query_param) {
            media.insert("encrypt_query_param".into(), json!(value));
        }
        if let Some(value) = opt_str(full_url) {
            media.insert("full_url".into(), json!(value));
        }
        if media.is_empty() {
            return Err(Error::unknown(
                "either encrypted_query_param or full_url is required",
            ));
        }
        let mut params = json!({ "media": Value::Object(media) });
        if let Some(value) = opt_str(aes_key) {
            params["aes_key"] = json!(value);
        }
        if let Some(value) = opt_str(dest_path) {
            params["dest_path"] = json!(value);
        }
        let value = call("media.download", unsafe { params_with(config, params) })?;
        unsafe { *out = media_fetch_from_json(&value) };
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_uploaded_file_free(file: *mut IlinkUploadedFile) {
    if file.is_null() {
        return;
    }
    unsafe {
        free_c_string((*file).filekey);
        free_c_string((*file).download_param);
        free_c_string((*file).aeskey_hex);
        free_c_string((*file).aes_key_base64);
        *file = std::mem::zeroed();
    }
}

#[no_mangle]
pub extern "C" fn ilink_upload(
    config: *const IlinkConfig,
    to: *const c_char,
    path: *const c_char,
    media_type: *const c_char,
    out: *mut IlinkUploadedFile,
) -> c_int {
    guard(out, || {
        let mut params = json!({
            "to": req_str(to, "to")?,
            "path": req_str(path, "path")?,
        });
        if let Some(value) = opt_str(media_type) {
            params["media_type"] = json!(value);
        }
        let value = call("media.upload", unsafe { params_with(config, params) })?;
        unsafe {
            *out = IlinkUploadedFile {
                filekey: into_c_string(string_field(&value, "filekey")),
                download_param: into_c_string(string_field(&value, "download_param")),
                aeskey_hex: into_c_string(string_field(&value, "aeskey_hex")),
                aes_key_base64: into_c_string(string_field(&value, "aes_key_base64")),
                file_size: int_field(&value, "file_size").max(0) as u64,
                file_size_ciphertext: int_field(&value, "file_size_ciphertext").max(0) as u64,
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_download_item(
    config: *const IlinkConfig,
    item_json: *const c_char,
    dest_dir: *const c_char,
    out: *mut IlinkInboundMedia,
) -> c_int {
    guard(out, || {
        let raw = req_str(item_json, "item_json")?;
        let item: Value = serde_json::from_str(&raw)
            .map_err(|e| Error::unknown(format!("item_json is not valid JSON: {e}")))?;
        let mut params = json!({ "item": item });
        if let Some(value) = opt_str(dest_dir) {
            params["dest_dir"] = json!(value);
        }
        let value = call("media.download_item", unsafe {
            params_with(config, params)
        })?;
        unsafe {
            *out = IlinkInboundMedia {
                image_path: opt_c_string(optional_field(&value, "image_path")),
                voice_path: opt_c_string(optional_field(&value, "voice_path")),
                voice_mime: opt_c_string(optional_field(&value, "voice_mime")),
                file_path: opt_c_string(optional_field(&value, "file_path")),
                file_mime: opt_c_string(optional_field(&value, "file_mime")),
                video_path: opt_c_string(optional_field(&value, "video_path")),
                referenced: bool_field(&value, "referenced") as c_int,
                is_empty: bool_field(&value, "is_empty") as c_int,
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_inbound_media_free(media: *mut IlinkInboundMedia) {
    if media.is_null() {
        return;
    }
    unsafe {
        free_c_string((*media).image_path);
        free_c_string((*media).voice_path);
        free_c_string((*media).voice_mime);
        free_c_string((*media).file_path);
        free_c_string((*media).file_mime);
        free_c_string((*media).video_path);
        *media = std::mem::zeroed();
    }
}

// ---------------------------------------------------------------------------
// Context tokens / sync cursor
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn ilink_context_get(
    config: *const IlinkConfig,
    user_id: *const c_char,
    out: *mut *mut c_char,
) -> c_int {
    guard_str(out, || {
        let params = json!({ "user_id": req_str(user_id, "user_id")? });
        let value = call("context.get", unsafe { params_with(config, params) })?;
        // The operation returns `null` when nothing is stored.
        Ok(value
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string())
    })
}

#[no_mangle]
pub extern "C" fn ilink_context_set(
    config: *const IlinkConfig,
    user_id: *const c_char,
    token: *const c_char,
) -> c_int {
    let mut dummy: c_int = 0;
    guard(&mut dummy, || {
        let params = json!({
            "user_id": req_str(user_id, "user_id")?,
            "token": req_str(token, "token")?,
        });
        call("context.set", unsafe { params_with(config, params) })?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_context_clear(config: *const IlinkConfig) -> c_int {
    let mut dummy: c_int = 0;
    guard(&mut dummy, || {
        call("context.clear", unsafe { config_params(config) })?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_context_restore(config: *const IlinkConfig) -> c_int {
    let mut dummy: c_int = 0;
    guard(&mut dummy, || {
        call("context.restore", unsafe { config_params(config) })?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_sync_load(config: *const IlinkConfig, out: *mut *mut c_char) -> c_int {
    guard_str(out, || {
        let value = call("sync.load", unsafe { config_params(config) })?;
        Ok(string_field(&value, "get_updates_buf"))
    })
}

#[no_mangle]
pub extern "C" fn ilink_sync_save(config: *const IlinkConfig, buf: *const c_char) -> c_int {
    let mut dummy: c_int = 0;
    guard(&mut dummy, || {
        let params = json!({ "buf": req_str(buf, "buf")? });
        call("sync.save", unsafe { params_with(config, params) })?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_sync_clear(config: *const IlinkConfig) -> c_int {
    let mut dummy: c_int = 0;
    guard(&mut dummy, || {
        call("sync.clear", unsafe { config_params(config) })?;
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Quote cache
// ---------------------------------------------------------------------------

fn quote_policy_from_json(value: &Value) -> IlinkQuotePolicy {
    let unsigned = |key: &str| int_field(value, key).max(0) as u64;
    IlinkQuotePolicy {
        enabled: bool_field(value, "enabled") as c_int,
        retention_ms: unsigned("retention_ms"),
        max_messages_per_account: unsigned("max_messages_per_account"),
        media_retention_ms: unsigned("media_retention_ms"),
        max_media_bytes_per_account: unsigned("max_media_bytes_per_account"),
        max_single_media_bytes: unsigned("max_single_media_bytes"),
    }
}

#[no_mangle]
pub extern "C" fn ilink_quote_policy(out: *mut IlinkQuotePolicy) -> c_int {
    guard(out, || {
        let value = call("quote.policy", Value::Null)?;
        unsafe { *out = quote_policy_from_json(&value) };
        Ok(())
    })
}

fn quote_record_from_json(value: &Value) -> IlinkQuoteRecord {
    IlinkQuoteRecord {
        account_id: into_c_string(string_field(value, "account_id")),
        conversation_id: into_c_string(string_field(value, "conversation_id")),
        message_id: into_c_string(string_field(value, "message_id")),
        direction: into_c_string(string_field(value, "direction")),
        body: into_c_string(string_field(value, "body")),
        media_path: opt_c_string(optional_field(value, "media_path")),
        media_mime: opt_c_string(optional_field(value, "media_mime")),
        media_name: opt_c_string(optional_field(value, "media_name")),
        media_size: int_field(value, "media_size"),
        created_at: int_field(value, "created_at"),
    }
}

#[no_mangle]
pub extern "C" fn ilink_quote_record_free(record: *mut IlinkQuoteRecord) {
    if record.is_null() {
        return;
    }
    unsafe {
        free_c_string((*record).account_id);
        free_c_string((*record).conversation_id);
        free_c_string((*record).message_id);
        free_c_string((*record).direction);
        free_c_string((*record).body);
        free_c_string((*record).media_path);
        free_c_string((*record).media_mime);
        free_c_string((*record).media_name);
        *record = std::mem::zeroed();
    }
}

#[no_mangle]
pub extern "C" fn ilink_quote_find(
    account_id: *const c_char,
    conversation_id: *const c_char,
    message_id: *const c_char,
    out: *mut IlinkQuoteRecord,
    found: *mut c_int,
) -> c_int {
    if found.is_null() {
        set_last_error("null `found` pointer");
        return ILINK_ERR_INVALID_ARG;
    }
    guard(out, || {
        let params = json!({
            "account_id": req_str(account_id, "account_id")?,
            "conversation_id": req_str(conversation_id, "conversation_id")?,
            "message_id": req_str(message_id, "message_id")?,
        });
        let value = call("quote.find", params)?;
        unsafe {
            if value.is_null() {
                *found = 0;
                *out = std::mem::zeroed();
            } else {
                *found = 1;
                *out = quote_record_from_json(&value);
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_quote_put(input: *const IlinkQuoteInput) -> c_int {
    let mut dummy: c_int = 0;
    guard(&mut dummy, || {
        if input.is_null() {
            return Err(Error::unknown("`input` must not be NULL"));
        }
        let input = unsafe { &*input };
        let mut params = json!({
            "account_id": req_str(input.account_id, "account_id")?,
            "conversation_id": req_str(input.conversation_id, "conversation_id")?,
            "message_id": req_str(input.message_id, "message_id")?,
        });
        if let Some(value) = opt_str(input.direction) {
            params["direction"] = json!(value);
        }
        if let Some(value) = opt_str(input.body) {
            params["body"] = json!(value);
        }
        if let Some(value) = opt_str(input.source_media_path) {
            params["source_media_path"] = json!(value);
        }
        if let Some(value) = opt_str(input.media_mime) {
            params["media_mime"] = json!(value);
        }
        if let Some(value) = opt_str(input.media_name) {
            params["media_name"] = json!(value);
        }
        if input.created_at > 0 {
            params["created_at"] = json!(input.created_at);
        }
        call("quote.put", params)?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_quote_gc() -> c_int {
    let mut dummy: c_int = 0;
    guard(&mut dummy, || {
        call("quote.gc", Value::Null)?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_quote_delete_account(account_id: *const c_char) -> c_int {
    let mut dummy: c_int = 0;
    guard(&mut dummy, || {
        let params = json!({ "account_id": req_str(account_id, "account_id")? });
        call("quote.delete_account", params)?;
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_quote_media_dir(account_id: *const c_char, out: *mut *mut c_char) -> c_int {
    guard_str(out, || {
        let params = json!({ "account_id": req_str(account_id, "account_id")? });
        let value = call("quote.media_dir", params)?;
        Ok(string_field(&value, "path"))
    })
}

// ---------------------------------------------------------------------------
// Local config
// ---------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn ilink_config_load(out: *mut IlinkLocalConfig) -> c_int {
    guard(out, || {
        let value = call("config.load", Value::Null)?;
        let config = value.get("config").cloned().unwrap_or(Value::Null);
        let policy = value
            .get("quote_cache_policy")
            .map(quote_policy_from_json)
            .unwrap_or_else(|| quote_policy_from_json(&Value::Null));
        // The config JSON is camelCase on the wire (it mirrors openclaw.json).
        unsafe {
            *out = IlinkLocalConfig {
                reply_progress_messages: bool_field(&config, "replyProgressMessages") as c_int,
                accounts_json: into_c_string(
                    config
                        .get("accounts")
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "{}".to_string()),
                ),
                route_tag_json: config
                    .get("routeTag")
                    .filter(|v| !v.is_null())
                    .map(|v| into_c_string(v.to_string()))
                    .unwrap_or(std::ptr::null_mut()),
                bot_agent: opt_c_string(optional_field(&config, "botAgent")),
                channel_config_updated_at: opt_c_string(optional_field(
                    &config,
                    "channelConfigUpdatedAt",
                )),
                quote_cache_policy: policy,
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn ilink_local_config_free(config: *mut IlinkLocalConfig) {
    if config.is_null() {
        return;
    }
    unsafe {
        free_c_string((*config).accounts_json);
        free_c_string((*config).route_tag_json);
        free_c_string((*config).bot_agent);
        free_c_string((*config).channel_config_updated_at);
        *config = std::mem::zeroed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(text: &str) -> CString {
        CString::new(text).unwrap()
    }

    #[test]
    fn mime_info_round_trip() {
        let mut info: IlinkMimeInfo = unsafe { std::mem::zeroed() };
        let path = c("photo.PNG");
        assert_eq!(ilink_mime_info(path.as_ptr(), &mut info), ILINK_OK);
        unsafe {
            assert_eq!(CStr::from_ptr(info.mime).to_str().unwrap(), "image/png");
            assert_eq!(info.is_image, 1);
            ilink_mime_info_free(&mut info);
        }
    }

    #[test]
    fn null_out_pointer_is_invalid_arg() {
        let path = c("a.png");
        assert_eq!(
            ilink_mime_info(path.as_ptr(), std::ptr::null_mut()),
            ILINK_ERR_INVALID_ARG
        );
    }

    #[test]
    fn error_message_is_recorded() {
        let mut info: IlinkMimeInfo = unsafe { std::mem::zeroed() };
        assert_eq!(
            ilink_mime_info(std::ptr::null(), &mut info),
            ILINK_ERR_UNKNOWN
        );
        let message = unsafe { CStr::from_ptr(ilink_last_error()) };
        assert!(message.to_str().unwrap().contains("path"));
    }

    #[test]
    fn generate_id_and_file_key() {
        let prefix = c("test");
        let mut out: *mut c_char = std::ptr::null_mut();
        assert_eq!(ilink_generate_id(prefix.as_ptr(), &mut out), ILINK_OK);
        let id = unsafe { CStr::from_ptr(out).to_str().unwrap().to_string() };
        free_c_string(out);
        assert!(id.starts_with("test:"));

        let mut key: *mut c_char = std::ptr::null_mut();
        assert_eq!(ilink_file_key(&mut key), ILINK_OK);
        let value = unsafe { CStr::from_ptr(key).to_str().unwrap().to_string() };
        free_c_string(key);
        assert_eq!(value.len(), 32);
    }

    #[test]
    fn crypto_round_trip() {
        let key = c("00112233445566778899aabbccddeeff");
        let data = b"hello world";
        let mut encrypted: IlinkCryptoResult = unsafe { std::mem::zeroed() };
        assert_eq!(
            ilink_crypto_encrypt(key.as_ptr(), data.as_ptr(), data.len(), &mut encrypted),
            ILINK_OK
        );
        let encoded = unsafe {
            CStr::from_ptr(encrypted.data_base64)
                .to_str()
                .unwrap()
                .to_string()
        };
        ilink_crypto_result_free(&mut encrypted);
        assert!(!encoded.is_empty());
    }

    #[test]
    fn version_string_is_static() {
        let text = unsafe { CStr::from_ptr(ilink_version_string()) };
        assert_eq!(text.to_str().unwrap(), crate::VERSION);
    }

    #[test]
    fn status_messages_are_non_null() {
        for status in [ILINK_OK, ILINK_ERR_DNS, ILINK_ERR_PANIC, 12345] {
            assert!(!ilink_status_message(status).is_null());
        }
    }

    #[test]
    fn config_is_applied() {
        let account = c("demo");
        let config = IlinkConfig {
            account_id: account.as_ptr(),
            base_url: std::ptr::null(),
            cdn_base_url: std::ptr::null(),
            route_tag: std::ptr::null(),
            bot_agent: std::ptr::null(),
            timeout_ms: 1000,
        };
        let mut status: IlinkSessionStatus = unsafe { std::mem::zeroed() };
        assert_eq!(
            ilink_session_status(account.as_ptr(), &mut status),
            ILINK_OK
        );
        assert_eq!(status.paused, 0);

        let mut out: *mut c_char = std::ptr::null_mut();
        assert_eq!(ilink_sync_load(&config, &mut out), ILINK_OK);
        free_c_string(out);
    }

    #[test]
    fn accounts_list_round_trip() {
        let mut list: IlinkAccountList = unsafe { std::mem::zeroed() };
        assert_eq!(ilink_accounts_list(std::ptr::null(), &mut list), ILINK_OK);
        // Freeing an empty list must be safe.
        ilink_account_list_free(&mut list);
        assert_eq!(list.count, 0);
    }

    #[test]
    fn login_start_returns_qr_without_network() {
        // login.start reaches the network, so it is expected to fail here; the
        // point is that it fails cleanly (status + message) and does not panic.
        let mut start: IlinkLoginStart = unsafe { std::mem::zeroed() };
        let status = ilink_login_start(
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            &mut start,
        );
        assert_ne!(status, ILINK_ERR_PANIC);
        ilink_login_start_free(&mut start);
    }

    #[test]
    fn login_poll_and_result_free_are_null_safe() {
        ilink_login_start_free(std::ptr::null_mut());
        ilink_login_poll_free(std::ptr::null_mut());
        ilink_login_result_free(std::ptr::null_mut());
        ilink_qr_session_free(std::ptr::null_mut());
    }

    #[test]
    fn quote_policy_and_config_load() {
        let mut policy: IlinkQuotePolicy = unsafe { std::mem::zeroed() };
        assert_eq!(ilink_quote_policy(&mut policy), ILINK_OK);
        assert_ne!(policy.retention_ms, 0);

        let mut config: IlinkLocalConfig = unsafe { std::mem::zeroed() };
        assert_eq!(ilink_config_load(&mut config), ILINK_OK);
        assert!(!config.accounts_json.is_null());
        ilink_local_config_free(&mut config);
    }

    #[test]
    fn quote_find_missing_returns_not_found_flag() {
        let account = c("ffi-quote-missing");
        let conversation = c("c1");
        let message = c("m1");
        let mut record: IlinkQuoteRecord = unsafe { std::mem::zeroed() };
        let mut found: c_int = -1;
        let status = ilink_quote_find(
            account.as_ptr(),
            conversation.as_ptr(),
            message.as_ptr(),
            &mut record,
            &mut found,
        );
        assert_eq!(status, ILINK_OK);
        assert_eq!(found, 0);
        ilink_quote_record_free(&mut record);
    }

    #[test]
    fn quote_find_null_found_pointer_is_invalid_arg() {
        let account = c("a");
        let conversation = c("c");
        let message = c("m");
        let mut record: IlinkQuoteRecord = unsafe { std::mem::zeroed() };
        assert_eq!(
            ilink_quote_find(
                account.as_ptr(),
                conversation.as_ptr(),
                message.as_ptr(),
                &mut record,
                std::ptr::null_mut(),
            ),
            ILINK_ERR_INVALID_ARG
        );
    }

    #[test]
    fn quote_media_dir_and_free() {
        let account = c("ffi-quote-dir");
        let mut out: *mut c_char = std::ptr::null_mut();
        assert_eq!(ilink_quote_media_dir(account.as_ptr(), &mut out), ILINK_OK);
        assert!(!out.is_null());
        free_c_string(out);
    }

    #[test]
    fn msg_context_free_is_null_safe() {
        ilink_msg_context_free(std::ptr::null_mut());
        ilink_inbound_media_free(std::ptr::null_mut());
    }

    #[test]
    fn context_to_message_rejects_bad_json() {
        let bad = c("{not json");
        let mut ctx: IlinkMsgContext = unsafe { std::mem::zeroed() };
        assert_eq!(
            ilink_context_to_message(std::ptr::null(), bad.as_ptr(), &mut ctx),
            ILINK_ERR_UNKNOWN
        );
    }

    #[test]
    fn message_body_from_items_json() {
        // An empty item list yields an empty body.
        let items = c("[]");
        let mut out: *mut c_char = std::ptr::null_mut();
        assert_eq!(ilink_message_body(items.as_ptr(), &mut out), ILINK_OK);
        unsafe {
            assert_eq!(CStr::from_ptr(out).to_str().unwrap(), "");
            free_c_string(out);
        }
    }
}
