//! CLI entry point for the Weixin SDK.
//!
//! Wraps every capability of the library in a command-line tool so the SDK's
//! behaviour can be exercised end-to-end (and scripted) without writing code
//! first: QR login, long-poll receive, media send/download, typing indicators,
//! the quote cache, context-token store, sync cursor, session guard, MIME and
//! CDN helpers.
//!
//! Most commands accept `--json` (global) to emit machine-readable output.

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand, ValueEnum};
use ilink_sdk::api::session_guard::{self, STALE_TOKEN_ERRCODE};
use ilink_sdk::api::types::{CdnMedia, GetUpdatesReq, MessageItem};
use ilink_sdk::api::WeixinApiOptions;
use ilink_sdk::cdn::download::{
    download_and_decrypt, download_item_media, download_plain, fetch_cdn_bytes, InboundMedia,
    MediaSink,
};
use ilink_sdk::cdn::upload::{upload_file_attachment, upload_image, upload_video, UploadedFile};
use ilink_sdk::client::{WeixinClient, WeixinClientBuilder};
use ilink_sdk::error::{Error, Result};
use ilink_sdk::logging::{init_tracing, Level};
use ilink_sdk::messaging::context::{
    body_from_items, message_to_context, remember_context_token, ContextTokenStore,
};
use ilink_sdk::messaging::quote_cache::{Direction, QuoteMessageInput, QuoteStore};
use ilink_sdk::messaging::typing::{send_typing, Typing};
use ilink_sdk::messaging::SendOpts;
use ilink_sdk::storage::{accounts, config, state_dir, sync_buf};
use serde_json::json;

/// Weixin (iLink) SDK command line.
#[derive(Debug, Parser)]
#[command(
    name = "weixin",
    version,
    about = "Weixin (iLink) backend SDK: QR login, long-poll receive, media send/download",
    long_about = None,
)]
struct Cli {
    /// Log level: trace | debug | info | warn | error.
    #[arg(long, env = "RUST_LOG_LEVEL", default_value = "info", global = true)]
    log_level: Level,

    /// Operate on this account id instead of the first registered account.
    #[arg(long, short = 'a', global = true, env = "WEIXIN_ACCOUNT_ID")]
    account: Option<String>,

    /// Override the API base URL.
    #[arg(long, global = true, env = "WEIXIN_BASE_URL")]
    base_url: Option<String>,

    /// Override the CDN base URL.
    #[arg(long, global = true, env = "WEIXIN_CDN_BASE_URL")]
    cdn_base_url: Option<String>,

    /// Forward this value as the `SKRouteTag` header.
    #[arg(long, global = true, env = "WEIXIN_ROUTE_TAG")]
    route_tag: Option<String>,

    /// Override the observability `bot_agent` declaration.
    #[arg(long, global = true, env = "WEIXIN_BOT_AGENT")]
    bot_agent: Option<String>,

    /// Per-request timeout override, in milliseconds.
    #[arg(long, global = true, env = "WEIXIN_TIMEOUT_MS")]
    timeout_ms: Option<u64>,

    /// Emit machine-readable JSON instead of human text.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Scan a QR code to bind a new Weixin bot account.
    Login {
        /// Give up after this many seconds.
        #[arg(long, default_value = "480")]
        timeout: u64,
        /// Print the QR code as text on stdout.
        #[arg(long)]
        show_qr: bool,
        /// Override the `bot_type` query parameter (default 3).
        #[arg(long)]
        bot_type: Option<String>,
        /// Do not persist the resulting credentials.
        #[arg(long)]
        no_save: bool,
    },

    /// List locally registered accounts.
    Accounts,

    /// Resolve and print the effective settings for an account.
    Status,

    /// Forget a local account (credentials + cursor + context tokens + quote cache).
    Logout {
        /// The account id to forget.
        id: String,
    },

    /// Long-poll for inbound messages, print them and optionally save media.
    Listen {
        /// Stop after this many seconds (0 = forever).
        #[arg(long, default_value = "0")]
        duration: u64,
        /// Long-poll timeout per request, in milliseconds.
        #[arg(long, default_value = "35000")]
        poll_timeout: u64,
        /// Directory for downloaded inbound media.
        #[arg(long)]
        media_dir: Option<PathBuf>,
        /// Populate the local quote cache with inbound/echoed messages.
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        quote_cache: bool,
        /// Reply to `/ping` with `pong`.
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        echo: bool,
    },

    /// Send a text message to a Weixin user.
    Send {
        /// Recipient Weixin id (e.g. `xxx@im.wechat`).
        to: String,
        /// Message text.
        text: String,
        /// Conversation context token from an inbound message.
        #[arg(long)]
        context_token: Option<String>,
        /// Optional generation / run id.
        #[arg(long)]
        run_id: Option<String>,
    },

    /// Send a local file or an image/video referenced by path.
    SendFile {
        /// Recipient Weixin id.
        to: String,
        /// Path to a local file.
        path: PathBuf,
        /// Optional caption sent before the media.
        #[arg(long, default_value = "")]
        caption: String,
        #[arg(long)]
        context_token: Option<String>,
    },

    /// Send a raw `MessageItem` JSON object.
    SendItem {
        /// Recipient Weixin id.
        to: String,
        /// A JSON object matching the wire `MessageItem` shape.
        item: String,
        #[arg(long)]
        context_token: Option<String>,
    },

    /// Fetch the bot config (typing ticket) for a user, or the local config.
    Config {
        /// Weixin user id; when omitted the local channel config is printed.
        user: Option<String>,
        #[arg(long)]
        context_token: Option<String>,
    },

    /// Show or cancel the typing indicator for a user.
    Typing {
        /// Recipient Weixin id.
        user: String,
        /// `on` (typing) or `off` (cancel).
        #[arg(value_enum, default_value_t = TypingAction::On)]
        action: TypingAction,
        #[arg(long)]
        context_token: Option<String>,
    },

    /// Notify the backend this client is starting or stopping.
    Notify {
        #[arg(value_enum)]
        action: NotifyAction,
    },

    /// Download (and optionally decrypt) a CDN object.
    Download {
        /// Complete URL; when set, `--param` is ignored.
        #[arg(long)]
        url: Option<String>,
        /// `encrypt_query_param` to build the URL from.
        #[arg(long)]
        param: Option<String>,
        /// Base64 AES key (16 raw bytes or 32 hex chars); omit for plain objects.
        #[arg(long)]
        aes_key: Option<String>,
        /// Write the bytes here instead of printing them.
        #[arg(long, short = 'o')]
        out: Option<PathBuf>,
    },

    /// Upload a local file to the CDN and print the resulting reference.
    Upload {
        /// Recipient Weixin id the upload is addressed to.
        to: String,
        /// Path to the local file.
        path: PathBuf,
        /// Media type: image | video | file | voice.
        #[arg(long, default_value = "file")]
        media_type: String,
    },

    /// Print the MIME type and outbound routing for a path.
    Mime {
        /// File name or path.
        path: String,
    },

    /// Print a generated id / file key / temp name.
    Id {
        #[arg(long, default_value = "ilink-sdk")]
        prefix: String,
        /// `id` | `filekey` | `temp`.
        #[arg(long, default_value = "id")]
        kind: String,
        /// Extension used by `--kind temp`.
        #[arg(long, default_value = ".bin")]
        ext: String,
    },

    /// Quote-cache operations.
    Quote {
        #[command(subcommand)]
        command: QuoteCommand,
    },

    /// Reply-context-token store operations.
    Context {
        #[command(subcommand)]
        command: ContextCommand,
    },

    /// Long-poll cursor operations.
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },

    /// Session-guard (stale-token pause) operations.
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },

    /// CDN URL builders.
    Cdn {
        #[command(subcommand)]
        command: CdnCommand,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TypingAction {
    On,
    Off,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum NotifyAction {
    Start,
    Stop,
}

#[derive(Debug, Subcommand)]
enum QuoteCommand {
    /// Print the effective quote-cache policy.
    Policy,
    /// Look up one cached message.
    Find {
        /// Conversation id (usually the peer Weixin id).
        #[arg(long)]
        conversation: String,
        /// Server message id.
        #[arg(long)]
        message_id: String,
    },
    /// Insert or update a cached message.
    Put {
        #[arg(long)]
        conversation: String,
        #[arg(long)]
        message_id: String,
        #[arg(long, default_value = "")]
        body: String,
        #[arg(long, default_value = "inbound")]
        direction: String,
        /// A media file inside the account's managed media directory.
        #[arg(long)]
        media_path: Option<PathBuf>,
        #[arg(long)]
        media_mime: Option<String>,
    },
    /// Run a garbage-collection pass.
    Gc,
    /// Delete every cached message and media file for an account.
    Clear,
    /// Print the account's managed media directory.
    MediaDir,
}

#[derive(Debug, Subcommand)]
enum ContextCommand {
    /// Get the stored context token for a user.
    Get {
        /// The peer Weixin id.
        user: String,
    },
    /// Store a context token for a user.
    Set { user: String, token: String },
    /// Remove every stored token for the account.
    Clear,
    /// Reload persisted tokens from disk.
    Restore,
}

#[derive(Debug, Subcommand)]
enum SyncCommand {
    /// Print the persisted cursor.
    Show,
    /// Overwrite the persisted cursor.
    Set { buf: String },
    /// Delete the persisted cursor.
    Clear,
}

#[derive(Debug, Subcommand)]
enum SessionCommand {
    /// Print whether the account is paused.
    Status,
    /// Pause the account for one hour.
    Pause,
    /// Clear a pause early.
    Resume,
    /// Error when the account is paused (verification helper).
    Assert,
}

#[derive(Debug, Subcommand)]
enum CdnCommand {
    /// Build a download URL from `encrypt_query_param`.
    DownloadUrl {
        #[arg(long)]
        param: String,
    },
    /// Build an upload URL from `upload_param` + `filekey`.
    UploadUrl {
        #[arg(long)]
        param: String,
        #[arg(long)]
        filekey: String,
    },
}

fn main() {
    let cli = Cli::parse();
    init_tracing(cli.log_level);

    if let Err(err) = run(&cli) {
        if cli.json {
            let payload = json!({
                "ok": false,
                "error": err.to_string(),
                "kind": err.kind().as_str(),
            });
            println!("{payload}");
        } else {
            eprintln!("error: {err}");
        }
        std::process::exit(1);
    }
}

fn run(cli: &Cli) -> Result<()> {
    match &cli.command {
        Command::Accounts => {
            let ids = accounts::list_indexed_account_ids();
            if cli.json {
                let mut out = Vec::new();
                for id in &ids {
                    let a = accounts::resolve_account(id)?;
                    out.push(account_json(&a));
                }
                emit_json(&json!({ "accounts": out }));
                return Ok(());
            }
            if ids.is_empty() {
                println!("no accounts registered — run `weixin login`");
                return Ok(());
            }
            for id in &ids {
                let a = accounts::resolve_account(id)?;
                println!(
                    "{id}\n  base_url={}\n  cdn_base_url={}\n  configured={}\n  enabled={}\n  user={}\n  route_tag={}",
                    a.base_url,
                    a.cdn_base_url,
                    a.configured,
                    a.enabled,
                    a.user_id.as_deref().unwrap_or("(none)"),
                    a.route_tag.as_deref().unwrap_or("(none)")
                );
            }
            Ok(())
        }

        Command::Logout { id } => {
            let normalized = accounts::normalize_account_id(id);
            session_guard::resume_session(&normalized);
            clear_persisted_pause(&normalized);
            let mut ctx = ContextTokenStore::new();
            ctx.clear_account(&normalized);
            sync_buf::clear_for_account(&normalized);
            if let Ok(store) = open_quote_store() {
                let mut store = store;
                store.delete_account(&normalized);
            }
            accounts::clear_account(&normalized)?;
            accounts::unregister_account_id(&normalized)?;
            if cli.json {
                emit_json(&json!({ "forgot": normalized }));
            } else {
                println!("forgot account {normalized}");
            }
            Ok(())
        }

        Command::Status => {
            let rt = runtime()?;
            rt.block_on(show_status(cli))
        }

        Command::Login {
            timeout,
            show_qr,
            bot_type,
            no_save,
        } => {
            let rt = runtime()?;
            rt.block_on(login(
                cli,
                *timeout,
                *show_qr,
                bot_type.as_deref(),
                *no_save,
            ))
        }

        Command::Listen {
            duration,
            poll_timeout,
            media_dir,
            quote_cache,
            echo,
        } => {
            let rt = runtime()?;
            rt.block_on(listen(
                cli,
                *duration,
                *poll_timeout,
                media_dir.clone(),
                *quote_cache,
                *echo,
            ))
        }

        Command::Send {
            to,
            text,
            context_token,
            run_id,
        } => {
            let rt = runtime()?;
            rt.block_on(send_text_cmd(
                cli,
                to,
                text,
                context_token.as_deref(),
                run_id.as_deref(),
            ))
        }

        Command::SendFile {
            to,
            path,
            caption,
            context_token,
        } => {
            let rt = runtime()?;
            rt.block_on(send_file_cmd(
                cli,
                to,
                path,
                caption,
                context_token.as_deref(),
            ))
        }

        Command::SendItem {
            to,
            item,
            context_token,
        } => {
            let rt = runtime()?;
            rt.block_on(send_item_cmd(cli, to, item, context_token.as_deref()))
        }

        Command::Config {
            user,
            context_token,
        } => {
            let rt = runtime()?;
            rt.block_on(config_cmd(cli, user.as_deref(), context_token.as_deref()))
        }

        Command::Typing {
            user,
            action,
            context_token,
        } => {
            let rt = runtime()?;
            rt.block_on(typing_cmd(cli, user, *action, context_token.as_deref()))
        }

        Command::Notify { action } => {
            let rt = runtime()?;
            rt.block_on(notify_cmd(cli, *action))
        }

        Command::Download {
            url,
            param,
            aes_key,
            out,
        } => {
            let rt = runtime()?;
            rt.block_on(download_cmd(
                cli,
                url.as_deref(),
                param.as_deref(),
                aes_key.as_deref(),
                out.clone(),
            ))
        }

        Command::Upload {
            to,
            path,
            media_type,
        } => {
            let rt = runtime()?;
            rt.block_on(upload_cmd(cli, to, path, media_type))
        }

        Command::Mime { path } => {
            let mime = ilink_sdk::media::mime::mime_from_filename(path);
            let kind = ilink_sdk::media::mime::MediaKind::from_path(path);
            if cli.json {
                emit_json(&json!({
                    "path": path,
                    "mime": mime,
                    "kind": format!("{kind:?}"),
                    "extension": ilink_sdk::media::mime::extension_of(path),
                    "is_image": ilink_sdk::media::mime::is_image(path),
                    "is_video": ilink_sdk::media::mime::is_video(path),
                    "is_audio": ilink_sdk::media::mime::is_audio(path),
                }));
            } else {
                println!("mime={mime} kind={kind:?}");
            }
            Ok(())
        }

        Command::Id { prefix, kind, ext } => {
            let value = match kind.as_str() {
                "filekey" => ilink_sdk::util::file_key(),
                "temp" => ilink_sdk::util::temp_file_name(prefix, ext),
                _ => ilink_sdk::util::generate_id(prefix),
            };
            if cli.json {
                emit_json(&json!({ "value": value }));
            } else {
                println!("{value}");
            }
            Ok(())
        }

        Command::Quote { command } => quote_cmd(cli, command),
        Command::Context { command } => context_cmd(cli, command),
        Command::Sync { command } => sync_cmd(cli, command),
        Command::Session { command } => session_cmd(cli, command),
        Command::Cdn { command } => cdn_cmd(cli, command),
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn emit_json(value: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
    );
}

fn account_json(a: &accounts::StoredAccount) -> serde_json::Value {
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

/// Build a client for API-only work, tolerating a missing token.
///
/// When `enforce_pause` is set, a persisted/CLI session pause for the resolved
/// account aborts the call (login passes `false` so re-login always works).
fn api_only_client_cli(cli: &Cli, enforce_pause: bool) -> Result<ilink_sdk::api::WeixinApiClient> {
    if enforce_pause {
        if let Some(id) = effective_account_id(cli) {
            assert_not_paused(&id)?;
        }
    }
    let base = cli
        .base_url
        .clone()
        .unwrap_or_else(|| accounts::DEFAULT_BASE_URL.to_string());
    let mut opts = WeixinApiOptions::new(base);
    let id = cli
        .account
        .clone()
        .or_else(|| accounts::list_indexed_account_ids().into_iter().next());
    if let Some(id) = id {
        if let Ok(data) = accounts::resolve_account(&id) {
            if let Some(token) = data.token {
                opts = opts.with_token(token);
            }
            if cli.base_url.is_none() {
                opts.base_url = data.base_url;
            }
        }
    }
    if let Some(ms) = cli.timeout_ms {
        opts = opts.with_timeout_ms(ms);
    }
    if let Some(tag) = &cli.route_tag {
        opts = opts.with_route_tag(tag.clone());
    }
    if let Some(cdn) = &cli.cdn_base_url {
        opts = opts.with_cdn_base_url(cdn.clone());
    }
    let mut api = ilink_sdk::api::WeixinApiClient::new(opts)?;
    if let Some(agent) = &cli.bot_agent {
        api = api.with_bot_agent(agent);
    }
    Ok(api)
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::unknown(e.to_string()))
}

async fn build_client(cli: &Cli) -> Result<WeixinClient> {
    if let Some(id) = effective_account_id(cli) {
        assert_not_paused(&id)?;
    }
    let mut builder = WeixinClientBuilder::new()
        .account_id_opt(cli.account.clone())
        .base_url_opt(cli.base_url.clone())
        .cdn_base_url_opt(cli.cdn_base_url.clone());
    if let Some(tag) = &cli.route_tag {
        builder = builder.route_tag(tag.clone());
    }
    if let Some(agent) = &cli.bot_agent {
        builder = builder.bot_agent(agent.clone());
    }
    if let Some(ms) = cli.timeout_ms {
        builder = builder.timeout_ms(ms);
    }
    builder.build().await
}

fn send_opts(context_token: Option<&str>, run_id: Option<&str>) -> SendOpts {
    SendOpts {
        context_token: context_token.map(str::to_string),
        run_id: run_id.map(str::to_string),
    }
}

/// Effective quote-cache policy from local config.
fn quote_policy() -> ilink_sdk::messaging::quote_cache::QuoteCachePolicy {
    let raw = config::load_local_config().quote_cache.to_policy();
    ilink_sdk::messaging::quote_cache::QuoteCachePolicy {
        enabled: raw.enabled,
        retention_ms: raw.retention_ms,
        max_messages_per_account: raw.max_messages_per_account,
        media_retention_ms: raw.media_retention_ms,
        max_media_bytes_per_account: raw.max_media_bytes_per_account,
        max_single_media_bytes: raw.max_single_media_bytes,
    }
}

fn open_quote_store() -> Result<QuoteStore> {
    QuoteStore::open(quote_policy())?
        .ok_or_else(|| Error::unknown("quote cache is disabled by config"))
}

// ---------------------------------------------------------------------------
// Session-guard persistence
//
// The SDK's session guard is deliberately in-process (it mirrors the upstream
// plugin). A CLI command, however, is a fresh process every time, so the CLI
// additionally persists the pause to the state directory; `build_client` and
// the API-only helpers refuse to run while it is active.
// ---------------------------------------------------------------------------

fn now_ms_u128() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn pause_state_path(account_id: &str) -> PathBuf {
    state_dir::accounts_dir().join(format!("{account_id}.session-pause.json"))
}

/// Remaining persisted pause in ms (0 when absent/expired). Expired files are
/// removed on read.
fn persisted_pause_ms(account_id: &str) -> u128 {
    let path = pause_state_path(account_id);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return 0;
    };
    let until = serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| v.get("until_ms").and_then(|x| x.as_u64()))
        .unwrap_or(0) as u128;
    let now = now_ms_u128();
    if until <= now {
        let _ = std::fs::remove_file(&path);
        0
    } else {
        until - now
    }
}

fn persist_pause(account_id: &str, remaining_ms: u128) {
    let dir = state_dir::accounts_dir();
    let _ = std::fs::create_dir_all(&dir);
    let until = now_ms_u128() + remaining_ms;
    let body = json!({ "account_id": account_id, "until_ms": until }).to_string();
    let _ = std::fs::write(pause_state_path(account_id), body);
}

fn clear_persisted_pause(account_id: &str) {
    let _ = std::fs::remove_file(pause_state_path(account_id));
}

/// Remaining pause, taking the max of the persisted and in-process guards.
fn pause_remaining_ms(account_id: &str) -> u128 {
    persisted_pause_ms(account_id).max(session_guard::get_remaining_pause_ms(account_id))
}

fn assert_not_paused(account_id: &str) -> Result<()> {
    let remaining = pause_remaining_ms(account_id);
    if remaining > 0 {
        let minutes = remaining.div_ceil(60_000);
        return Err(Error::unknown(format!(
            "session paused for accountId={account_id}, {minutes} min remaining \
             (errcode {STALE_TOKEN_ERRCODE}); run `weixin session resume` to clear"
        )));
    }
    Ok(())
}

/// Normalized id of the account a command will operate on.
fn effective_account_id(cli: &Cli) -> Option<String> {
    cli.account
        .clone()
        .or_else(|| accounts::list_indexed_account_ids().into_iter().next())
        .map(|id| accounts::normalize_account_id(&id))
}

// ---------------------------------------------------------------------------
// async commands
// ---------------------------------------------------------------------------

async fn show_status(cli: &Cli) -> Result<()> {
    let client = build_client(cli).await?;
    let a = client.account();
    let account_id = a.account_id.clone();
    let remaining = pause_remaining_ms(&account_id);
    let paused = remaining > 0;
    if cli.json {
        let mut value = account_json(a);
        value["token"] = json!(client.token().map(|_| "present").unwrap_or("missing"));
        value["session_paused"] = json!(paused);
        value["session_remaining_ms"] = json!(remaining);
        value["config_path"] = json!(config::resolve_config_path().to_string_lossy());
        value["state_dir"] = json!(state_dir::resolve_state_dir().to_string_lossy());
        emit_json(&value);
        return Ok(());
    }
    println!("account_id={}", a.account_id);
    println!("base_url={}", a.base_url);
    println!("cdn_base_url={}", a.cdn_base_url);
    println!("configured={}", a.configured);
    println!("enabled={}", a.enabled);
    println!(
        "token={}",
        client.token().map(|_| "present").unwrap_or("missing")
    );
    println!("user_id={}", a.user_id.as_deref().unwrap_or("(none)"));
    println!("route_tag={}", a.route_tag.as_deref().unwrap_or("(none)"));
    println!("state_dir={}", state_dir::resolve_state_dir().display());
    println!("config_path={}", config::resolve_config_path().display());
    println!("session_paused={paused}");
    Ok(())
}

async fn send_text_cmd(
    cli: &Cli,
    to: &str,
    text: &str,
    context_token: Option<&str>,
    run_id: Option<&str>,
) -> Result<()> {
    let client = build_client(cli).await?;
    let result = client
        .sender()
        .send_text(to, text, &send_opts(context_token, run_id))
        .await?;
    if cli.json {
        emit_json(&json!({
            "message_id": result.message_id,
            "server_message_id": result.server_message_id,
        }));
    } else {
        println!("sent clientId={}", result.message_id);
        if let Some(id) = result.server_message_id {
            println!("serverMessageId={id}");
        }
    }
    Ok(())
}

async fn send_file_cmd(
    cli: &Cli,
    to: &str,
    path: &std::path::Path,
    caption: &str,
    context_token: Option<&str>,
) -> Result<()> {
    let client = build_client(cli).await?;
    let cdn = client.account().cdn_base_url.clone();
    let result = client
        .sender()
        .send_file(to, path, caption, &send_opts(context_token, None), &cdn)
        .await?;
    if cli.json {
        emit_json(&json!({
            "message_id": result.message_id,
            "server_message_id": result.server_message_id,
        }));
    } else {
        println!("sent clientId={}", result.message_id);
    }
    Ok(())
}

async fn send_item_cmd(
    cli: &Cli,
    to: &str,
    item_json: &str,
    context_token: Option<&str>,
) -> Result<()> {
    let item: MessageItem = serde_json::from_str(item_json)?;
    let client = build_client(cli).await?;
    let result = client
        .sender()
        .send_item(to, item, &send_opts(context_token, None))
        .await?;
    if cli.json {
        emit_json(&json!({
            "message_id": result.message_id,
            "server_message_id": result.server_message_id,
        }));
    } else {
        println!("sent clientId={}", result.message_id);
    }
    Ok(())
}

async fn config_cmd(cli: &Cli, user: Option<&str>, context_token: Option<&str>) -> Result<()> {
    match user {
        Some(user) => {
            let api = api_only_client_cli(cli, true)?;
            let resp = api.get_config(user, context_token).await?;
            if cli.json {
                emit_json(&serde_json::to_value(&resp)?);
            } else {
                println!("ret={:?} errmsg={:?}", resp.ret, resp.errmsg);
                println!(
                    "typing_ticket={}",
                    resp.typing_ticket.as_deref().unwrap_or("(none)")
                );
            }
        }
        None => {
            let cfg = config::load_local_config();
            if cli.json {
                emit_json(&json!({
                    "config": serde_json::to_value(&cfg)?,
                    "config_path": config::resolve_config_path().to_string_lossy(),
                    "quote_cache_policy": {
                        "enabled": quote_policy().enabled,
                        "retention_ms": quote_policy().retention_ms,
                    },
                }));
            } else {
                println!("config_path={}", config::resolve_config_path().display());
                println!("bot_agent={}", cfg.bot_agent.as_deref().unwrap_or("(none)"));
                println!(
                    "route_tag={}",
                    cfg.route_tag
                        .as_ref()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "(none)".to_string())
                );
                println!("reply_progress_messages={}", cfg.reply_progress_messages);
                println!("quote_cache_enabled={}", quote_policy().enabled);
            }
        }
    }
    Ok(())
}

async fn typing_cmd(
    cli: &Cli,
    user: &str,
    action: TypingAction,
    context_token: Option<&str>,
) -> Result<()> {
    let api = api_only_client_cli(cli, true)?;
    let config = api.get_config(user, context_token).await?;
    let ticket = config.typing_ticket.clone().unwrap_or_default();
    let status = match action {
        TypingAction::On => Typing::Typing,
        TypingAction::Off => Typing::Cancel,
    };
    send_typing(&api, user, &ticket, status).await?;
    if cli.json {
        emit_json(&json!({
            "sent": true,
            "ticket_present": !ticket.is_empty(),
            "action": match action { TypingAction::On => "on", TypingAction::Off => "off" },
        }));
    } else {
        println!(
            "typing {} sent (ticket={})",
            match action {
                TypingAction::On => "on",
                TypingAction::Off => "off",
            },
            if ticket.is_empty() {
                "missing"
            } else {
                "present"
            }
        );
    }
    Ok(())
}

async fn notify_cmd(cli: &Cli, action: NotifyAction) -> Result<()> {
    let api = api_only_client_cli(cli, true)?;
    match action {
        NotifyAction::Start => {
            api.notify_start().await?;
        }
        NotifyAction::Stop => {
            api.notify_stop().await?;
        }
    }
    if cli.json {
        emit_json(&json!({
            "notified": match action { NotifyAction::Start => "start", NotifyAction::Stop => "stop" },
        }));
    } else {
        println!(
            "notified {}",
            match action {
                NotifyAction::Start => "start",
                NotifyAction::Stop => "stop",
            }
        );
    }
    Ok(())
}

async fn download_cmd(
    cli: &Cli,
    url: Option<&str>,
    param: Option<&str>,
    aes_key: Option<&str>,
    out: Option<PathBuf>,
) -> Result<()> {
    let cdn = cli
        .cdn_base_url
        .clone()
        .unwrap_or_else(|| accounts::CDN_BASE_URL.to_string());
    let media = CdnMedia {
        encrypt_query_param: param.map(str::to_string),
        full_url: url.map(str::to_string),
        ..Default::default()
    };
    let bytes = match aes_key {
        Some(key) => download_and_decrypt(&media, key, &cdn, "cli download").await?,
        None if url.is_some() && param.is_none() => {
            fetch_cdn_bytes(url.unwrap(), "cli download").await?
        }
        None => download_plain(&media, &cdn, "cli download").await?,
    };

    match out {
        Some(path) => {
            std::fs::write(&path, &bytes)?;
            if cli.json {
                emit_json(&json!({ "bytes": bytes.len(), "path": path.to_string_lossy() }));
            } else {
                println!("wrote {} bytes to {}", bytes.len(), path.display());
            }
        }
        None => {
            if cli.json {
                use base64::Engine;
                emit_json(&json!({
                    "bytes": bytes.len(),
                    "data_base64": base64::engine::general_purpose::STANDARD.encode(&bytes),
                }));
            } else {
                println!("downloaded {} bytes", bytes.len());
            }
        }
    }
    Ok(())
}

async fn upload_cmd(cli: &Cli, to: &str, path: &std::path::Path, media_type: &str) -> Result<()> {
    let api = api_only_client_cli(cli, true)?;
    let cdn = cli
        .cdn_base_url
        .clone()
        .unwrap_or_else(|| accounts::CDN_BASE_URL.to_string());
    let uploaded = match media_type {
        "image" => upload_image(to, path, &api, &cdn).await?,
        "video" => upload_video(to, path, &api, &cdn).await?,
        "voice" => {
            ilink_sdk::cdn::upload::upload_media(
                to,
                path,
                ilink_sdk::api::types::UploadMediaType::Voice,
                &api,
                &cdn,
            )
            .await?
        }
        _ => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file.bin".to_string());
            upload_file_attachment(to, path, &name, &api, &cdn).await?
        }
    };
    if cli.json {
        emit_json(&uploaded_json(&uploaded));
    } else {
        println!("filekey={}", uploaded.filekey);
        println!("download_param={}", uploaded.download_param);
        println!("aes_key_base64={}", uploaded.aes_key_base64());
        println!("raw_size={}", uploaded.file_size);
        println!("ciphertext_size={}", uploaded.file_size_ciphertext);
    }
    Ok(())
}

fn uploaded_json(u: &UploadedFile) -> serde_json::Value {
    json!({
        "filekey": u.filekey,
        "download_param": u.download_param,
        "aeskey_hex": u.aeskey_hex,
        "aes_key_base64": u.aes_key_base64(),
        "file_size": u.file_size,
        "file_size_ciphertext": u.file_size_ciphertext,
    })
}

async fn login(
    cli: &Cli,
    timeout: u64,
    show_qr: bool,
    bot_type: Option<&str>,
    no_save: bool,
) -> Result<()> {
    // Login works without a stored account: the QR flow issues the token.
    let api = api_only_client_cli(cli, false)?;
    let mut qr = ilink_sdk::messaging::QrLogin::new(&api);
    if let Some(bot_type) = bot_type {
        qr = qr.with_bot_type(bot_type);
    }

    let start = qr.start(None).await?;
    if cli.json {
        emit_json(&json!({
            "qrcode": start.qrcode,
            "qrcode_url": start.qrcode_url,
            "session_key": start.session_key,
        }));
    } else {
        println!("{}", start.qrcode_url);
        if show_qr {
            print_qr(&start.qrcode_url);
        }
        println!("waiting for scan (session {})", start.session_key);
    }

    let deadline = Duration::from_secs(timeout.max(1));
    let result = qr
        .wait(
            &start.session_key,
            deadline,
            |retry: bool| {
                let prompt = if retry {
                    "code did not match, try again: "
                } else {
                    "enter the code shown on your phone: "
                };
                async move {
                    eprintln!("{prompt}");
                    std::io::stdout().flush().ok();
                    let mut line = String::new();
                    if std::io::stdin().read_line(&mut line).is_err() {
                        line.clear();
                    }
                    Ok(line.trim().to_string())
                }
            },
            |refreshed: &ilink_sdk::messaging::QrStartResult| {
                let url = refreshed.qrcode_url.clone();
                async move {
                    eprintln!("qr refreshed: {url}");
                }
            },
        )
        .await?;

    match result.session {
        Some(session) => {
            let id = accounts::normalize_account_id(&session.bot_id);
            if !no_save {
                accounts::save_account(
                    &id,
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
                accounts::register_account_id(&id)?;
                session_guard::resume_session(&id);
                clear_persisted_pause(&id);
                let mut ctx = ContextTokenStore::new();
                accounts::clear_stale_accounts_for_user_id(&id, &session.user_id, |stale| {
                    ctx.clear_account(stale)
                })?;
            }
            if cli.json {
                emit_json(&json!({
                    "connected": true,
                    "account_id": id,
                    "saved": !no_save,
                    "session": {
                        "bot_id": session.bot_id,
                        "base_url": session.base_url,
                        "user_id": session.user_id,
                        "token_present": !session.token.is_empty(),
                    },
                }));
            } else if no_save {
                println!("connected (not saved); account {id}");
            } else {
                println!("connected; saved account {id}");
            }
        }
        None => {
            if cli.json {
                emit_json(&json!({
                    "connected": false,
                    "already_connected": result.already_connected,
                    "message": result.message,
                }));
            } else {
                println!("{}", result.message);
            }
            if !result.already_connected {
                std::process::exit(2);
            }
        }
    }
    Ok(())
}

async fn listen(
    cli: &Cli,
    duration: u64,
    poll_timeout: u64,
    media_dir: Option<PathBuf>,
    use_quote_cache: bool,
    echo: bool,
) -> Result<()> {
    let mut client = build_client(cli).await?;
    if use_quote_cache {
        // The quote cache is best-effort: a failure must not stop receiving.
        if let Err(err) = client.enable_quote_cache() {
            eprintln!("warning: quote cache disabled: {err}");
        }
    }
    let account_id = client.account_id().to_string();
    let api = client.api().clone();
    let cdn = client.account().cdn_base_url.clone();

    let cursor_path = sync_buf::get_sync_buf_path(&account_id);
    let mut buf = sync_buf::load_get_updates_buf(&cursor_path).unwrap_or_default();

    let mut ctx_store = ContextTokenStore::new();
    ctx_store.restore(&account_id);

    let media_dir = media_dir.unwrap_or_else(|| {
        state_dir::resolve_state_dir()
            .join("media")
            .join("inbound")
            .join("openclaw-weixin")
    });
    let sink = MediaSink::new(&media_dir);
    let mut quote_store = if use_quote_cache {
        open_quote_store().ok()
    } else {
        None
    };

    if !cli.json {
        println!("listening on account {account_id}");
        println!("media_dir={}", media_dir.display());
    }
    client.notify_start().await.ok();

    let start = std::time::Instant::now();
    loop {
        if duration > 0 && start.elapsed().as_secs() >= duration {
            break;
        }
        let resp = api
            .get_updates(
                &GetUpdatesReq {
                    get_updates_buf: Some(buf.clone()),
                    sync_buf: None,
                },
                Some(poll_timeout),
                None,
            )
            .await?;

        if resp.is_api_error() {
            if resp.is_stale_token(STALE_TOKEN_ERRCODE) {
                session_guard::pause_session(&account_id);
                persist_pause(
                    &account_id,
                    session_guard::SESSION_PAUSE_DURATION.as_millis(),
                );
                eprintln!(
                    "stale bot token (errcode {STALE_TOKEN_ERRCODE}); session paused. Re-run `weixin login`."
                );
                break;
            }
            eprintln!(
                "getUpdates error ret={:?} errcode={:?} errmsg={:?}",
                resp.ret, resp.errcode, resp.errmsg
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }
        if let Some(next) = resp.get_updates_buf.as_deref().filter(|s| !s.is_empty()) {
            buf = next.to_string();
            let _ = sync_buf::save_get_updates_buf(&cursor_path, &buf);
        }

        for msg in resp.msgs.as_deref().unwrap_or(&[]) {
            let from = msg.from_user_id.as_deref().unwrap_or("").to_string();
            let items = msg.item_list.as_deref().unwrap_or(&[]);

            // Download any attached media before building the context.
            let mut inbound = InboundMedia::default();
            for item in items {
                let got = download_item_media(item, &cdn, &sink, &account_id).await;
                merge_media(&mut inbound, got);
            }

            let ctx = message_to_context(msg, &account_id, &inbound);
            let body = body_from_items(items);
            remember_context_token(&mut ctx_store, &ctx);

            if let Some(store) = quote_store.as_mut() {
                let _ = store.put(&QuoteMessageInput {
                    account_id: account_id.clone(),
                    conversation_id: from.clone(),
                    message_id: ctx
                        .message_sid_full
                        .clone()
                        .unwrap_or_else(|| ctx.message_sid.clone()),
                    direction: Direction::Inbound,
                    body: body.clone(),
                    source_media_path: ctx
                        .media_path
                        .clone()
                        .or_else(|| ctx.media_paths.first().cloned()),
                    media_mime: ctx.media_type.clone(),
                    media_name: None,
                    created_at: 0,
                });
            }

            if cli.json {
                emit_json(&json!({
                    "from": from,
                    "message_id": ctx.message_sid_full,
                    "body": body,
                    "context_token_present": ctx.context_token.is_some(),
                    "image_path": inbound.image_path,
                    "video_path": inbound.video_path,
                    "file_path": inbound.file_path,
                    "voice_path": inbound.voice_path,
                    "reply_to_id": ctx.reply_to_id,
                    "reply_to_body": ctx.reply_to_body,
                }));
            } else {
                println!(
                    "inbound from={from} id={:?} body={body}",
                    ctx.message_sid_full.as_deref().unwrap_or("")
                );
                if let Some(p) = &inbound.image_path {
                    println!("  image={p}");
                }
                if let Some(p) = &inbound.video_path {
                    println!("  video={p}");
                }
                if let Some(p) = &inbound.file_path {
                    println!("  file={p}");
                }
                if let Some(p) = &inbound.voice_path {
                    println!("  voice={p}");
                }
                if let Some(q) = &ctx.reply_to_body {
                    println!("  quote={q}");
                }
            }

            // Echo slash commands so the CLI is usable on its own.
            let trimmed = body.trim();
            if echo && trimmed == "/ping" {
                let context_token = ctx_store
                    .get(&account_id, &from)
                    .map(str::to_string)
                    .or_else(|| msg.context_token.clone());
                client
                    .sender()
                    .send_text(
                        &from,
                        "pong",
                        &SendOpts {
                            context_token,
                            ..Default::default()
                        },
                    )
                    .await?;
            }
        }
    }

    client.notify_stop().await.ok();
    Ok(())
}

fn merge_media(target: &mut InboundMedia, other: InboundMedia) {
    if target.image_path.is_none() {
        target.image_path = other.image_path;
    }
    if target.video_path.is_none() {
        target.video_path = other.video_path;
    }
    if target.file_path.is_none() {
        target.file_path = other.file_path;
        target.file_mime = other.file_mime;
    }
    if target.voice_path.is_none() {
        target.voice_path = other.voice_path;
        target.voice_mime = other.voice_mime;
    }
    target.referenced |= other.referenced;
}

// ---------------------------------------------------------------------------
// sync subcommands
// ---------------------------------------------------------------------------

fn quote_cmd(cli: &Cli, command: &QuoteCommand) -> Result<()> {
    let account_id = effective_account_id(cli).unwrap_or_else(|| "default".to_string());
    match command {
        QuoteCommand::Policy => {
            let p = quote_policy();
            if cli.json {
                emit_json(&json!({
                    "enabled": p.enabled,
                    "retention_ms": p.retention_ms,
                    "max_messages_per_account": p.max_messages_per_account,
                    "media_retention_ms": p.media_retention_ms,
                    "max_media_bytes_per_account": p.max_media_bytes_per_account,
                    "max_single_media_bytes": p.max_single_media_bytes,
                }));
            } else {
                println!("enabled={}", p.enabled);
                println!("retention_ms={}", p.retention_ms);
                println!("max_messages_per_account={}", p.max_messages_per_account);
                println!("media_retention_ms={}", p.media_retention_ms);
                println!(
                    "max_media_bytes_per_account={}",
                    p.max_media_bytes_per_account
                );
                println!("max_single_media_bytes={}", p.max_single_media_bytes);
            }
        }
        QuoteCommand::Find {
            conversation,
            message_id,
        } => {
            let store = open_quote_store()?;
            match store.find(&account_id, conversation, message_id) {
                Some(r) => {
                    if cli.json {
                        emit_json(&json!({
                            "account_id": r.account_id,
                            "conversation_id": r.conversation_id,
                            "message_id": r.message_id,
                            "direction": match r.direction { Direction::Inbound => "inbound", Direction::Outbound => "outbound" },
                            "body": r.body,
                            "media_path": r.media_path,
                            "media_mime": r.media_mime,
                            "created_at": r.created_at,
                        }));
                    } else {
                        println!("body={}", r.body);
                        println!("direction={:?}", r.direction);
                        println!("media_path={}", r.media_path.as_deref().unwrap_or("(none)"));
                    }
                }
                None => {
                    if cli.json {
                        emit_json(&serde_json::Value::Null);
                    } else {
                        println!("(not found)");
                    }
                }
            }
        }
        QuoteCommand::Put {
            conversation,
            message_id,
            body,
            direction,
            media_path,
            media_mime,
        } => {
            let mut store = open_quote_store()?;
            let dir = match direction.as_str() {
                "outbound" => Direction::Outbound,
                _ => Direction::Inbound,
            };
            store.put(&QuoteMessageInput {
                account_id: account_id.clone(),
                conversation_id: conversation.clone(),
                message_id: message_id.clone(),
                direction: dir,
                body: body.clone(),
                source_media_path: media_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned()),
                media_mime: media_mime.clone(),
                media_name: None,
                created_at: 0,
            })?;
            if cli.json {
                emit_json(&json!({ "stored": true }));
            } else {
                println!("stored message {message_id}");
            }
        }
        QuoteCommand::Gc => {
            let mut store = open_quote_store()?;
            store.gc();
            if cli.json {
                emit_json(&json!({ "gc": true }));
            } else {
                println!("quote cache gc complete");
            }
        }
        QuoteCommand::Clear => {
            let mut store = open_quote_store()?;
            store.delete_account(&account_id);
            if cli.json {
                emit_json(&json!({ "cleared": account_id }));
            } else {
                println!("cleared quote cache for {account_id}");
            }
        }
        QuoteCommand::MediaDir => {
            let store = open_quote_store()?;
            let dir = store.account_media_dir(&account_id);
            if cli.json {
                emit_json(&json!({ "path": dir.to_string_lossy() }));
            } else {
                println!("{}", dir.display());
            }
        }
    }
    Ok(())
}

fn context_cmd(cli: &Cli, command: &ContextCommand) -> Result<()> {
    let account_id = effective_account_id(cli).unwrap_or_else(|| "default".to_string());
    let mut store = ContextTokenStore::new();
    store.restore(&account_id);
    match command {
        ContextCommand::Get { user } => match store.get(&account_id, user) {
            Some(token) => {
                if cli.json {
                    emit_json(&json!({ "token": token }));
                } else {
                    println!("{token}");
                }
            }
            None => {
                if cli.json {
                    emit_json(&serde_json::Value::Null);
                } else {
                    println!("(none)");
                }
            }
        },
        ContextCommand::Set { user, token } => {
            store.set(&account_id, user, token);
            if cli.json {
                emit_json(&json!({ "stored": true }));
            } else {
                println!("stored token for {user}");
            }
        }
        ContextCommand::Clear => {
            store.clear_account(&account_id);
            if cli.json {
                emit_json(&json!({ "cleared": true }));
            } else {
                println!("cleared context tokens for {account_id}");
            }
        }
        ContextCommand::Restore => {
            let count = store.restore(&account_id);
            if cli.json {
                emit_json(&json!({ "restored": count }));
            } else {
                println!("restored {count} context token(s)");
            }
        }
    }
    Ok(())
}

fn sync_cmd(cli: &Cli, command: &SyncCommand) -> Result<()> {
    let account_id = effective_account_id(cli).unwrap_or_else(|| "default".to_string());
    match command {
        SyncCommand::Show => {
            let buf = sync_buf::load_for_account(&account_id);
            if cli.json {
                emit_json(&json!({ "get_updates_buf": buf }));
            } else {
                println!("{}", buf.as_deref().unwrap_or("(none)"));
            }
        }
        SyncCommand::Set { buf } => {
            sync_buf::save_for_account(&account_id, buf)?;
            if cli.json {
                emit_json(&json!({ "saved": true }));
            } else {
                println!("saved cursor for {account_id}");
            }
        }
        SyncCommand::Clear => {
            sync_buf::clear_for_account(&account_id);
            if cli.json {
                emit_json(&json!({ "cleared": true }));
            } else {
                println!("cleared cursor for {account_id}");
            }
        }
    }
    Ok(())
}

fn session_cmd(cli: &Cli, command: &SessionCommand) -> Result<()> {
    let account_id = effective_account_id(cli).unwrap_or_else(|| "default".to_string());
    match command {
        SessionCommand::Status => {
            let remaining = pause_remaining_ms(&account_id);
            let paused = remaining > 0;
            if cli.json {
                emit_json(
                    &json!({ "account_id": account_id, "paused": paused, "remaining_ms": remaining }),
                );
            } else {
                println!("account_id={account_id}");
                println!("paused={paused}");
                println!("remaining_ms={remaining}");
            }
        }
        SessionCommand::Pause => {
            session_guard::pause_session(&account_id);
            persist_pause(
                &account_id,
                session_guard::SESSION_PAUSE_DURATION.as_millis(),
            );
            if cli.json {
                emit_json(&json!({ "paused": true }));
            } else {
                println!("paused {account_id} for 1 hour");
            }
        }
        SessionCommand::Resume => {
            session_guard::resume_session(&account_id);
            clear_persisted_pause(&account_id);
            if cli.json {
                emit_json(&json!({ "resumed": true }));
            } else {
                println!("resumed {account_id}");
            }
        }
        SessionCommand::Assert => {
            assert_not_paused(&account_id)?;
            session_guard::assert_session_active(&account_id)?;
            if cli.json {
                emit_json(&json!({ "active": true }));
            } else {
                println!("{account_id} is active");
            }
        }
    }
    Ok(())
}

fn cdn_cmd(cli: &Cli, command: &CdnCommand) -> Result<()> {
    let base = cli
        .cdn_base_url
        .clone()
        .unwrap_or_else(|| accounts::CDN_BASE_URL.to_string());
    match command {
        CdnCommand::DownloadUrl { param } => {
            let url = ilink_sdk::cdn::build_cdn_download_url(param, &base)?;
            if cli.json {
                emit_json(&json!({ "url": url }));
            } else {
                println!("{url}");
            }
        }
        CdnCommand::UploadUrl { param, filekey } => {
            let url = ilink_sdk::cdn::build_cdn_upload_url(&base, param, filekey)?;
            if cli.json {
                emit_json(&json!({ "url": url }));
            } else {
                println!("{url}");
            }
        }
    }
    Ok(())
}

/// Render a QR code as half-block characters on stdout.
///
/// Weixin passes a URL, and this SDK has no terminal-aware dependency, so the
/// code is rendered from a downgraded UTF-8-friendly representation using
/// `█` / `▀` / `▄` / ` `, which fits any UTF-8 terminal.
fn print_qr(url: &str) {
    use qrcode::QrCode;

    let Ok(code) = QrCode::new(url.as_bytes()) else {
        eprintln!("warning: could not encode QR code");
        return;
    };

    let width = code.width();
    let colors = code.to_colors();
    // Two rows per output line.
    for row in (0..width).step_by(2) {
        let mut line = String::with_capacity(width);
        for col in 0..width {
            let top = colors[row * width + col];
            let bottom = colors
                .get((row + 1) * width + col)
                .copied()
                .unwrap_or(qrcode::Color::Light);
            line.push_str(match (top, bottom) {
                (qrcode::Color::Dark, qrcode::Color::Dark) => "█",
                (qrcode::Color::Dark, qrcode::Color::Light) => "▀",
                (qrcode::Color::Light, qrcode::Color::Dark) => "▄",
                (qrcode::Color::Light, qrcode::Color::Light) => " ",
            });
        }
        println!("{}", line.trim_end());
    }
}
