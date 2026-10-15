//! Wire types for the Weixin (iLink) HTTP JSON protocol.
//!
//! Mirrors `src/api/types.ts`. JSON byte fields are base64 strings; uint64
//! message identifiers are decoded losslessly into `String` because `serde`
//!'s default `u64` handling would silently truncate / overflow values that
//! exceed `Number.MAX_SAFE_INTEGER` in the JavaScript original.

use serde::{Deserialize, Serialize};

/// Common request metadata attached to every CGI request.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BaseInfo {
    pub channel_version: Option<String>,
    /// Self-declared bot identity; observability only, never auth or routing.
    pub bot_agent: Option<String>,
}

/// proto: `UploadMediaType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadMediaType {
    Image = 1,
    Video = 2,
    File = 3,
    Voice = 4,
}

impl UploadMediaType {
    pub fn value(self) -> i64 {
        self as i64
    }
    pub fn from_value(v: i64) -> Option<Self> {
        match v {
            1 => Some(UploadMediaType::Image),
            2 => Some(UploadMediaType::Video),
            3 => Some(UploadMediaType::File),
            4 => Some(UploadMediaType::Voice),
            _ => None,
        }
    }
}

/// `getuploadurl` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetUploadUrlReq {
    pub filekey: Option<String>,
    /// proto field 2: media_type, see [`UploadMediaType`].
    pub media_type: Option<i64>,
    pub to_user_id: Option<String>,
    /// Plaintext size in bytes.
    pub rawsize: Option<i64>,
    /// Plaintext MD5, hex encoded.
    pub rawfilemd5: Option<String>,
    /// Ciphertext size after AES-128-ECB encryption.
    pub filesize: Option<i64>,
    /// Thumbnail plaintext size (required for IMAGE/VIDEO).
    pub thumb_rawsize: Option<i64>,
    /// Thumbnail plaintext MD5.
    pub thumb_rawfilemd5: Option<String>,
    /// Thumbnail ciphertext size.
    pub thumb_filesize: Option<i64>,
    /// Set when no thumbnail upload URL is needed.
    pub no_need_thumb: Option<bool>,
    /// AES key, hex encoded (16 bytes).
    pub aeskey: Option<String>,
}

/// `getuploadurl` response.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetUploadUrlResp {
    /// Encrypted upload parameter for the original file.
    pub upload_param: Option<String>,
    /// Encrypted upload parameter for the thumbnail, empty when absent.
    pub thumb_upload_param: Option<String>,
    /// Complete upload URL supplied directly by the server.
    pub upload_full_url: Option<String>,
}

/// proto: `MessageType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    None = 0,
    User = 1,
    Bot = 2,
}

impl MessageType {
    pub fn value(self) -> i64 {
        self as i64
    }
}

/// proto: `MessageItemType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageItemType {
    None = 0,
    Text = 1,
    Image = 2,
    Voice = 3,
    File = 4,
    Video = 5,
    ToolCallStart = 11,
    ToolCallResult = 12,
}

impl MessageItemType {
    pub fn value(self) -> i64 {
        self as i64
    }
    pub fn from_value(v: i64) -> Option<Self> {
        match v {
            1 => Some(MessageItemType::Text),
            2 => Some(MessageItemType::Image),
            3 => Some(MessageItemType::Voice),
            4 => Some(MessageItemType::File),
            5 => Some(MessageItemType::Video),
            11 => Some(MessageItemType::ToolCallStart),
            12 => Some(MessageItemType::ToolCallResult),
            _ => None,
        }
    }
}

/// proto: `MessageState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageState {
    New = 0,
    Generating = 1,
    Finish = 2,
}

impl MessageState {
    pub fn value(self) -> i64 {
        self as i64
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextItem {
    pub text: Option<String>,
}

/// CDN media reference. `aes_key` is base64-encoded in JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CdnMedia {
    pub encrypt_query_param: Option<String>,
    /// Base64-encoded AES key (either 16 raw bytes or 32 hex chars).
    pub aes_key: Option<String>,
    /// 0 = only the file id is encrypted, 1 = bundled with thumbnail info.
    pub encrypt_type: Option<i64>,
    /// Complete download URL supplied directly by the server.
    pub full_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageItem {
    /// Original image CDN reference.
    pub media: Option<CdnMedia>,
    /// Thumbnail CDN reference.
    pub thumb_media: Option<CdnMedia>,
    /// Raw AES-128 key as a hex string; preferred over `media.aes_key`
    /// for inbound decryption.
    pub aeskey: Option<String>,
    pub url: Option<String>,
    /// Ciphertext byte count set by the image sender.
    pub mid_size: Option<i64>,
    pub thumb_size: Option<i64>,
    pub thumb_height: Option<i64>,
    pub thumb_width: Option<i64>,
    pub hd_size: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceItem {
    pub media: Option<CdnMedia>,
    /// Codec: 1=pcm 2=adpcm 3=feature 4=speex 5=amr 6=silk 7=mp3 8=ogg-speex.
    pub encode_type: Option<i64>,
    pub bits_per_sample: Option<i64>,
    /// Sample rate in Hz.
    pub sample_rate: Option<i64>,
    /// Duration in milliseconds.
    pub playtime: Option<i64>,
    /// Speech-to-text transcript, when present.
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileItem {
    pub media: Option<CdnMedia>,
    pub file_name: Option<String>,
    pub md5: Option<String>,
    /// Plaintext byte count encoded as a decimal string by the file sender.
    pub len: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VideoItem {
    pub media: Option<CdnMedia>,
    /// Ciphertext byte count set by the video sender.
    pub video_size: Option<i64>,
    pub play_length: Option<i64>,
    pub video_md5: Option<String>,
    pub thumb_media: Option<CdnMedia>,
    pub thumb_size: Option<i64>,
    pub thumb_height: Option<i64>,
    pub thumb_width: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RefMessage {
    pub message_item: Option<Box<MessageItem>>,
    /// Quoted message summary.
    pub title: Option<String>,
    /// Server message ID used when newer clients omit the quoted content.
    pub svr_id: Option<String>,
    /// Metadata describing a selected substring of the quoted message.
    pub partial_text: Option<PartialText>,
}

/// A selected substring of a quoted message.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartialText {
    pub start: String,
    pub end: String,
    pub startindex: i64,
    pub endindex: i64,
    pub quotemd5: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCallStartItem {
    pub tool_name: Option<String>,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCallResultItem {
    pub tool_name: Option<String>,
    pub tool_call_id: Option<String>,
    pub status: Option<String>,
}

/// A single message content item.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageItem {
    pub r#type: Option<i64>,
    pub create_time_ms: Option<i64>,
    pub update_time_ms: Option<i64>,
    pub is_completed: Option<bool>,
    /// uint64 on the wire, carried losslessly as a string.
    pub msg_id: Option<String>,
    pub ref_msg: Option<RefMessage>,
    pub text_item: Option<TextItem>,
    pub image_item: Option<ImageItem>,
    pub voice_item: Option<VoiceItem>,
    pub file_item: Option<FileItem>,
    pub video_item: Option<VideoItem>,
    pub tool_call_start_item: Option<ToolCallStartItem>,
    pub tool_call_result_item: Option<ToolCallResultItem>,
}

/// Unified message (proto: `WeixinMessage`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WeixinMessage {
    pub seq: Option<i64>,
    /// uint64 on the wire, decoded losslessly as a string.
    pub message_id: Option<String>,
    pub from_user_id: Option<String>,
    pub to_user_id: Option<String>,
    pub client_id: Option<String>,
    pub create_time_ms: Option<i64>,
    pub update_time_ms: Option<i64>,
    pub delete_time_ms: Option<i64>,
    pub session_id: Option<String>,
    /// Group ID when applicable; describes the type surface only.
    pub group_id: Option<String>,
    /// 1 = user, 2 = bot.
    pub message_type: Option<i64>,
    /// 0 = new, 1 = generating, 2 = finished.
    pub message_state: Option<i64>,
    pub item_list: Option<Vec<MessageItem>>,
    pub context_token: Option<String>,
    pub run_id: Option<String>,
}

/// `getupdates` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetUpdatesReq {
    /// Deprecated compat field; the current builder does not send it.
    pub sync_buf: Option<String>,
    /// Full context buffer cached locally; `""` when none.
    pub get_updates_buf: Option<String>,
}

/// `getupdates` response.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetUpdatesResp {
    /// 0 means success.
    pub ret: Option<i64>,
    /// Application error code (e.g. -14 = session timeout).
    pub errcode: Option<i64>,
    pub errmsg: Option<String>,
    pub msgs: Option<Vec<WeixinMessage>>,
    /// Deprecated compat field; the monitor does not use it as a fallback.
    pub sync_buf: Option<String>,
    /// Cursor to send on the next request.
    pub get_updates_buf: Option<String>,
    /// Server-suggested long-poll timeout in milliseconds.
    pub longpolling_timeout_ms: Option<i64>,
}

impl GetUpdatesResp {
    /// `true` when either `ret` or `errcode` indicates failure.
    pub fn is_api_error(&self) -> bool {
        matches!(self.ret, Some(r) if r != 0) || matches!(self.errcode, Some(c) if c != 0)
    }

    /// `true` when the server reported a stale / expired bot token.
    pub fn is_stale_token(&self, stale_errcode: i64) -> bool {
        self.errcode == Some(stale_errcode) || self.ret == Some(stale_errcode)
    }
}

/// `sendmessage` request: wraps a single [`WeixinMessage`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendMessageReq {
    pub msg: Option<WeixinMessage>,
}

/// `sendmessage` response.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendMessageResp {
    /// uint64 on the wire, decoded losslessly as a string.
    pub message_id: Option<String>,
    pub ret: Option<i64>,
    pub errmsg: Option<String>,
}

impl SendMessageResp {
    /// `true` when the server explicitly reported a business failure.
    ///
    /// Absent `ret` does **not** trigger the failure check, mirroring the
    /// TypeScript `if (resp.ret && resp.ret !== 0)` guard.
    pub fn is_error(&self) -> bool {
        matches!(self.ret, Some(r) if r != 0)
    }
}

/// `sendtyping` status: 1 = typing, 2 = cancel typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypingStatus {
    Typing = 1,
    Cancel = 2,
}

impl TypingStatus {
    pub fn value(self) -> i64 {
        self as i64
    }
}

/// `sendtyping` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendTypingReq {
    pub ilink_user_id: Option<String>,
    pub typing_ticket: Option<String>,
    /// 1 = typing (default), 2 = cancel typing.
    pub status: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendTypingResp {
    pub ret: Option<i64>,
    pub errmsg: Option<String>,
}

/// `getconfig` response: bot config including the typing ticket.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetConfigResp {
    pub ret: Option<i64>,
    pub errmsg: Option<String>,
    /// Base64-encoded typing ticket for `sendtyping`.
    pub typing_ticket: Option<String>,
}

impl GetConfigResp {
    /// The cache layer accepts configuration only when `ret === 0`.
    pub fn is_ok(&self) -> bool {
        self.ret == Some(0)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotifyStopReq {
    pub base_info: Option<BaseInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotifyStopResp {
    pub ret: Option<i64>,
    pub errmsg: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotifyStartReq {
    pub base_info: Option<BaseInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotifyStartResp {
    pub ret: Option<i64>,
    pub errmsg: Option<String>,
}

/// Default `bot_agent` used when the host app declares none.
pub const DEFAULT_BOT_AGENT: &str = "OpenClaw";

/// Maximum byte length of the sanitized `bot_agent` string.
pub const BOT_AGENT_MAX_LEN: usize = 256;
