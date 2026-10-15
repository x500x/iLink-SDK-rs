//! C ABI surface for the iLink SDK.
//!
//! Building this crate produces a shared library (`ilink_sdk.dll` on Windows,
//! `libilink_sdk.so` on Linux, `libilink_sdk.dylib` on macOS) and a static
//! library (`ilink_sdk.lib` / `libilink_sdk.a`) in addition to the Rust rlib.
//!
//! Rather than exporting one C function per SDK operation (there are dozens of
//! async, generic and borrowed-argument APIs), the whole SDK is reachable
//! through a single JSON-in / JSON-out entry point:
//!
//! ```c
//! char *ilink_call(const char *op, const char *params_json); // caller frees
//! ```
//!
//! The return value is always a JSON object:
//!
//! ```json
//! { "ok": true,  "result": <op-specific> }
//! { "ok": false, "error": "message", "kind": "dns" }
//! ```
//!
//! `op` selects the operation (see [`dispatch`] and `include/ilink_sdk.h` for
//! the full list). `params_json` is an operation-specific JSON object; pass
//! `NULL` or `"{}"` for operations that take no parameters.
//!
//! The runtime is created lazily on first use, so the exported functions are
//! safe to call from any thread that is not already inside a Tokio runtime.

use std::ffi::{c_char, CStr, CString};
use std::future::Future;
use std::sync::OnceLock;

use serde_json::{json, Value};

use crate::api::session_guard;
use crate::api::types::{CdnMedia, GetUpdatesReq, MessageItem, UploadMediaType};
use crate::api::{WeixinApiClient, WeixinApiOptions};
use crate::cdn::download::{download_and_decrypt, download_plain, fetch_cdn_bytes, MediaSink};
use crate::cdn::upload::{
    upload_file_attachment, upload_image, upload_media, upload_video, UploadedFile,
};
use crate::cdn::{aes_ecb_padded_size, decrypt_aes_ecb, encrypt_aes_ecb};
use crate::client::{WeixinClient, WeixinClientBuilder};
use crate::error::{Error, Result};
use crate::messaging::auth::{QrLogin, QrSession, QrWaitResult};
use crate::messaging::context::ContextTokenStore;
use crate::messaging::outbound::SendOpts;
use crate::messaging::quote_cache::{
    account_media_dir_name, Direction, QuoteMessageInput, QuoteStore,
};
use crate::messaging::typing::{send_typing, Typing};
use crate::storage::{accounts, config, state_dir, sync_buf};

/// Lazily-created Tokio runtime shared by every FFI call.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("ilink-ffi")
            .build()
            .expect("failed to build Tokio runtime for ilink FFI")
    })
}

fn block_on<F: Future>(fut: F) -> F::Output {
    runtime().block_on(fut)
}

// ---------------------------------------------------------------------------
// Exported symbols
// ---------------------------------------------------------------------------

/// Run one SDK operation.
///
/// Returns a newly allocated NUL-terminated JSON string that the caller must
/// release with [`ilink_string_free`]. Never returns NULL for valid input.
///
/// # Safety contract (C callers)
/// `op` must be a valid NUL-terminated C string. `params_json` may be NULL or
/// a valid NUL-terminated C string containing a JSON object. The function is
/// safe to call from C; the allow below silences the raw-pointer lint because
/// the contract cannot be expressed in Rust's type system across the FFI
/// boundary.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn ilink_call(op: *const c_char, params_json: *const c_char) -> *mut c_char {
    let outcome = std::panic::catch_unwind(|| -> Result<Value> {
        let op = cstr_to_str(op)?;
        let params = parse_params(params_json)?;
        dispatch_sync(op, params)
    });

    let payload = match outcome {
        Ok(Ok(value)) => json!({ "ok": true, "result": value }),
        Ok(Err(err)) => {
            json!({ "ok": false, "error": err.to_string(), "kind": err.kind().as_str() })
        }
        Err(_) => json!({ "ok": false, "error": "panic inside ilink_call", "kind": "panic" }),
    };
    to_c_string(payload.to_string())
}

/// Library version as a NUL-terminated string (caller frees).
#[no_mangle]
pub extern "C" fn ilink_version() -> *mut c_char {
    to_c_string(crate::VERSION.to_string())
}

/// Free a string returned by [`ilink_call`] or [`ilink_version`].
///
/// # Safety contract (C callers)
/// `ptr` must be a pointer previously returned by this library, or NULL.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn ilink_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(ptr));
    }
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Execute an operation, blocking on the internal runtime.
///
/// This is the Rust-friendly entry point used by tests and by the CLI when it
/// wants structured JSON; the C ABI wraps it in [`ilink_call`].
pub fn dispatch_sync(op: &str, params: Value) -> Result<Value> {
    block_on(dispatch(op, params))
}

/// Execute an operation asynchronously.
pub async fn dispatch(op: &str, params: Value) -> Result<Value> {
    match op {
        // -- meta ---------------------------------------------------------
        "version" => Ok(json!(crate::VERSION)),
        "paths" => Ok(json!({
            "state_dir": state_dir::resolve_state_dir().to_string_lossy(),
            "sdk_state_dir": state_dir::sdk_state_dir().to_string_lossy(),
            "accounts_dir": state_dir::accounts_dir().to_string_lossy(),
            "config_path": config::resolve_config_path().to_string_lossy(),
        })),

        // -- accounts -----------------------------------------------------
        "accounts.list" => {
            let mut out = Vec::new();
            for id in accounts::list_indexed_account_ids() {
                if let Ok(a) = accounts::resolve_account(&id) {
                    out.push(account_json(&a));
                }
            }
            Ok(Value::Array(out))
        }
        "account.resolve" => {
            let id = req_str(&params, "account_id")?;
            Ok(account_json(&accounts::resolve_account(id)?))
        }
        "account.save" => {
            let id = req_str(&params, "account_id")?;
            let update = accounts::AccountUpdate {
                token: opt_string(&params, "token"),
                base_url: opt_string(&params, "base_url"),
                user_id: opt_string(&params, "user_id"),
            };
            accounts::save_account(id, &update)?;
            accounts::register_account_id(id)?;
            Ok(account_json(&accounts::resolve_account(id)?))
        }
        "account.clear" => {
            let raw = req_str(&params, "account_id")?;
            let id = accounts::normalize_account_id(raw);
            let mut ctx = ContextTokenStore::new();
            ctx.clear_account(&id);
            sync_buf::clear_for_account(&id);
            if let Some(mut store) = QuoteStore::open(quote_policy()?)? {
                store.delete_account(&id);
            }
            accounts::clear_account(&id)?;
            accounts::unregister_account_id(&id)?;
            Ok(json!({ "account_id": id }))
        }

        // -- QR login -----------------------------------------------------
        "login.start" => {
            let api = build_api_client(&params)?;
            let mut qr = QrLogin::new(&api);
            if let Some(bot_type) = opt_str(&params, "bot_type") {
                qr = qr.with_bot_type(bot_type);
            }
            let start = qr.start(opt_str(&params, "session_key")).await?;
            Ok(json!({
                "qrcode": start.qrcode,
                "qrcode_url": start.qrcode_url,
                "session_key": start.session_key,
            }))
        }
        "login.poll" => {
            let key = req_str(&params, "session_key")?;
            let api = build_api_client(&params)?;
            let outcome = QrLogin::new(&api).poll(key).await?;
            Ok(poll_outcome_json(&outcome))
        }
        "login.submit_verify_code" => {
            let key = req_str(&params, "session_key")?;
            let code = req_str(&params, "code")?;
            let api = build_api_client(&params)?;
            QrLogin::new(&api).submit_verify_code(key, code);
            Ok(json!({ "submitted": true }))
        }
        "login.refresh" => {
            let key = req_str(&params, "session_key")?;
            let api = build_api_client(&params)?;
            let start = QrLogin::new(&api).refresh(key).await?;
            Ok(json!({
                "qrcode": start.qrcode,
                "qrcode_url": start.qrcode_url,
                "session_key": start.session_key,
            }))
        }
        "login.wait" => {
            let key = req_str(&params, "session_key")?;
            let timeout = params
                .get("timeout_secs")
                .and_then(Value::as_u64)
                .unwrap_or(480);
            let code = opt_string(&params, "verify_code");
            let api = build_api_client(&params)?;
            let result = QrLogin::new(&api)
                .wait(
                    key,
                    std::time::Duration::from_secs(timeout.max(1)),
                    |_retry| {
                        let code = code.clone();
                        async move { Ok(code.clone().unwrap_or_default()) }
                    },
                    |_| async {},
                )
                .await?;
            Ok(wait_result_json(&result))
        }
        "login.complete" => {
            // login.wait + persist the resulting credentials in one call.
            let key = req_str(&params, "session_key")?;
            let timeout = params
                .get("timeout_secs")
                .and_then(Value::as_u64)
                .unwrap_or(480);
            let code = opt_string(&params, "verify_code");
            let api = build_api_client(&params)?;
            let result = QrLogin::new(&api)
                .wait(
                    key,
                    std::time::Duration::from_secs(timeout.max(1)),
                    |_retry| {
                        let code = code.clone();
                        async move { Ok(code.clone().unwrap_or_default()) }
                    },
                    |_| async {},
                )
                .await?;
            match result.session.clone() {
                Some(session) => {
                    let account_id = save_session(&session, true)?;
                    Ok(json!({
                        "connected": true,
                        "already_connected": false,
                        "message": result.message,
                        "account_id": account_id,
                        "session": session_json(&session),
                    }))
                }
                None => Ok(json!({
                    "connected": false,
                    "already_connected": result.already_connected,
                    "message": result.message,
                    "session": Value::Null,
                })),
            }
        }

        // -- outbound -----------------------------------------------------
        "send.text" => {
            let client = build_client(&params).await?;
            let opts = send_opts(&params);
            let result = client
                .sender()
                .send_text(req_str(&params, "to")?, req_str(&params, "text")?, &opts)
                .await?;
            Ok(json!({
                "message_id": result.message_id,
                "server_message_id": result.server_message_id,
            }))
        }
        "send.file" => {
            let client = build_client(&params).await?;
            let opts = send_opts(&params);
            let cdn = client.account().cdn_base_url.clone();
            let path = req_str(&params, "path")?;
            let caption = opt_str(&params, "caption").unwrap_or("");
            let result = client
                .sender()
                .send_file(
                    req_str(&params, "to")?,
                    std::path::Path::new(path),
                    caption,
                    &opts,
                    &cdn,
                )
                .await?;
            Ok(json!({
                "message_id": result.message_id,
                "server_message_id": result.server_message_id,
            }))
        }
        "send.item" => {
            let client = build_client(&params).await?;
            let opts = send_opts(&params);
            let raw = params
                .get("item")
                .cloned()
                .ok_or_else(|| Error::unknown("missing required object param `item`"))?;
            let item: MessageItem = serde_json::from_value(raw)?;
            let result = client
                .sender()
                .send_item(req_str(&params, "to")?, item, &opts)
                .await?;
            Ok(json!({
                "message_id": result.message_id,
                "server_message_id": result.server_message_id,
            }))
        }
        "send.bytes" => {
            let client = build_client(&params).await?;
            let opts = send_opts(&params);
            let cdn = client.account().cdn_base_url.clone();
            let data = decode_base64(req_str(&params, "data_base64")?)?;
            let name = req_str(&params, "file_name")?;
            let caption = opt_str(&params, "caption").unwrap_or("");
            let result = client
                .sender()
                .send_bytes(req_str(&params, "to")?, &data, name, caption, &opts, &cdn)
                .await?;
            Ok(json!({
                "message_id": result.message_id,
                "server_message_id": result.server_message_id,
            }))
        }

        // -- inbound / API -------------------------------------------------
        "get_updates" => {
            let api = build_api_client(&params)?;
            let req = GetUpdatesReq {
                get_updates_buf: opt_string(&params, "get_updates_buf"),
                sync_buf: None,
            };
            let resp = api
                .get_updates(&req, params.get("timeout_ms").and_then(Value::as_u64), None)
                .await?;
            Ok(serde_json::to_value(resp)?)
        }
        "get_config" => {
            let api = build_api_client(&params)?;
            let resp = api
                .get_config(
                    req_str(&params, "user_id")?,
                    opt_str(&params, "context_token"),
                )
                .await?;
            Ok(serde_json::to_value(resp)?)
        }
        "send_typing" => {
            let api = build_api_client(&params)?;
            let status = match opt_str(&params, "status").unwrap_or("typing") {
                "cancel" | "off" | "stop" => Typing::Cancel,
                _ => Typing::Typing,
            };
            send_typing(
                &api,
                req_str(&params, "user_id")?,
                opt_str(&params, "typing_ticket").unwrap_or(""),
                status,
            )
            .await?;
            Ok(json!({ "sent": true }))
        }
        "notify" => {
            let api = build_api_client(&params)?;
            match opt_str(&params, "action").unwrap_or("start") {
                "stop" => {
                    api.notify_stop().await?;
                }
                _ => {
                    api.notify_start().await?;
                }
            }
            Ok(json!({ "notified": true }))
        }
        "context.to_message" => {
            // Parse a raw WeixinMessage into a MsgContext (no media download).
            let raw = params
                .get("message")
                .cloned()
                .ok_or_else(|| Error::unknown("missing required object param `message`"))?;
            let msg: crate::api::types::WeixinMessage = serde_json::from_value(raw)?;
            let account_id = opt_str(&params, "account_id").unwrap_or("");
            let ctx = crate::messaging::context::message_to_context(
                &msg,
                account_id,
                &Default::default(),
            );
            Ok(serde_json::to_value(ctx)?)
        }
        "message.body" => {
            let raw = params
                .get("items")
                .cloned()
                .ok_or_else(|| Error::unknown("missing required array param `items`"))?;
            let items: Vec<MessageItem> = serde_json::from_value(raw)?;
            Ok(json!({ "body": crate::messaging::context::body_from_items(&items) }))
        }

        // -- media ---------------------------------------------------------
        "media.fetch" => {
            let url = req_str(&params, "url")?;
            let label = opt_str(&params, "label").unwrap_or("media.fetch");
            let bytes = fetch_cdn_bytes(url, label).await?;
            let path = opt_str(&params, "dest_path").map(std::path::PathBuf::from);
            if let Some(path) = path {
                if let Some(dir) = path.parent() {
                    tokio::fs::create_dir_all(dir).await?;
                }
                tokio::fs::write(&path, &bytes).await?;
                Ok(json!({ "bytes": bytes.len(), "path": path.to_string_lossy() }))
            } else {
                Ok(json!({ "bytes": bytes.len(), "data_base64": encode_base64(&bytes) }))
            }
        }
        "media.download" => {
            let media: CdnMedia = serde_json::from_value(
                params
                    .get("media")
                    .cloned()
                    .ok_or_else(|| Error::unknown("missing required object param `media`"))?,
            )?;
            let cdn = opt_str(&params, "cdn_base_url").unwrap_or(crate::storage::CDN_BASE_URL);
            let label = opt_str(&params, "label").unwrap_or("media.download");
            let bytes = match opt_str(&params, "aes_key") {
                Some(key) => download_and_decrypt(&media, key, cdn, label).await?,
                None => download_plain(&media, cdn, label).await?,
            };
            write_bytes_out(&params, &bytes).await
        }
        "media.download_item" => {
            let item: MessageItem = serde_json::from_value(
                params
                    .get("item")
                    .cloned()
                    .ok_or_else(|| Error::unknown("missing required object param `item`"))?,
            )?;
            let cdn = opt_str(&params, "cdn_base_url").unwrap_or(crate::storage::CDN_BASE_URL);
            let dest = opt_str(&params, "dest_dir")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            let label = opt_str(&params, "label").unwrap_or("media.download_item");
            let sink = MediaSink::new(dest);
            let media = crate::cdn::download::download_item_media(&item, cdn, &sink, label).await;
            Ok(inbound_media_json(&media))
        }
        "media.upload" => {
            let api = build_api_client(&params)?;
            let path = req_str(&params, "path")?;
            let to = req_str(&params, "to")?;
            let cdn = opt_str(&params, "cdn_base_url").unwrap_or(crate::storage::CDN_BASE_URL);
            let kind = opt_str(&params, "media_type").unwrap_or("file");
            let uploaded = match kind {
                "image" => upload_image(to, std::path::Path::new(path), &api, cdn).await?,
                "video" => upload_video(to, std::path::Path::new(path), &api, cdn).await?,
                "file" => {
                    let name = std::path::Path::new(path)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "file.bin".to_string());
                    upload_file_attachment(to, std::path::Path::new(path), &name, &api, cdn).await?
                }
                other => {
                    let media_type = match other {
                        "voice" => UploadMediaType::Voice,
                        _ => UploadMediaType::File,
                    };
                    upload_media(to, std::path::Path::new(path), media_type, &api, cdn).await?
                }
            };
            Ok(uploaded_json(&uploaded))
        }

        // -- quote cache ---------------------------------------------------
        "quote.policy" => Ok(quote_policy_json(&quote_policy()?)),
        "quote.find" => {
            let store = open_quote_store()?;
            let found = store.find(
                req_str(&params, "account_id")?,
                req_str(&params, "conversation_id")?,
                req_str(&params, "message_id")?,
            );
            Ok(match found {
                Some(record) => quote_record_json(&record),
                None => Value::Null,
            })
        }
        "quote.put" => {
            let mut store = open_quote_store()?;
            let direction = match opt_str(&params, "direction").unwrap_or("inbound") {
                "outbound" => Direction::Outbound,
                _ => Direction::Inbound,
            };
            let input = QuoteMessageInput {
                account_id: req_str(&params, "account_id")?.to_string(),
                conversation_id: req_str(&params, "conversation_id")?.to_string(),
                message_id: req_str(&params, "message_id")?.to_string(),
                direction,
                body: opt_string(&params, "body").unwrap_or_default(),
                source_media_path: opt_string(&params, "source_media_path"),
                media_mime: opt_string(&params, "media_mime"),
                media_name: opt_string(&params, "media_name"),
                created_at: params
                    .get("created_at")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
            };
            store.put(&input)?;
            Ok(json!({ "stored": true }))
        }
        "quote.gc" => {
            let mut store = open_quote_store()?;
            store.gc();
            Ok(json!({ "gc": true }))
        }
        "quote.delete_account" => {
            let mut store = open_quote_store()?;
            store.delete_account(req_str(&params, "account_id")?);
            Ok(json!({ "deleted": true }))
        }
        "quote.media_dir" => {
            let store = open_quote_store()?;
            let dir = store.account_media_dir(req_str(&params, "account_id")?);
            Ok(json!({ "path": dir.to_string_lossy() }))
        }

        // -- context token store ------------------------------------------
        "context.get" => {
            let mut store = ContextTokenStore::new();
            let account_id = req_str(&params, "account_id")?;
            store.restore(account_id);
            Ok(match store.get(account_id, req_str(&params, "user_id")?) {
                Some(token) => json!({ "token": token }),
                None => Value::Null,
            })
        }
        "context.set" => {
            let mut store = ContextTokenStore::new();
            store.set(
                req_str(&params, "account_id")?,
                req_str(&params, "user_id")?,
                req_str(&params, "token")?,
            );
            Ok(json!({ "stored": true }))
        }
        "context.clear" => {
            let mut store = ContextTokenStore::new();
            store.clear_account(req_str(&params, "account_id")?);
            Ok(json!({ "cleared": true }))
        }
        "context.restore" => {
            let mut store = ContextTokenStore::new();
            let count = store.restore(req_str(&params, "account_id")?);
            Ok(json!({ "restored": count }))
        }

        // -- sync cursor ---------------------------------------------------
        "sync.load" => {
            let id = req_str(&params, "account_id")?;
            Ok(json!({ "get_updates_buf": sync_buf::load_for_account(id) }))
        }
        "sync.save" => {
            let id = req_str(&params, "account_id")?;
            let buf = params
                .get("buf")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::unknown("missing required string param `buf`"))?;
            sync_buf::save_for_account(id, buf)?;
            Ok(json!({ "saved": true }))
        }
        "sync.clear" => {
            sync_buf::clear_for_account(req_str(&params, "account_id")?);
            Ok(json!({ "cleared": true }))
        }

        // -- session guard -------------------------------------------------
        "session.status" => {
            let id = req_str(&params, "account_id")?;
            Ok(json!({
                "paused": session_guard::is_session_paused(id),
                "remaining_ms": session_guard::get_remaining_pause_ms(id),
            }))
        }
        "session.pause" => {
            session_guard::pause_session(req_str(&params, "account_id")?);
            Ok(json!({ "paused": true }))
        }
        "session.resume" => {
            session_guard::resume_session(req_str(&params, "account_id")?);
            Ok(json!({ "resumed": true }))
        }
        "session.assert" => {
            session_guard::assert_session_active(req_str(&params, "account_id")?)?;
            Ok(json!({ "active": true }))
        }

        // -- media helpers -------------------------------------------------
        "mime.info" => {
            let path = opt_str(&params, "path")
                .or_else(|| opt_str(&params, "filename"))
                .ok_or_else(|| Error::unknown("missing `path` or `filename`"))?;
            let mime = crate::media::mime::mime_from_filename(path);
            Ok(json!({
                "mime": mime,
                "kind": format!("{:?}", crate::media::mime::MediaKind::from_path(path)),
                "extension": crate::media::mime::extension_of(path),
                "is_image": crate::media::mime::is_image(path),
                "is_video": crate::media::mime::is_video(path),
                "is_audio": crate::media::mime::is_audio(path),
            }))
        }
        "mime.extension" => {
            let mime = req_str(&params, "mime")?;
            Ok(json!({ "extension": crate::media::mime::extension_from_mime(mime) }))
        }

        // -- config --------------------------------------------------------
        "config.load" => {
            let cfg = config::load_local_config();
            let policy = quote_policy()?;
            Ok(json!({
                "config": serde_json::to_value(cfg)?,
                "quote_cache_policy": quote_policy_json(&policy),
            }))
        }

        // -- utilities -----------------------------------------------------
        "util.generate_id" => {
            let prefix = opt_str(&params, "prefix").unwrap_or("ilink-sdk");
            Ok(json!({ "id": crate::util::generate_id(prefix) }))
        }
        "util.file_key" => Ok(json!({ "file_key": crate::util::file_key() })),
        "util.temp_name" => {
            let prefix = opt_str(&params, "prefix").unwrap_or("ilink-sdk");
            let ext = opt_str(&params, "ext").unwrap_or("");
            Ok(json!({ "name": crate::util::temp_file_name(prefix, ext) }))
        }

        // -- CDN URL builders ---------------------------------------------
        "cdn.download_url" => {
            let url = crate::cdn::build_cdn_download_url(
                req_str(&params, "encrypted_query_param")?,
                opt_str(&params, "cdn_base_url").unwrap_or(crate::storage::CDN_BASE_URL),
            )?;
            Ok(json!({ "url": url }))
        }
        "cdn.upload_url" => {
            let url = crate::cdn::build_cdn_upload_url(
                opt_str(&params, "cdn_base_url").unwrap_or(crate::storage::CDN_BASE_URL),
                req_str(&params, "upload_param")?,
                req_str(&params, "filekey")?,
            )?;
            Ok(json!({ "url": url }))
        }

        // -- crypto --------------------------------------------------------
        "crypto.encrypt" => {
            let key = parse_key_hex(req_str(&params, "key_hex")?)?;
            let data = decode_base64(req_str(&params, "data_base64")?)?;
            let out = encrypt_aes_ecb(&data, &key)?;
            Ok(json!({
                "data_base64": encode_base64(&out),
                "padded_size": aes_ecb_padded_size(data.len()),
            }))
        }
        "crypto.decrypt" => {
            let key = parse_key_hex(req_str(&params, "key_hex")?)?;
            let data = decode_base64(req_str(&params, "data_base64")?)?;
            let out = decrypt_aes_ecb(&data, &key)?;
            Ok(json!({ "data_base64": encode_base64(&out) }))
        }

        other => Err(Error::unknown(format!("unknown op `{other}`"))),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn cstr_to_str<'a>(ptr: *const c_char) -> Result<&'a str> {
    if ptr.is_null() {
        return Err(Error::unknown("null pointer"));
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| Error::unknown(format!("invalid utf-8 argument: {e}")))
}

fn parse_params(ptr: *const c_char) -> Result<Value> {
    if ptr.is_null() {
        return Ok(Value::Null);
    }
    let raw = cstr_to_str(ptr)?;
    if raw.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(raw).map_err(Into::into)
}

fn to_c_string(value: String) -> *mut c_char {
    match CString::new(value) {
        Ok(s) => s.into_raw(),
        Err(_) => CString::new(r#"{"ok":false,"error":"result contained a NUL byte"}"#)
            .expect("static string")
            .into_raw(),
    }
}

fn req_str<'a>(params: &'a Value, key: &str) -> Result<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| Error::unknown(format!("missing required string param `{key}`")))
}

fn opt_str<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
}

fn opt_string(params: &Value, key: &str) -> Option<String> {
    opt_str(params, key).map(str::to_string)
}

fn encode_base64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn decode_base64(raw: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(raw.as_bytes())
        .map_err(Into::into)
}

fn parse_key_hex(raw: &str) -> Result<[u8; 16]> {
    let bytes = hex::decode(raw)?;
    if bytes.len() != 16 {
        return Err(Error::unknown(format!(
            "aes key must be 16 bytes of hex, got {} bytes",
            bytes.len()
        )));
    }
    let mut key = [0u8; 16];
    key.copy_from_slice(&bytes);
    Ok(key)
}

async fn write_bytes_out(params: &Value, bytes: &[u8]) -> Result<Value> {
    match opt_str(params, "dest_path") {
        Some(path) => {
            let path = std::path::PathBuf::from(path);
            if let Some(dir) = path.parent() {
                tokio::fs::create_dir_all(dir).await?;
            }
            tokio::fs::write(&path, bytes).await?;
            Ok(json!({ "bytes": bytes.len(), "path": path.to_string_lossy() }))
        }
        None => Ok(json!({
            "bytes": bytes.len(),
            "data_base64": encode_base64(bytes),
        })),
    }
}

fn build_api_client(params: &Value) -> Result<WeixinApiClient> {
    let mut base = opt_str(params, "base_url")
        .unwrap_or(accounts::DEFAULT_BASE_URL)
        .to_string();
    let mut token = opt_string(params, "token");
    if token.is_none() {
        if let Some(id) = opt_str(params, "account_id") {
            if let Ok(data) = accounts::resolve_account(id) {
                if opt_str(params, "base_url").is_none() {
                    base = data.base_url.clone();
                }
                token = data.token.clone();
            }
        }
    }

    let mut opts = WeixinApiOptions::new(base);
    if let Some(token) = token {
        opts = opts.with_token(token);
    }
    if let Some(ms) = params.get("timeout_ms").and_then(Value::as_u64) {
        opts = opts.with_timeout_ms(ms);
    }
    if let Some(tag) = opt_str(params, "route_tag") {
        opts = opts.with_route_tag(tag);
    }
    if let Some(cdn) = opt_str(params, "cdn_base_url") {
        opts = opts.with_cdn_base_url(cdn);
    }
    let mut api = WeixinApiClient::new(opts)?;
    if let Some(agent) = opt_str(params, "bot_agent") {
        api = api.with_bot_agent(agent);
    }
    Ok(api)
}

async fn build_client(params: &Value) -> Result<WeixinClient> {
    let mut builder = WeixinClientBuilder::new()
        .account_id_opt(opt_string(params, "account_id"))
        .base_url_opt(opt_string(params, "base_url"))
        .cdn_base_url_opt(opt_string(params, "cdn_base_url"));
    if let Some(tag) = opt_str(params, "route_tag") {
        builder = builder.route_tag(tag);
    }
    if let Some(agent) = opt_str(params, "bot_agent") {
        builder = builder.bot_agent(agent);
    }
    if let Some(ms) = params.get("timeout_ms").and_then(Value::as_u64) {
        builder = builder.timeout_ms(ms);
    }
    builder.build().await
}

fn send_opts(params: &Value) -> SendOpts {
    SendOpts {
        context_token: opt_string(params, "context_token"),
        run_id: opt_string(params, "run_id"),
    }
}

fn account_json(a: &accounts::StoredAccount) -> Value {
    json!({
        "account_id": a.account_id,
        "base_url": a.base_url,
        "cdn_base_url": a.cdn_base_url,
        "configured": a.configured,
        "enabled": a.enabled,
        "user_id": a.user_id,
        "name": a.name,
        "route_tag": a.route_tag,
        "token_present": a.token.is_some(),
    })
}

fn session_json(s: &QrSession) -> Value {
    json!({
        "token": s.token,
        "bot_id": s.bot_id,
        "base_url": s.base_url,
        "user_id": s.user_id,
    })
}

fn wait_result_json(r: &QrWaitResult) -> Value {
    json!({
        "connected": r.connected,
        "already_connected": r.already_connected,
        "message": r.message,
        "session": r.session.as_ref().map(session_json),
    })
}

fn poll_outcome_json(outcome: &crate::messaging::auth::PollOutcome) -> Value {
    use crate::messaging::auth::PollOutcome;
    match outcome {
        PollOutcome::Continue => json!({ "kind": "continue" }),
        PollOutcome::NeedsVerification { retry } => {
            json!({ "kind": "needs_verification", "retry": retry })
        }
        PollOutcome::Expired => json!({ "kind": "expired" }),
        PollOutcome::VerifyCodeBlocked => json!({ "kind": "verify_code_blocked" }),
        PollOutcome::AlreadyConnected => json!({ "kind": "already_connected" }),
        PollOutcome::Done(session) => {
            json!({ "kind": "done", "session": session_json(session) })
        }
    }
}

fn uploaded_json(u: &UploadedFile) -> Value {
    json!({
        "filekey": u.filekey,
        "download_param": u.download_param,
        "aeskey_hex": u.aeskey_hex,
        "aes_key_base64": u.aes_key_base64(),
        "file_size": u.file_size,
        "file_size_ciphertext": u.file_size_ciphertext,
    })
}

fn inbound_media_json(m: &crate::cdn::download::InboundMedia) -> Value {
    json!({
        "image_path": m.image_path,
        "video_path": m.video_path,
        "file_path": m.file_path,
        "file_mime": m.file_mime,
        "voice_path": m.voice_path,
        "voice_mime": m.voice_mime,
        "referenced": m.referenced,
        "is_empty": m.is_empty(),
    })
}

fn quote_record_json(r: &crate::messaging::quote_cache::QuoteMessageRecord) -> Value {
    json!({
        "account_id": r.account_id,
        "conversation_id": r.conversation_id,
        "message_id": r.message_id,
        "direction": match r.direction {
            Direction::Inbound => "inbound",
            Direction::Outbound => "outbound",
        },
        "body": r.body,
        "media_path": r.media_path,
        "media_mime": r.media_mime,
        "media_name": r.media_name,
        "media_size": r.media_size,
        "created_at": r.created_at,
    })
}

fn quote_policy_json(p: &crate::messaging::quote_cache::QuoteCachePolicy) -> Value {
    json!({
        "enabled": p.enabled,
        "retention_ms": p.retention_ms,
        "max_messages_per_account": p.max_messages_per_account,
        "media_retention_ms": p.media_retention_ms,
        "max_media_bytes_per_account": p.max_media_bytes_per_account,
        "max_single_media_bytes": p.max_single_media_bytes,
    })
}

/// Convert the config-level policy into the quote-cache policy.
fn quote_policy() -> Result<crate::messaging::quote_cache::QuoteCachePolicy> {
    let raw = config::load_local_config().quote_cache.to_policy();
    Ok(crate::messaging::quote_cache::QuoteCachePolicy {
        enabled: raw.enabled,
        retention_ms: raw.retention_ms,
        max_messages_per_account: raw.max_messages_per_account,
        media_retention_ms: raw.media_retention_ms,
        max_media_bytes_per_account: raw.max_media_bytes_per_account,
        max_single_media_bytes: raw.max_single_media_bytes,
    })
}

fn open_quote_store() -> Result<QuoteStore> {
    QuoteStore::open(quote_policy()?)?
        .ok_or_else(|| Error::unknown("quote cache is disabled by config"))
}

/// Persist a successful login. When `clear_stale` is set, drop other accounts
/// that paired with the same Weixin user id.
pub fn save_session(session: &QrSession, clear_stale: bool) -> Result<String> {
    let account_id = accounts::normalize_account_id(&session.bot_id);
    accounts::save_account(
        &account_id,
        &accounts::AccountUpdate {
            token: Some(session.token.clone()),
            base_url: if session.base_url.is_empty() {
                None
            } else {
                Some(session.base_url.clone())
            },
            user_id: Some(session.user_id.clone()),
        },
    )?;
    accounts::register_account_id(&account_id)?;

    if clear_stale {
        let mut ctx = ContextTokenStore::new();
        accounts::clear_stale_accounts_for_user_id(&account_id, &session.user_id, |id| {
            ctx.clear_account(id)
        })?;
    }
    Ok(account_id)
}

/// Convenience used by the CLI: the directory name for an account's managed
/// quote media.
pub fn account_quote_media_dir_name(account_id: &str) -> String {
    account_media_dir_name(account_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(op: &str, params: Value) -> Result<Value> {
        dispatch_sync(op, params)
    }

    #[test]
    fn version_op() {
        let v = call("version", Value::Null).unwrap();
        assert_eq!(v.as_str(), Some(crate::VERSION));
    }

    #[test]
    fn unknown_op_errors() {
        let err = call("does.not.exist", Value::Null).unwrap_err();
        assert!(err.to_string().contains("unknown op"));
    }

    #[test]
    fn util_ops_work() {
        let id = call("util.generate_id", json!({ "prefix": "test" })).unwrap();
        assert!(id["id"].as_str().unwrap().starts_with("test:"));

        let key = call("util.file_key", Value::Null).unwrap();
        assert_eq!(key["file_key"].as_str().unwrap().len(), 32);
    }

    #[test]
    fn mime_op() {
        let info = call("mime.info", json!({ "path": "photo.PNG" })).unwrap();
        assert_eq!(info["mime"], "image/png");
        assert_eq!(info["kind"], "Image");
        assert_eq!(info["is_image"], true);
    }

    #[test]
    fn cdn_url_ops() {
        let url = call(
            "cdn.download_url",
            json!({ "encrypted_query_param": "a+b", "cdn_base_url": "https://c.example/c2c" }),
        )
        .unwrap();
        assert!(url["url"]
            .as_str()
            .unwrap()
            .contains("encrypted_query_param=a%2Bb"));
    }

    #[test]
    fn crypto_round_trip() {
        let key_hex = "00112233445566778899aabbccddeeff";
        let data = encode_base64(b"hello world");
        let enc = call(
            "crypto.encrypt",
            json!({ "key_hex": key_hex, "data_base64": data }),
        )
        .unwrap();
        assert_eq!(enc["padded_size"], 16);
        let dec = call(
            "crypto.decrypt",
            json!({ "key_hex": key_hex, "data_base64": enc["data_base64"] }),
        )
        .unwrap();
        assert_eq!(
            decode_base64(dec["data_base64"].as_str().unwrap()).unwrap(),
            b"hello world"
        );
    }

    #[test]
    fn c_abi_round_trip() {
        let op = CString::new("version").unwrap();
        let ptr = ilink_call(op.as_ptr(), std::ptr::null());
        assert!(!ptr.is_null());
        let text = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_string();
        ilink_string_free(ptr);
        assert!(text.contains("\"ok\":true"));
        assert!(text.contains(crate::VERSION));
    }

    #[test]
    fn c_abi_null_op_is_error() {
        let ptr = ilink_call(std::ptr::null(), std::ptr::null());
        assert!(!ptr.is_null());
        let text = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_string();
        ilink_string_free(ptr);
        assert!(text.contains("\"ok\":false"));
    }

    #[test]
    fn session_guard_ops() {
        let id = format!("ffi-acct-{}", std::process::id());
        assert_eq!(
            call("session.status", json!({ "account_id": id })).unwrap()["paused"],
            false
        );
        call("session.pause", json!({ "account_id": id })).unwrap();
        assert_eq!(
            call("session.status", json!({ "account_id": id })).unwrap()["paused"],
            true
        );
        call("session.resume", json!({ "account_id": id })).unwrap();
        assert_eq!(
            call("session.status", json!({ "account_id": id })).unwrap()["paused"],
            false
        );
    }
}
