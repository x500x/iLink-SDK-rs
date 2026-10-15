//! QR-code login flow.
//!
//! Ports `src/auth/login-qr.ts`:
//!
//! 1. `POST /ilink/bot/get_bot_qrcode?bot_type=3` — returns a `qrcode` value
//!    and a URL to render. Up to ten locally stored bot tokens are sent along.
//! 2. `GET /ilink/bot/get_qrcode_status?qrcode=...` — long-polls the status.
//!    When the server asks for verification the client adds
//!    `&verify_code=...`.
//! 3. On `confirmed`, the response carries the new bot token, bot id, API base
//!    URL and the paired Weixin user id.
//!
//! Statuses handled: `wait`, `scaned`, `confirmed`, `expired`,
//! `need_verifycode`, `verify_code_blocked`, `scaned_but_redirect`,
//! `binded_redirect`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::api::WeixinApiClient;
use crate::error::{Error, ErrorKind, Result};
use crate::storage::accounts::list_indexed_account_ids;
use crate::storage::accounts::load_account;
use crate::util::random_u32;

/// Default `bot_type` for the ilink QR endpoints (this channel build).
pub const DEFAULT_ILINK_BOT_TYPE: &str = "3";

/// Fixed API base URL for all QR-code requests.
pub const FIXED_BASE_URL: &str = "https://ilinkai.weixin.qq.com";

/// How long a QR-code session stays valid client-side.
pub const ACTIVE_LOGIN_TTL: Duration = Duration::from_secs(5 * 60);

/// Client-side timeout for the long-poll `get_qrcode_status` request.
pub const QR_LONG_POLL_TIMEOUT_MS: u64 = 35_000;

/// Maximum number of times an expired QR code may be refreshed before the
/// flow gives up.
pub const MAX_QR_REFRESH_COUNT: usize = 3;

/// Poll interval between QR status requests (1s in the TypeScript plugin).
pub const QR_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Response of `get_bot_qrcode`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QrCodeResponse {
    pub qrcode: String,
    pub qrcode_img_content: String,
}

/// Status values reported by the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrStatus {
    Wait,
    Scaned,
    Confirmed,
    Expired,
    ScanButRedirect,
    NeedVerifyCode,
    VerifyCodeBlocked,
    BindedRedirect,
    Unknown,
}

impl QrStatus {
    /// Parse the wire string into a known status.
    pub fn parse(raw: &str) -> Self {
        match raw {
            "wait" => QrStatus::Wait,
            "scaned" => QrStatus::Scaned,
            "confirmed" => QrStatus::Confirmed,
            "expired" => QrStatus::Expired,
            "scaned_but_redirect" => QrStatus::ScanButRedirect,
            "need_verifycode" => QrStatus::NeedVerifyCode,
            "verify_code_blocked" => QrStatus::VerifyCodeBlocked,
            "binded_redirect" => QrStatus::BindedRedirect,
            _ => QrStatus::Unknown,
        }
    }
}

/// Response of `get_qrcode_status`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusResponse {
    pub status: String,
    pub bot_token: Option<String>,
    pub ilink_bot_id: Option<String>,
    #[serde(default)]
    pub baseurl: Option<String>,
    /// The user id of the person who scanned the QR code.
    #[serde(default)]
    pub ilink_user_id: Option<String>,
    /// New host to redirect polling to when `scaned_but_redirect`.
    #[serde(default)]
    pub redirect_host: Option<String>,
}

/// A login session that has started but not yet completed.
#[derive(Debug, Clone)]
struct ActiveLogin {
    qrcode: String,
    qrcode_url: String,
    started_at: Instant,
    /// Effective polling base URL; updated on IDC redirect.
    current_api_base_url: String,
    /// Verification code awaiting submission on the next poll.
    pending_verify_code: Option<String>,
}

fn registry() -> &'static Mutex<HashMap<String, ActiveLogin>> {
    static R: std::sync::OnceLock<Mutex<HashMap<String, ActiveLogin>>> = std::sync::OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

fn purge_expired() {
    let mut map = registry().lock().expect("qr registry poisoned");
    let now = Instant::now();
    map.retain(|_, login| now.duration_since(login.started_at) < ACTIVE_LOGIN_TTL);
}

fn lookup_session(session_key: &str) -> Result<ActiveLogin> {
    let map = registry().lock().expect("qr registry poisoned");
    match map.get(session_key) {
        None => Err(Error::new(
            ErrorKind::Unknown,
            "no active login session — call qr_login().start() first",
        )),
        Some(login) => {
            let age = login.started_at.elapsed();
            if age >= ACTIVE_LOGIN_TTL {
                Err(Error::new(
                    ErrorKind::Unknown,
                    "QR code expired, re-run start() to get a fresh one",
                ))
            } else {
                Ok(login.clone())
            }
        }
    }
}

/// Read up to the ten most recent locally stored bot tokens.
pub fn local_token_list() -> Vec<String> {
    let ids = list_indexed_account_ids();
    let mut tokens = Vec::new();
    for id in ids.iter().rev() {
        if tokens.len() >= 10 {
            break;
        }
        if let Some(token) = load_account(id)
            .and_then(|d| d.token)
            .filter(|t| !t.trim().is_empty())
        {
            tokens.push(token);
        }
    }
    tokens
}

/// Builder for the QR-code login flow, bound to one API client.
pub struct QrLogin<'a> {
    client: &'a WeixinApiClient,
    bot_type: String,
}

impl<'a> QrLogin<'a> {
    /// Create a login builder over an existing client.
    pub fn new(client: &'a WeixinApiClient) -> Self {
        QrLogin {
            client,
            bot_type: DEFAULT_ILINK_BOT_TYPE.to_string(),
        }
    }

    /// Override the `bot_type` query parameter.
    pub fn with_bot_type(mut self, bot_type: impl Into<String>) -> Self {
        self.bot_type = bot_type.into();
        self
    }

    /// Request a fresh QR code and register the resulting session.
    ///
    /// Returns the URL to display plus the session key to pass to
    /// [`QrLogin::wait`].
    pub async fn start(&self, session_key: Option<&str>) -> Result<QrStartResult> {
        purge_expired();

        let key = session_key
            .filter(|k| !k.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("qr-{}", random_u32()));

        let qr = self.fetch_qr_code().await?;

        let login = ActiveLogin {
            qrcode: qr.qrcode.clone(),
            qrcode_url: qr.qrcode_img_content.clone(),
            started_at: Instant::now(),
            current_api_base_url: FIXED_BASE_URL.to_string(),
            pending_verify_code: None,
        };
        registry()
            .lock()
            .expect("qr registry poisoned")
            .insert(key.clone(), login);

        Ok(QrStartResult {
            qrcode: qr.qrcode,
            qrcode_url: qr.qrcode_img_content,
            session_key: key,
        })
    }

    /// Fetch a QR code from the fixed API base URL.
    pub async fn fetch_qr_code(&self) -> Result<QrCodeResponse> {
        let endpoint = format!(
            "ilink/bot/get_bot_qrcode?bot_type={}",
            urlencoding(&self.bot_type)
        );
        let body = serde_json::json!({ "local_token_list": local_token_list() });
        let text = self
            .client
            .post_text_unauthenticated(&endpoint, &body, Some(QR_LONG_POLL_TIMEOUT_MS))
            .await?;
        Ok(serde_json::from_slice::<QrCodeResponse>(text.as_bytes())?)
    }

    /// Poll the status once and advance the state machine.
    ///
    /// Returns [`PollOutcome::Done`] when the flow has concluded, otherwise
    /// [`PollOutcome::Continue`] (which may carry a new QR code after a
    /// refresh, or a verification prompt).
    pub async fn poll(&self, session_key: &str) -> Result<PollOutcome> {
        let login = lookup_session(session_key)?;

        let status = self.poll_status(&login).await?;
        let kind = QrStatus::parse(&status.status);

        match kind {
            QrStatus::Wait => {
                tracing::debug!("qr status: wait");
                Ok(PollOutcome::Continue)
            }
            QrStatus::Scaned => {
                if login.pending_verify_code.is_some() {
                    tracing::info!("verify code accepted, clearing pending code");
                    let mut map = registry().lock().expect("qr registry poisoned");
                    if let Some(entry) = map.get_mut(session_key) {
                        entry.pending_verify_code = None;
                    }
                }
                Ok(PollOutcome::Continue)
            }
            QrStatus::NeedVerifyCode => Ok(PollOutcome::NeedsVerification {
                retry: login.pending_verify_code.is_some(),
            }),
            QrStatus::Expired => Ok(PollOutcome::Expired),
            QrStatus::VerifyCodeBlocked => Ok(PollOutcome::VerifyCodeBlocked),
            QrStatus::BindedRedirect => {
                self.remove(session_key);
                Ok(PollOutcome::AlreadyConnected)
            }
            QrStatus::ScanButRedirect => {
                if let Some(host) = status.redirect_host.as_deref().filter(|h| !h.is_empty()) {
                    let new_base = format!("https://{host}");
                    tracing::info!(host = %host, "qr status: IDC redirect");
                    let mut map = registry().lock().expect("qr registry poisoned");
                    if let Some(entry) = map.get_mut(session_key) {
                        entry.current_api_base_url = new_base;
                    }
                } else {
                    tracing::warn!("qr status: scaned_but_redirect without redirect_host");
                }
                Ok(PollOutcome::Continue)
            }
            QrStatus::Confirmed => {
                let bot_id = status.ilink_bot_id.as_deref().filter(|s| !s.is_empty());
                match bot_id {
                    None => {
                        self.remove(session_key);
                        Err(Error::new(
                            ErrorKind::Unknown,
                            "login confirmed but ilink_bot_id missing from response",
                        ))
                    }
                    Some(bot_id) => {
                        self.remove(session_key);
                        Ok(PollOutcome::Done(QrSession {
                            token: status.bot_token.clone().unwrap_or_default(),
                            bot_id: bot_id.to_string(),
                            base_url: status.baseurl.clone().unwrap_or_default(),
                            user_id: status.ilink_user_id.clone().unwrap_or_default(),
                        }))
                    }
                }
            }
            QrStatus::Unknown => {
                tracing::debug!(status = %status.status, "qr status: unrecognized, waiting");
                Ok(PollOutcome::Continue)
            }
        }
    }

    /// Submit a verification code obtained from the phone.
    pub fn submit_verify_code(&self, session_key: &str, code: &str) {
        let mut map = registry().lock().expect("qr registry poisoned");
        if let Some(entry) = map.get_mut(session_key) {
            entry.pending_verify_code = Some(code.trim().to_string());
        }
    }

    /// Forget a session.
    pub fn remove(&self, session_key: &str) {
        registry()
            .lock()
            .expect("qr registry poisoned")
            .remove(session_key);
    }

    /// Whether a live (unexpired) session exists for the key.
    pub fn has_session(session_key: &str) -> bool {
        lookup_session(session_key).is_ok()
    }

    async fn poll_status(&self, login: &ActiveLogin) -> Result<StatusResponse> {
        let endpoint = match &login.pending_verify_code {
            Some(code) if !code.is_empty() => format!(
                "ilink/bot/get_qrcode_status?qrcode={}&verify_code={}",
                urlencoding(&login.qrcode),
                urlencoding(code)
            ),
            _ => format!(
                "ilink/bot/get_qrcode_status?qrcode={}",
                urlencoding(&login.qrcode)
            ),
        };

        match self
            .client
            .get_text(&endpoint, QR_LONG_POLL_TIMEOUT_MS)
            .await
        {
            Ok(text) => Ok(serde_json::from_slice::<StatusResponse>(text.as_bytes())?),
            Err(err) if err.is_abort() => {
                tracing::debug!("qr poll: client-side timeout, treating as wait");
                Ok(StatusResponse {
                    status: "wait".to_string(),
                    bot_token: None,
                    ilink_bot_id: None,
                    baseurl: None,
                    ilink_user_id: None,
                    redirect_host: None,
                })
            }
            // Gateway timeouts (e.g. Cloudflare 524) and other network errors
            // are treated as "still waiting" and retried on the next poll.
            Err(err) => {
                tracing::warn!("qr poll: network/gateway error, will retry: {err}");
                Ok(StatusResponse {
                    status: "wait".to_string(),
                    bot_token: None,
                    ilink_bot_id: None,
                    baseurl: None,
                    ilink_user_id: None,
                    redirect_host: None,
                })
            }
        }
    }

    /// Drive the whole flow to completion.
    ///
    /// `on_verification_required` is invoked when the server asks for the
    /// code displayed on the phone; it must return the code the user typed.
    /// Any error it returns aborts the login with that error, an empty string
    /// is equivalent to an incorrect code.
    ///
    /// `on_refresh` is invoked whenever the QR code is replaced so the host
    /// application can re-render it.
    pub async fn wait<F, Fut, G, GFut>(
        &self,
        session_key: &str,
        timeout: Duration,
        mut on_verification_required: F,
        mut on_refresh: G,
    ) -> Result<QrWaitResult>
    where
        F: FnMut(bool) -> Fut,
        Fut: std::future::Future<Output = Result<String>>,
        G: FnMut(&QrStartResult) -> GFut,
        GFut: std::future::Future<Output = ()>,
    {
        lookup_session(session_key)?;

        let deadline = Instant::now() + timeout;
        let mut refresh_count = 1usize;

        while Instant::now() < deadline {
            match self.poll(session_key).await? {
                PollOutcome::Done(session) => return Ok(QrWaitResult::connected(session)),
                PollOutcome::AlreadyConnected => return Ok(QrWaitResult::already_connected()),
                PollOutcome::Continue => {}
                PollOutcome::NeedsVerification { retry } => {
                    let code = on_verification_required(retry).await?;
                    self.submit_verify_code(session_key, &code);
                    // Poll again immediately, without the 1s sleep.
                    continue;
                }
                PollOutcome::Expired | PollOutcome::VerifyCodeBlocked => {
                    refresh_count += 1;
                    if refresh_count > MAX_QR_REFRESH_COUNT {
                        self.remove(session_key);
                        return Ok(QrWaitResult::failed(
                            if matches!(std::mem::discriminant(&PollOutcome::Expired), _) {
                                "QR code expired too many times, connection flow stopped"
                            } else {
                                "too many incorrect verification attempts, connection flow stopped"
                            },
                        ));
                    }
                    match self.refresh(session_key).await {
                        Ok(start) => on_refresh(&start).await,
                        Err(err) => {
                            self.remove(session_key);
                            return Ok(QrWaitResult::failed(&format!("QR refresh failed: {err}")));
                        }
                    }
                }
            }
            tokio::time::sleep(QR_POLL_INTERVAL).await;
        }

        self.remove(session_key);
        Ok(QrWaitResult::failed("login timed out, please try again"))
    }

    /// Replace the QR code of a live session with a fresh one.
    pub async fn refresh(&self, session_key: &str) -> Result<QrStartResult> {
        lookup_session(session_key)?;
        let qr = self.fetch_qr_code().await?;
        {
            let mut map = registry().lock().expect("qr registry poisoned");
            if let Some(entry) = map.get_mut(session_key) {
                entry.qrcode = qr.qrcode.clone();
                entry.qrcode_url = qr.qrcode_img_content.clone();
                entry.started_at = Instant::now();
                entry.pending_verify_code = None;
            }
        }
        Ok(QrStartResult {
            qrcode: qr.qrcode,
            qrcode_url: qr.qrcode_img_content,
            session_key: session_key.to_string(),
        })
    }
}

/// Percent-encode a query-string component without pulling in a form crate.
///
/// Unreserved per RFC 3986 §2.3 pass through unchanged so the value stays
/// readable in logs and stable across round trips.
fn urlencoding(raw: &str) -> String {
    const FRAGMENT: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
        .add(b' ')
        .add(b'!')
        .add(b'"')
        .add(b'#')
        .add(b'$')
        .add(b'%')
        .add(b'&')
        .add(b'\'')
        .add(b'(')
        .add(b')')
        .add(b'*')
        .add(b'+')
        .add(b',')
        .add(b'/')
        .add(b':')
        .add(b';')
        .add(b'<')
        .add(b'=')
        .add(b'>')
        .add(b'?')
        .add(b'@')
        .add(b'[')
        .add(b'\\')
        .add(b']')
        .add(b'^')
        .add(b'`')
        .add(b'{')
        .add(b'|')
        .add(b'}');
    percent_encoding::utf8_percent_encode(raw, FRAGMENT).to_string()
}

/// Result of [`QrLogin::start`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrStartResult {
    /// The opaque `qrcode` value; not needed by callers that only render.
    pub qrcode: String,
    /// URL (or content) to render as the QR code.
    pub qrcode_url: String,
    /// Key passed back into [`QrLogin::wait`] / [`QrLogin::poll`].
    pub session_key: String,
}

/// Outcome of a single [`QrLogin::poll`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOutcome {
    /// Keep polling.
    Continue,
    /// The server wants the verification code shown on the phone.
    NeedsVerification {
        /// `true` when a previously submitted code was rejected.
        retry: bool,
    },
    /// QR code expired; refresh it.
    Expired,
    /// Too many wrong verification codes; the flow must refresh or stop.
    VerifyCodeBlocked,
    /// The bot is already bound to this instance; no new credentials were
    /// issued and existing local credentials remain valid.
    AlreadyConnected,
    /// Login succeeded.
    Done(QrSession),
}

/// Credentials obtained on a successful login.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QrSession {
    /// Bot token for authenticated API requests.
    pub token: String,
    /// Bot account id.
    pub bot_id: String,
    /// API base URL the backend assigned to this account.
    pub base_url: String,
    /// The Weixin user id that scanned; add to the allow list.
    pub user_id: String,
}

/// Result of [`QrLogin::wait`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrWaitResult {
    pub connected: bool,
    /// Set for `binded_redirect`: already paired, nothing to persist.
    pub already_connected: bool,
    pub session: Option<QrSession>,
    /// Failure explanation when `connected == false`.
    pub message: String,
}

impl QrWaitResult {
    fn connected(session: QrSession) -> Self {
        QrWaitResult {
            connected: true,
            already_connected: false,
            session: Some(session),
            message: "connected this instance to Weixin".to_string(),
        }
    }

    fn already_connected() -> Self {
        QrWaitResult {
            connected: false,
            already_connected: true,
            session: None,
            message: "already connected to this instance, no need to connect again".to_string(),
        }
    }

    fn failed(message: &str) -> Self {
        QrWaitResult {
            connected: false,
            already_connected: false,
            session: None,
            message: message.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_parsing() {
        assert_eq!(QrStatus::parse("wait"), QrStatus::Wait);
        assert_eq!(
            QrStatus::parse("scaned_but_redirect"),
            QrStatus::ScanButRedirect
        );
        assert_eq!(QrStatus::parse("binded_redirect"), QrStatus::BindedRedirect);
        assert_eq!(QrStatus::parse("something_else"), QrStatus::Unknown);
    }

    #[test]
    fn session_result_constructors() {
        let s = QrSession::default();
        let r = QrWaitResult::connected(s.clone());
        assert!(r.connected);
        assert!(!r.already_connected);
        assert_eq!(r.session, Some(s));

        let b = QrWaitResult::already_connected();
        assert!(!b.connected);
        assert!(b.already_connected);

        let f = QrWaitResult::failed("boom");
        assert!(!f.connected);
        assert!(f.session.is_none());
        assert_eq!(f.message, "boom");
    }

    #[test]
    fn url_encoding_keeps_query_safe() {
        assert_eq!(urlencoding("abc-123_xyz"), "abc-123_xyz");
        assert_eq!(urlencoding("a b+c/d"), "a%20b%2Bc%2Fd");
    }

    #[test]
    fn unknown_session_errors() {
        // No session registered under this key.
        let mut map = registry().lock().unwrap();
        map.clear();
        drop(map);
        assert!(lookup_session("missing").is_err());
    }

    #[test]
    fn purge_removes_expired() {
        {
            let mut map = registry().lock().unwrap();
            map.insert(
                "old".into(),
                ActiveLogin {
                    qrcode: "q".into(),
                    qrcode_url: "u".into(),
                    started_at: Instant::now() - ACTIVE_LOGIN_TTL - Duration::from_secs(1),
                    current_api_base_url: FIXED_BASE_URL.into(),
                    pending_verify_code: None,
                },
            );
        }
        purge_expired();
        assert!(!QrLogin::has_session("old"));
    }
}
