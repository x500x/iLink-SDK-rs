//! CLI entry point for the Weixin SDK.
//!
//! Wraps the library in a small command-line tool so the SDK's behaviour can be
//! exercised end-to-end without writing code first.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use ilink_sdk::api::WeixinApiOptions;
use ilink_sdk::client::{WeixinClient, WeixinClientBuilder};
use ilink_sdk::logging::{init_tracing, Level};
use ilink_sdk::messaging::SendOpts;
use ilink_sdk::storage::accounts;

/// Weixin (iLink) SDK command line.
#[derive(Debug, Parser)]
#[command(
    name = "weixin",
    version,
    about = "Weixin (iLink) backend SDK: QR login, long-poll receive, media send",
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
    },

    /// List locally registered accounts.
    Accounts,

    /// Forget a local account (credentials + cursor + quote cache).
    Logout {
        /// The account id to forget.
        id: String,
    },

    /// Long-poll for inbound messages and print them.
    Listen {
        /// Stop after this many seconds (0 = forever).
        #[arg(long, default_value = "0")]
        duration: u64,
        /// Long-poll timeout per request, in milliseconds.
        #[arg(long, default_value = "35000")]
        poll_timeout: u64,
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

    /// Resolve and print the effective settings for an account.
    Status,
}

/// Build a client for API-only work, tolerating a missing token.
fn api_only_client_cli(cli: &Cli) -> ilink_sdk::error::Result<ilink_sdk::api::WeixinApiClient> {
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
    ilink_sdk::api::WeixinApiClient::new(opts)
}

fn main() {
    let cli = Cli::parse();
    init_tracing(cli.log_level);

    if let Err(err) = run(&cli) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run(cli: &Cli) -> ilink_sdk::error::Result<()> {
    match &cli.command {
        Command::Accounts => {
            let ids = accounts::list_indexed_account_ids();
            if ids.is_empty() {
                println!("no accounts registered — run `weixin login`");
                return Ok(());
            }
            for id in &ids {
                let a = accounts::resolve_account(id)?;
                println!(
                    "{id}\n  base_url={}\n  cdn_base_url={}\n  configured={}\n  user={}",
                    a.base_url,
                    a.cdn_base_url,
                    a.configured,
                    a.user_id.as_deref().unwrap_or("(none)")
                );
            }
            Ok(())
        }

        Command::Logout { id } => {
            let normalized = accounts::normalize_account_id(id);
            accounts::clear_account(&normalized)?;
            accounts::unregister_account_id(&normalized)?;
            println!("forgot account {normalized}");
            Ok(())
        }

        Command::Status => {
            let rt = runtime()?;
            rt.block_on(show_status(cli))
        }

        Command::Login { timeout, show_qr } => {
            let rt = runtime()?;
            rt.block_on(login(cli, *timeout, *show_qr))
        }

        Command::Listen {
            duration,
            poll_timeout,
        } => {
            let rt = runtime()?;
            rt.block_on(listen(cli, *duration, *poll_timeout))
        }

        Command::Send {
            to,
            text,
            context_token,
        } => {
            let rt = runtime()?;
            rt.block_on(send_text_cmd(cli, to, text, context_token.as_deref()))
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
    }
}

fn runtime() -> ilink_sdk::error::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| ilink_sdk::error::Error::unknown(e.to_string()))
}

async fn build_client(cli: &Cli) -> ilink_sdk::error::Result<WeixinClient> {
    WeixinClientBuilder::new()
        .account_id_opt(cli.account.clone())
        .base_url_opt(cli.base_url.clone())
        .cdn_base_url_opt(cli.cdn_base_url.clone())
        .build()
        .await
}

async fn show_status(cli: &Cli) -> ilink_sdk::error::Result<()> {
    let client = build_client(cli).await?;
    let a = client.account();
    println!("account_id={}", a.account_id);
    println!("base_url={}", a.base_url);
    println!("cdn_base_url={}", a.cdn_base_url);
    println!("configured={}", a.configured);
    println!(
        "token={}",
        client.token().map(|_| "present").unwrap_or("missing")
    );
    Ok(())
}

async fn send_text_cmd(
    cli: &Cli,
    to: &str,
    text: &str,
    context_token: Option<&str>,
) -> ilink_sdk::error::Result<()> {
    let client = build_client(cli).await?;
    let opts = SendOpts {
        context_token: context_token.map(str::to_string),
        ..Default::default()
    };
    let result = client.sender().send_text(to, text, &opts).await?;
    println!("sent clientId={}", result.message_id);
    if let Some(id) = result.server_message_id {
        println!("serverMessageId={id}");
    }
    Ok(())
}

async fn send_file_cmd(
    cli: &Cli,
    to: &str,
    path: &std::path::Path,
    caption: &str,
    context_token: Option<&str>,
) -> ilink_sdk::error::Result<()> {
    let client = build_client(cli).await?;
    let cdn = client.account().cdn_base_url.clone();
    let opts = SendOpts {
        context_token: context_token.map(str::to_string),
        ..Default::default()
    };
    let result = client
        .sender()
        .send_file(to, path, caption, &opts, &cdn)
        .await?;
    println!("sent clientId={}", result.message_id);
    Ok(())
}

async fn login(cli: &Cli, timeout: u64, show_qr: bool) -> ilink_sdk::error::Result<()> {
    // Login works without a stored account: the QR flow issues the token.
    let api = api_only_client_cli(cli)?;
    let qr = ilink_sdk::messaging::QrLogin::new(&api);

    let start = qr.start(None).await?;
    println!("{}", start.qrcode_url);
    if show_qr {
        print_qr(&start.qrcode_url);
    }
    println!("waiting for scan (session {})", start.session_key);

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
                    println!("{prompt}");
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
                    println!("qr refreshed: {url}");
                }
            },
        )
        .await?;

    match result.session {
        Some(session) => {
            let id = accounts::normalize_account_id(&session.bot_id);
            accounts::save_account(
                &id,
                &accounts::AccountUpdate {
                    token: Some(session.token),
                    base_url: if session.base_url.is_empty() {
                        None
                    } else {
                        Some(session.base_url)
                    },
                    user_id: Some(session.user_id),
                },
            )?;
            accounts::register_account_id(&id)?;
            println!("connected; saved account {id}");
        }
        None => {
            println!("{}", result.message);
            if !result.already_connected {
                std::process::exit(2);
            }
        }
    }
    Ok(())
}
async fn listen(cli: &Cli, duration: u64, poll_timeout: u64) -> ilink_sdk::error::Result<()> {
    let client = WeixinClientBuilder::new()
        .account_id_opt(cli.account.clone())
        .base_url_opt(cli.base_url.clone())
        .cdn_base_url_opt(cli.cdn_base_url.clone())
        .build()
        .await?;
    let account_id = client.account_id().to_string();
    let api = client.api().clone();

    let cursor_path = ilink_sdk::storage::sync_buf::get_sync_buf_path(&account_id);
    let mut buf =
        ilink_sdk::storage::sync_buf::load_get_updates_buf(&cursor_path).unwrap_or_default();

    println!("listening on account {account_id}");
    client.notify_start().await.ok();

    let start = std::time::Instant::now();
    loop {
        if duration > 0 && start.elapsed().as_secs() >= duration {
            break;
        }
        let resp = api
            .get_updates(
                &ilink_sdk::api::types::GetUpdatesReq {
                    get_updates_buf: Some(buf.clone()),
                    sync_buf: None,
                },
                Some(poll_timeout),
                None,
            )
            .await?;

        if resp.is_api_error() {
            eprintln!(
                "getUpdates error ret={:?} errcode={:?} errmsg={:?}",
                resp.ret, resp.errcode, resp.errmsg
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }
        if let Some(next) = resp.get_updates_buf.as_deref().filter(|s| !s.is_empty()) {
            buf = next.to_string();
            let _ = ilink_sdk::storage::sync_buf::save_get_updates_buf(&cursor_path, &buf);
        }

        for msg in resp.msgs.as_deref().unwrap_or(&[]) {
            let from = msg.from_user_id.as_deref().unwrap_or("");
            let body =
                ilink_sdk::messaging::body_from_items(msg.item_list.as_deref().unwrap_or(&[]));
            println!(
                "inbound from={from} id={:?} body={body}",
                msg.message_id.as_deref().unwrap_or("")
            );

            // Echo slash commands so the CLI is usable on its own.
            let trimmed = body.trim();
            if trimmed == "/ping" {
                client
                    .sender()
                    .send_text(
                        from,
                        "pong",
                        &SendOpts {
                            context_token: msg.context_token.clone(),
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
