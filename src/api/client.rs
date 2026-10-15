//! HTTP client for the Weixin (iLink) backend API.
//!
//! Ports `src/api/api.ts`. Responsibilities:
//! * build the common headers (`iLink-App-Id`, `iLink-App-ClientVersion`,
//!   `AuthorizationType`, `Authorization`, `X-WECHAT-UIN`, `SKRouteTag`);
//! * attach `base_info` (channel version + sanitized `bot_agent`);
//! * apply per-operation timeouts and optional external cancellation;
//! * classify transport failures into [`ErrorKind`];
//! * decode uint64 identifiers losslessly.

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE};
use reqwest::{Client, Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::error::{Error, ErrorKind, Result};

use super::types::{
    BaseInfo, GetConfigResp, GetUpdatesReq, GetUpdatesResp, GetUploadUrlReq, GetUploadUrlResp,
    NotifyStartResp, NotifyStopResp, SendMessageReq, SendMessageResp, SendTypingReq,
    BOT_AGENT_MAX_LEN, DEFAULT_BOT_AGENT,
};

/// `iLink-App-Id` sent by this SDK. The TypeScript plugin reads it from
/// `package.json`'s `ilink_appid` field (`"bot"`).
pub const ILINK_APP_ID: &str = "bot";

/// Default long-poll timeout for `getUpdates`; the server may hold the
/// request up to this long.
pub const DEFAULT_LONG_POLL_TIMEOUT_MS: u64 = 35_000;
/// Default timeout for regular API calls (`sendMessage`, `getUploadUrl`).
pub const DEFAULT_API_TIMEOUT_MS: u64 = 15_000;
/// Default timeout for lightweight API calls (`getConfig`, `sendTyping`).
pub const DEFAULT_CONFIG_TIMEOUT_MS: u64 = 10_000;

/// Authorization scheme used for bot API requests.
pub const AUTHORIZATION_TYPE: &str = "ilink_bot_token";

/// Connection-level options shared by every request.
#[derive(Debug, Clone)]
pub struct WeixinApiOptions {
    /// API base URL, e.g. `https://ilinkai.weixin.qq.com`.
    pub base_url: String,
    /// Bot token from QR login.
    pub token: Option<String>,
    /// Override for the default request timeout.
    pub timeout_ms: Option<u64>,
    /// Optional route tag forwarded as the `SKRouteTag` header.
    pub route_tag: Option<String>,
    /// Optional CDN base URL override (used by the media pipeline).
    pub cdn_base_url: Option<String>,
}

impl WeixinApiOptions {
    /// Build options with only a base URL (used for pre-login requests).
    pub fn new(base_url: impl Into<String>) -> Self {
        WeixinApiOptions {
            base_url: base_url.into(),
            token: None,
            timeout_ms: None,
            route_tag: None,
            cdn_base_url: None,
        }
    }

    /// Attach a bot token, returning `self` for chaining.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn with_route_tag(mut self, tag: impl Into<String>) -> Self {
        self.route_tag = Some(tag.into());
        self
    }

    pub fn with_cdn_base_url(mut self, url: impl Into<String>) -> Self {
        self.cdn_base_url = Some(url.into());
        self
    }

    pub fn with_timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = Some(ms);
        self
    }
}

/// Transport abstraction so the API client can be unit-tested without network.
#[async_trait::async_trait]
pub trait HttpTransport: Send + Sync + 'static {
    /// Perform a request and return `(status, headers, body)`.
    async fn request(
        &self,
        method: Method,
        url: String,
        headers: HeaderMap,
        body: Option<Vec<u8>>,
        timeout: Option<Duration>,
        cancel: Option<CancellationToken>,
    ) -> Result<(StatusCode, HeaderMap, Vec<u8>)>;
}

/// Default transport backed by [`reqwest::Client`].
pub struct ReqwestTransport {
    client: Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self> {
        Ok(ReqwestTransport {
            client: Client::builder()
                .use_preconfigured_tls(crate::tls::default_tls_config()?)
                .build()?,
        })
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        ReqwestTransport::new().expect("failed to build default reqwest client")
    }
}

#[async_trait::async_trait]
impl HttpTransport for ReqwestTransport {
    async fn request(
        &self,
        method: Method,
        url: String,
        headers: HeaderMap,
        body: Option<Vec<u8>>,
        timeout: Option<Duration>,
        cancel: Option<CancellationToken>,
    ) -> Result<(StatusCode, HeaderMap, Vec<u8>)> {
        let mut req = self.client.request(method, &url).headers(headers);
        if let Some(body) = body {
            req = req.body(body);
        }

        let fut = req.send();
        let result = match (timeout, cancel) {
            (None, None) => fut.await.map_err(Error::from_reqwest_wrap),
            (Some(t), None) => match tokio::time::timeout(t, fut).await {
                Ok(r) => r.map_err(Error::from_reqwest_wrap),
                Err(_) => Err(Error::new(ErrorKind::Timeout, "request timeout")),
            },
            (None, Some(token)) => tokio::select! {
                biased;
                _ = token.cancelled() => Err(Error::new(ErrorKind::Cancelled, "request cancelled").with_external_cancel()),
                r = fut => r.map_err(Error::from_reqwest_wrap),
            },
            (Some(t), Some(token)) => tokio::select! {
                biased;
                _ = token.cancelled() => Err(Error::new(ErrorKind::Cancelled, "request cancelled").with_external_cancel()),
                r = tokio::time::timeout(t, fut) => match r {
                    Ok(Ok(resp)) => Ok(resp),
                    Ok(Err(e)) => Err(Error::from_reqwest_wrap(e)),
                    Err(_) => Err(Error::new(ErrorKind::Timeout, "request timeout")),
                },
            },
        };

        let resp = result?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp
            .bytes()
            .await
            .map_err(Error::from_reqwest_wrap)?
            .to_vec();
        Ok((status, headers, body))
    }
}

/// The Weixin API client.
#[derive(Clone)]
pub struct WeixinApiClient {
    opts: WeixinApiOptions,
    transport: Arc<dyn HttpTransport>,
    channel_version: Arc<str>,
    client_version: u32,
    bot_agent: Arc<str>,
}

impl std::fmt::Debug for WeixinApiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeixinApiClient")
            .field("base_url", &self.opts.base_url)
            .field("channel_version", &self.channel_version)
            .field("client_version", &self.client_version)
            .field("bot_agent", &self.bot_agent)
            .finish_non_exhaustive()
    }
}

impl WeixinApiClient {
    /// Build a client with the default reqwest transport.
    pub fn new(opts: WeixinApiOptions) -> Result<Self> {
        Ok(WeixinApiClient {
            opts,
            transport: Arc::new(ReqwestTransport::new()?),
            channel_version: Arc::from(crate::VERSION),
            client_version: build_client_version(crate::VERSION),
            bot_agent: Arc::from(DEFAULT_BOT_AGENT),
        })
    }

    /// Build a client with a custom transport (tests, proxies, instrumentation).
    pub fn with_transport(opts: WeixinApiOptions, transport: Arc<dyn HttpTransport>) -> Self {
        WeixinApiClient {
            opts,
            transport,
            channel_version: Arc::from(crate::VERSION),
            client_version: build_client_version(crate::VERSION),
            bot_agent: Arc::from(DEFAULT_BOT_AGENT),
        }
    }

    /// Override the `bot_agent` declaration. The value is sanitized.
    pub fn with_bot_agent(mut self, bot_agent: impl AsRef<str>) -> Self {
        self.bot_agent = Arc::from(sanitize_bot_agent(Some(bot_agent.as_ref())).as_str());
        self
    }

    /// Access the underlying options.
    pub fn options(&self) -> &WeixinApiOptions {
        &self.opts
    }

    fn base_url(&self) -> Result<String> {
        let trimmed = self.opts.base_url.trim();
        if trimmed.is_empty() {
            return Err(Error::new(ErrorKind::Unknown, "base_url is empty"));
        }
        Ok(ensure_trailing_slash(trimmed))
    }

    fn resolve_url(&self, endpoint: &str) -> Result<String> {
        let base = self.base_url()?;
        let base = url::Url::parse(&base)?;
        base.join(endpoint)
            .map(|u| u.to_string())
            .map_err(Into::into)
    }

    // -- headers ----------------------------------------------------------

    /// Headers shared by GET and POST requests.
    fn common_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("ilink-app-id"),
            HeaderValue::from_static(ILINK_APP_ID),
        );
        headers.insert(
            HeaderName::from_static("ilink-app-clientversion"),
            HeaderValue::from_str(&self.client_version.to_string())
                .unwrap_or_else(|_| HeaderValue::from_static("0")),
        );
        if let Some(tag) = self
            .opts
            .route_tag
            .as_ref()
            .filter(|t| !t.trim().is_empty())
        {
            if let Ok(value) = HeaderValue::from_str(tag.trim()) {
                headers.insert(HeaderName::from_static("skroutetag"), value);
            }
        }
        headers
    }

    /// Headers for JSON POST requests (adds auth + `X-WECHAT-UIN`).
    fn post_headers(&self) -> Result<HeaderMap> {
        let mut headers = self.common_headers();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            HeaderName::from_static("authorizationtype"),
            HeaderValue::from_static(AUTHORIZATION_TYPE),
        );
        let uin = crate::util::random_u32();
        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            uin.to_string().as_bytes(),
        );
        headers.insert(
            HeaderName::from_static("x-wechat-uin"),
            HeaderValue::from_str(&encoded).unwrap_or_else(|_| HeaderValue::from_static("MA==")),
        );
        if let Some(token) = self.opts.token.as_ref().filter(|t| !t.trim().is_empty()) {
            let value = format!("Bearer {}", token.trim());
            headers.insert(
                HeaderName::from_static("authorization"),
                HeaderValue::from_str(&value)?,
            );
        }
        Ok(headers)
    }

    /// The `base_info` payload included in every CGI request.
    pub fn base_info(&self) -> BaseInfo {
        build_base_info_with(&self.channel_version, &self.bot_agent)
    }

    // -- request helpers --------------------------------------------------

    async fn get_text_inner(
        &self,
        endpoint: &str,
        timeout: Option<Duration>,
        cancel: Option<CancellationToken>,
    ) -> Result<String> {
        let url = self.resolve_url(endpoint)?;
        let headers = self.common_headers();
        let (status, _headers, body) = self
            .transport
            .request(Method::GET, url.clone(), headers, None, timeout, cancel)
            .await?;
        let text = String::from_utf8_lossy(&body).into_owned();
        tracing::debug!(endpoint = %redact(&url), status = %status.as_u16(), "GET response");
        if !status.is_success() {
            return Err(Error::new(
                ErrorKind::Unknown,
                format!(
                    "{} {}: {}",
                    short_label(endpoint),
                    status.as_u16(),
                    crate::util::redact_body(&text, Some(200))
                ),
            ));
        }
        Ok(text)
    }

    async fn post_json<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &impl Serialize,
        timeout: Duration,
        cancel: Option<CancellationToken>,
    ) -> Result<T> {
        let url = self.resolve_url(endpoint)?;
        let headers = self.post_headers()?;
        let payload = serde_json::to_vec(body)?;
        let (status, _headers, resp_body) = self
            .transport
            .request(
                Method::POST,
                url.clone(),
                headers,
                Some(payload),
                Some(timeout),
                cancel,
            )
            .await?;
        let text = String::from_utf8_lossy(&resp_body).into_owned();
        tracing::debug!(endpoint = %redact(&url), status = %status.as_u16(), "POST response");
        if !status.is_success() {
            return Err(Error::new(
                ErrorKind::Unknown,
                format!(
                    "{} {}: {}",
                    short_label(endpoint),
                    status.as_u16(),
                    crate::util::redact_body(&text, Some(200))
                ),
            ));
        }
        parse_api_json(&text)
    }

    fn timeout(&self, default_ms: u64) -> Duration {
        Duration::from_millis(self.opts.timeout_ms.unwrap_or(default_ms))
    }
}

fn redact(url: &str) -> String {
    crate::util::redact_url(url)
}

fn short_label(endpoint: &str) -> String {
    endpoint.rsplit('/').next().unwrap_or(endpoint).to_string()
}

/// Ensure a base URL ends in `/` so relative joins behave like JS `new URL()`.
pub(crate) fn ensure_trailing_slash(url: &str) -> String {
    if url.ends_with('/') {
        url.to_string()
    } else {
        format!("{url}/")
    }
}

/// Encode a semver string as `0x00MMNNPP`.
pub fn build_client_version(version: &str) -> u32 {
    let parts: Vec<u32> = version
        .split('.')
        .map(|p| p.parse::<u32>().unwrap_or(0))
        .collect();
    let major = parts.first().copied().unwrap_or(0);
    let minor = parts.get(1).copied().unwrap_or(0);
    let patch = parts.get(2).copied().unwrap_or(0);
    ((major & 0xFF) << 16) | ((minor & 0xFF) << 8) | (patch & 0xFF)
}

/// Build the `base_info` payload for a specific channel version + agent.
pub fn build_base_info_with(channel_version: &str, bot_agent: &str) -> BaseInfo {
    BaseInfo {
        channel_version: Some(channel_version.to_string()),
        bot_agent: Some(sanitize_bot_agent(Some(bot_agent))),
    }
}

pub fn build_base_info() -> BaseInfo {
    BaseInfo {
        channel_version: Some(crate::VERSION.to_string()),
        bot_agent: Some(DEFAULT_BOT_AGENT.to_string()),
    }
}

/// Sanitize a user-supplied `botAgent` into a wire-safe UA-style string.
///
/// Grammar:
/// ```text
/// bot_agent = product *( SP product )
/// product   = name "/" version [ SP "(" comment ")" ]
/// name      = 1*32( ALPHA / DIGIT / "_" / "." / "-" )
/// version   = 1*32( ALPHA / DIGIT / "_" / "." / "+" / "-" )
/// comment   = 1*64( printable ASCII minus "(" ")" )
/// ```
///
/// Invalid tokens are dropped; an empty result falls back to
/// [`DEFAULT_BOT_AGENT`]. A result over [`BOT_AGENT_MAX_LEN`] bytes is
/// truncated by dropping trailing tokens.
pub fn sanitize_bot_agent(raw: Option<&str>) -> String {
    let raw = match raw {
        Some(r) if !r.trim().is_empty() => r.trim(),
        _ => return DEFAULT_BOT_AGENT.to_string(),
    };

    let accepted = parse_agent_tokens(raw);
    if accepted.is_empty() {
        return DEFAULT_BOT_AGENT.to_string();
    }

    let mut accepted = accepted;
    loop {
        let joined = accepted.join(" ");
        if joined.len() <= BOT_AGENT_MAX_LEN {
            return joined;
        }
        accepted.pop();
        if accepted.is_empty() {
            return DEFAULT_BOT_AGENT.to_string();
        }
    }
}

/// Split on whitespace, re-attach `(comment)` runs to the preceding product,
/// then validate each token.
fn parse_agent_tokens(raw: &str) -> Vec<String> {
    // Glue a multi-word comment onto its product.
    let mut tokens: Vec<String> = Vec::new();
    let words: Vec<&str> = raw.split_whitespace().collect();
    let mut idx = 0;
    while idx < words.len() {
        let word = words[idx];
        if word.starts_with('(') && !word.ends_with(')') {
            let mut acc = word.to_string();
            idx += 1;
            while idx < words.len() {
                acc.push(' ');
                acc.push_str(words[idx]);
                let done = words[idx].ends_with(')');
                idx += 1;
                if done {
                    break;
                }
            }
            tokens.push(acc);
        } else {
            tokens.push(word.to_string());
            idx += 1;
        }
    }

    let mut accepted: Vec<String> = Vec::new();
    let mut pending_product: Option<String> = None;
    for tok in tokens {
        if tok.starts_with('(') && tok.ends_with(')') && tok.len() >= 2 {
            let inner = &tok[1..tok.len() - 1];
            if let Some(product) = pending_product.take() {
                if is_comment(inner) {
                    accepted.push(format!("{product} ({inner})"));
                } else {
                    accepted.push(product);
                }
            }
            continue;
        }
        if let Some(product) = pending_product.take() {
            accepted.push(product);
        }
        if is_product(&tok) {
            pending_product = Some(tok);
        }
    }
    if let Some(product) = pending_product {
        accepted.push(product);
    }
    accepted
}

fn is_product(tok: &str) -> bool {
    let Some((name, version)) = tok.split_once('/') else {
        return false;
    };
    !name.is_empty()
        && name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        && !version.is_empty()
        && version.len() <= 32
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '+' | '-'))
}

fn is_comment(inner: &str) -> bool {
    !inner.is_empty()
        && inner.len() <= 64
        && inner
            .chars()
            .all(|c| matches!(c as u32, 0x20..=0x7E) && c != '(' && c != ')')
}

/// JSON fields whose numeric values must be quoted before parsing.
const LOSSLESS_ID_FIELDS: &[&str] = &["message_id", "msg_id", "svr_id"];

/// Parse API JSON, quoting uint64 identifier fields first so values above
/// `i64::MAX` survive decoding.
///
/// Mirrors `parseWeixinApiJson` in the TypeScript implementation.
pub fn parse_api_json<T: DeserializeOwned>(raw: &str) -> Result<T> {
    let rewritten = quote_lossless_ids(raw);
    Ok(serde_json::from_str(&rewritten)?)
}

/// Rewrite `"field": <number>` into `"field": "<number>"` for lossless id
/// fields. Only real object properties are rewritten; string contents are
/// never touched.
pub fn quote_lossless_ids(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len() + 16);
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] != b'"' {
            // Copy the run of non-quote bytes verbatim.
            let mut j = i;
            while j < bytes.len() && bytes[j] != b'"' {
                j += 1;
            }
            out.push_str(&raw[i..j]);
            i = j;
            continue;
        }

        let (token, next) = read_json_string_token(raw, i);
        out.push_str(&raw[i..next]);
        i = next;

        // Skip whitespace; a ':' must immediately follow for this to be a key.
        let mut cursor = i;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b':' {
            continue;
        }
        let key = unescape_json_string(&token);
        if !LOSSLESS_ID_FIELDS.contains(&key.as_str()) {
            continue;
        }

        // Copy the ':' plus any whitespace after it.
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        out.push_str(&raw[i..cursor]);
        i = cursor;

        // Only rewrite an unquoted integer; a value that is already a string
        // (or null / true / false) is left completely alone.
        let number_start = cursor;
        if bytes.get(cursor) == Some(&b'-') {
            cursor += 1;
        }
        let digits_start = cursor;
        while bytes.get(cursor).is_some_and(|b| b.is_ascii_digit()) {
            cursor += 1;
        }
        if cursor > digits_start {
            out.push('"');
            out.push_str(&raw[number_start..cursor]);
            out.push('"');
            i = cursor;
        }
    }

    out
}

fn read_json_string_token(input: &str, start: usize) -> (String, usize) {
    let bytes = input.as_bytes();
    let mut i = start + 1;
    let mut escaped = false;
    while i < bytes.len() {
        let c = bytes[i];
        i += 1;
        if escaped {
            escaped = false;
        } else if c == b'\\' {
            escaped = true;
        } else if c == b'"' {
            break;
        }
    }
    (input[start..i].to_string(), i)
}

fn unescape_json_string(token: &str) -> String {
    if token.len() < 2 {
        return String::new();
    }
    let inner = &token[1..token.len() - 1];
    serde_json::from_str::<String>(&format!("\"{inner}\"")).unwrap_or_else(|_| inner.to_string())
}

impl WeixinApiClient {
    /// Long-poll for inbound messages. The server holds the request until a
    /// message is available or the long-poll timeout is reached.
    ///
    /// On a client-side timeout or external cancellation this returns an
    /// empty `ret: 0` response carrying the *same* `get_updates_buf`, so the
    /// caller can simply retry. Any other error propagates.
    pub async fn get_updates(
        &self,
        req: &GetUpdatesReq,
        timeout_ms: Option<u64>,
        cancel: Option<CancellationToken>,
    ) -> Result<GetUpdatesResp> {
        let timeout = self.timeout(timeout_ms.unwrap_or(DEFAULT_LONG_POLL_TIMEOUT_MS));
        let body = GetUpdatesBody {
            get_updates_buf: req.get_updates_buf.clone(),
            base_info: Some(self.base_info()),
        };
        match self
            .post_json("ilink/bot/getupdates", &body, timeout, cancel)
            .await
        {
            Ok(resp) => Ok(resp),
            Err(err) if err.is_abort() => {
                tracing::debug!(
                    "getUpdates: client-side timeout or external cancel, returning empty response"
                );
                Ok(GetUpdatesResp {
                    ret: Some(0),
                    errcode: None,
                    errmsg: None,
                    msgs: Some(Vec::new()),
                    sync_buf: None,
                    get_updates_buf: req.get_updates_buf.clone(),
                    longpolling_timeout_ms: None,
                })
            }
            Err(err) => Err(err),
        }
    }

    /// Send a message downstream and return the server-assigned message id.
    ///
    /// A non-zero `ret` is an error; an absent `ret` is accepted.
    pub async fn send_message(&self, req: &SendMessageReq) -> Result<SendMessageResp> {
        let body = SendMessageBody {
            msg: req.msg.clone(),
            base_info: Some(self.base_info()),
        };
        let resp: SendMessageResp = self
            .post_json(
                "ilink/bot/sendmessage",
                &body,
                self.timeout(DEFAULT_API_TIMEOUT_MS),
                None,
            )
            .await?;
        if resp.is_error() {
            return Err(Error::new(
                ErrorKind::Unknown,
                format!(
                    "sendMessage ret={} errmsg={}",
                    resp.ret.unwrap_or_default(),
                    resp.errmsg.as_deref().unwrap_or("(none)")
                ),
            ));
        }
        Ok(resp)
    }

    /// Get a pre-signed CDN upload URL for a file.
    pub async fn get_upload_url(&self, req: &GetUploadUrlReq) -> Result<GetUploadUrlResp> {
        let body = GetUploadUrlBody {
            filekey: req.filekey.clone(),
            media_type: req.media_type,
            to_user_id: req.to_user_id.clone(),
            rawsize: req.rawsize,
            rawfilemd5: req.rawfilemd5.clone(),
            filesize: req.filesize,
            thumb_rawsize: req.thumb_rawsize,
            thumb_rawfilemd5: req.thumb_rawfilemd5.clone(),
            thumb_filesize: req.thumb_filesize,
            no_need_thumb: req.no_need_thumb,
            aeskey: req.aeskey.clone(),
            base_info: Some(self.base_info()),
        };
        self.post_json(
            "ilink/bot/getuploadurl",
            &body,
            self.timeout(DEFAULT_API_TIMEOUT_MS),
            None,
        )
        .await
    }

    /// Fetch bot config (including `typing_ticket`) for a user.
    pub async fn get_config(
        &self,
        ilink_user_id: &str,
        context_token: Option<&str>,
    ) -> Result<GetConfigResp> {
        let body = GetConfigBody {
            ilink_user_id: ilink_user_id.to_string(),
            context_token: context_token.map(str::to_string),
            base_info: Some(self.base_info()),
        };
        self.post_json(
            "ilink/bot/getconfig",
            &body,
            self.timeout(DEFAULT_CONFIG_TIMEOUT_MS),
            None,
        )
        .await
    }

    /// Send (or cancel) a typing indicator. The response body is ignored.
    pub async fn send_typing(&self, req: &SendTypingReq) -> Result<()> {
        let body = SendTypingBody {
            ilink_user_id: req.ilink_user_id.clone(),
            typing_ticket: req.typing_ticket.clone(),
            status: req.status,
            base_info: Some(self.base_info()),
        };
        let url = self.resolve_url("ilink/bot/sendtyping")?;
        let headers = self.post_headers()?;
        let payload = serde_json::to_vec(&body)?;
        let (status, _headers, body) = self
            .transport
            .request(
                Method::POST,
                url,
                headers,
                Some(payload),
                Some(self.timeout(DEFAULT_CONFIG_TIMEOUT_MS)),
                None,
            )
            .await?;
        if !status.is_success() {
            let text = String::from_utf8_lossy(&body);
            return Err(Error::new(
                ErrorKind::Unknown,
                format!(
                    "sendTyping {}: {}",
                    status.as_u16(),
                    crate::util::redact_body(&text, Some(200))
                ),
            ));
        }
        tracing::debug!("sendTyping: ok");
        Ok(())
    }

    /// Notify the backend that this channel client is stopping.
    pub async fn notify_stop(&self) -> Result<NotifyStopResp> {
        let body = BaseInfoBody {
            base_info: Some(self.base_info()),
        };
        self.post_json(
            "ilink/bot/msg/notifystop",
            &body,
            self.timeout(DEFAULT_CONFIG_TIMEOUT_MS),
            None,
        )
        .await
    }

    /// Notify the backend that this channel client is starting.
    pub async fn notify_start(&self) -> Result<NotifyStartResp> {
        let body = BaseInfoBody {
            base_info: Some(self.base_info()),
        };
        self.post_json(
            "ilink/bot/msg/notifystart",
            &body,
            self.timeout(DEFAULT_CONFIG_TIMEOUT_MS),
            None,
        )
        .await
    }

    /// Send a raw authenticated GET request and return the response text.
    ///
    /// Used by the QR-code status poller, which has its own URI encoding.
    pub async fn get_text(&self, endpoint: &str, timeout_ms: u64) -> Result<String> {
        self.get_text_inner(endpoint, Some(Duration::from_millis(timeout_ms)), None)
            .await
    }

    /// Send a raw unauthenticated POST request (QR-code fetch) and return text.
    pub async fn post_text_unauthenticated(
        &self,
        endpoint: &str,
        json_body: &impl Serialize,
        timeout_ms: Option<u64>,
    ) -> Result<String> {
        let url = self.resolve_url(endpoint)?;
        let mut headers = self.post_headers()?;
        // QR-code POST carries the JSON POST headers but no Authorization.
        headers.remove(HeaderName::from_static("authorization"));
        let payload = serde_json::to_vec(json_body)?;
        let (status, _headers, body) = self
            .transport
            .request(
                Method::POST,
                url,
                headers,
                Some(payload),
                timeout_ms.map(Duration::from_millis),
                None,
            )
            .await?;
        let text = String::from_utf8_lossy(&body).into_owned();
        if !status.is_success() {
            return Err(Error::new(
                ErrorKind::Unknown,
                format!(
                    "{} {}: {}",
                    short_label(endpoint),
                    status.as_u16(),
                    crate::util::redact_body(&text, Some(200))
                ),
            ));
        }
        Ok(text)
    }
}

// ---------------------------------------------------------------------------
// Internal request bodies (base_info is always attached server-side)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct GetUpdatesBody {
    get_updates_buf: Option<String>,
    base_info: Option<BaseInfo>,
}

#[derive(Debug, Serialize)]
struct SendMessageBody {
    msg: Option<crate::api::types::WeixinMessage>,
    base_info: Option<BaseInfo>,
}

#[derive(Debug, Serialize)]
struct GetUploadUrlBody {
    filekey: Option<String>,
    media_type: Option<i64>,
    to_user_id: Option<String>,
    rawsize: Option<i64>,
    rawfilemd5: Option<String>,
    filesize: Option<i64>,
    thumb_rawsize: Option<i64>,
    thumb_rawfilemd5: Option<String>,
    thumb_filesize: Option<i64>,
    no_need_thumb: Option<bool>,
    aeskey: Option<String>,
    base_info: Option<BaseInfo>,
}

#[derive(Debug, Serialize)]
struct GetConfigBody {
    ilink_user_id: String,
    context_token: Option<String>,
    base_info: Option<BaseInfo>,
}

#[derive(Debug, Serialize)]
struct SendTypingBody {
    ilink_user_id: Option<String>,
    typing_ticket: Option<String>,
    status: Option<i64>,
    base_info: Option<BaseInfo>,
}

#[derive(Debug, Serialize)]
struct BaseInfoBody {
    base_info: Option<BaseInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::session_guard::STALE_TOKEN_ERRCODE;

    fn client() -> WeixinApiClient {
        WeixinApiClient::new(WeixinApiOptions::new("https://ilinkai.weixin.qq.com")).unwrap()
    }

    #[test]
    fn client_version_encoding() {
        assert_eq!(build_client_version("1.0.11"), 0x0001_000B);
        assert_eq!(build_client_version("2.4.10"), 0x0002_040A);
        assert_eq!(build_client_version("0.0.0"), 0);
        assert_eq!(build_client_version("garbage"), 0);
        assert_eq!(build_client_version("1.2"), 0x0001_0200);
        assert_eq!(build_client_version(""), 0);
    }

    #[test]
    fn sanitizes_defaults() {
        assert_eq!(sanitize_bot_agent(None), DEFAULT_BOT_AGENT);
        assert_eq!(sanitize_bot_agent(Some("")), DEFAULT_BOT_AGENT);
        assert_eq!(sanitize_bot_agent(Some("   ")), DEFAULT_BOT_AGENT);
    }

    #[test]
    fn passes_valid_products() {
        assert_eq!(sanitize_bot_agent(Some("MyBot/1.2.0")), "MyBot/1.2.0");
        assert_eq!(
            sanitize_bot_agent(Some("MyBot/1.2.0 (region=cn;env=prod)")),
            "MyBot/1.2.0 (region=cn;env=prod)"
        );
        assert_eq!(
            sanitize_bot_agent(Some("MyBot/1.2.0 LangChain/0.3.5")),
            "MyBot/1.2.0 LangChain/0.3.5"
        );
        assert_eq!(
            sanitize_bot_agent(Some("MyBot/1.2.0-rc.1+build.5")),
            "MyBot/1.2.0-rc.1+build.5"
        );
    }

    #[test]
    fn drops_invalid_tokens() {
        assert_eq!(sanitize_bot_agent(Some("nonsense")), DEFAULT_BOT_AGENT);
        assert_eq!(sanitize_bot_agent(Some("no-slash")), DEFAULT_BOT_AGENT);
        // A lone comment is dropped.
        assert_eq!(sanitize_bot_agent(Some("(orphan)")), DEFAULT_BOT_AGENT);
        // Invalid token in the middle is dropped, valid ones kept.
        assert_eq!(sanitize_bot_agent(Some("junk MyBot/1.0")), "MyBot/1.0");
    }

    #[test]
    fn keeps_multi_word_comment() {
        let out = sanitize_bot_agent(Some("MyBot/1.0 (a comment here)"));
        assert_eq!(out, "MyBot/1.0 (a comment here)");
    }

    #[test]
    fn truncates_to_max_len() {
        let mut tokens = Vec::new();
        for i in 0..20 {
            tokens.push(format!("Bot{i}/1.0.0"));
        }
        let raw = tokens.join(" ");
        let out = sanitize_bot_agent(Some(&raw));
        assert!(out.len() <= BOT_AGENT_MAX_LEN);
    }

    #[test]
    fn base_info_uses_sanitized_agent() {
        let c = client();
        let info = c.base_info();
        assert_eq!(info.channel_version.as_deref(), Some(crate::VERSION));
        assert_eq!(info.bot_agent.as_deref(), Some(DEFAULT_BOT_AGENT));
    }

    #[test]
    fn with_bot_agent_sanitizes() {
        let c = client().with_bot_agent("garbage");
        assert_eq!(c.base_info().bot_agent.as_deref(), Some(DEFAULT_BOT_AGENT));
        let c2 = client().with_bot_agent("Bot/1.0");
        assert_eq!(c2.base_info().bot_agent.as_deref(), Some("Bot/1.0"));
    }

    #[test]
    fn parse_api_json_quotes_uint64_ids() {
        let v: serde_json::Value =
            parse_api_json(r#"{"message_id": 18446744073709551615,"msgs":[],"msg_id":123}"#)
                .unwrap();
        assert_eq!(v["message_id"], "18446744073709551615");
        assert_eq!(v["msg_id"], "123");
    }

    #[test]
    fn parse_api_json_ignores_strings_mentioning_keys() {
        let v: serde_json::Value =
            parse_api_json(r#"{"note":"message_id is 42","svr_id":"abc"}"#).unwrap();
        assert_eq!(v["note"], "message_id is 42");
        assert_eq!(v["svr_id"], "abc");
    }

    #[test]
    fn parse_api_json_leaves_non_id_fields_numeric() {
        let v: serde_json::Value =
            parse_api_json(r#"{"ret":0,"message_id":7,"errcode":-14}"#).unwrap();
        assert!(v["ret"].is_number());
        assert_eq!(v["message_id"], "7");
        assert!(v["errcode"].is_number());
    }

    #[test]
    fn quote_lossless_ids_handles_negative() {
        assert_eq!(
            quote_lossless_ids(r#"{"message_id": -5}"#),
            r#"{"message_id": "-5"}"#
        );
    }

    #[test]
    fn quote_lossless_ids_ignores_non_numbers() {
        assert_eq!(
            quote_lossless_ids(r#"{"message_id":"7"}"#),
            r#"{"message_id":"7"}"#
        );
        assert_eq!(
            quote_lossless_ids(r#"{"message_id":null}"#),
            r#"{"message_id":null}"#
        );
        assert_eq!(
            quote_lossless_ids(r#"{"message_id":}"#),
            r#"{"message_id":}"#
        );
    }

    #[test]
    fn quote_lossless_ids_handles_whitespace_variants() {
        assert_eq!(
            quote_lossless_ids(r#"{"msg_id" :  99}"#),
            r#"{"msg_id" :  "99"}"#
        );
    }

    #[test]
    fn get_updates_resp_helpers() {
        let r = GetUpdatesResp {
            ret: None,
            errcode: Some(-14),
            ..Default::default()
        };
        assert!(r.is_api_error());
        assert!(r.is_stale_token(STALE_TOKEN_ERRCODE));

        let ok = GetUpdatesResp {
            ret: Some(0),
            errcode: None,
            ..Default::default()
        };
        assert!(!ok.is_api_error());
        assert!(!ok.is_stale_token(STALE_TOKEN_ERRCODE));
    }

    #[test]
    fn send_message_resp_error_semantics() {
        assert!(SendMessageResp {
            ret: Some(3),
            ..Default::default()
        }
        .is_error());
        // Absent ret does NOT trigger the failure check.
        assert!(!SendMessageResp::default().is_error());
        assert!(!SendMessageResp {
            ret: Some(0),
            ..Default::default()
        }
        .is_error());
    }

    #[test]
    fn resolve_url_joins_relative_paths() {
        let c = client();
        assert_eq!(
            c.resolve_url("ilink/bot/getupdates").unwrap(),
            "https://ilinkai.weixin.qq.com/ilink/bot/getupdates"
        );
    }

    #[test]
    fn resolve_url_handles_base_with_path() {
        let c = WeixinApiClient::new(WeixinApiOptions::new("https://x.example/ilink/")).unwrap();
        assert_eq!(
            c.resolve_url("bot/getupdates").unwrap(),
            "https://x.example/ilink/bot/getupdates"
        );
    }

    #[test]
    fn empty_base_url_errors() {
        let c = WeixinApiClient::new(WeixinApiOptions::new("  ")).unwrap();
        assert!(c.resolve_url("ilink/bot/getupdates").is_err());
    }
}
