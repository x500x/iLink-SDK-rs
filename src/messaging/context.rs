//! Conversation context tokens, mirroring `src/messaging/inbound.ts`.
//!
//! The backend issues a `context_token` per inbound message and it must be
//! echoed verbatim on every outbound send into that conversation. The
//! in-memory map is the primary lookup; a per-account JSON file makes the
//! tokens survive restarts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::api::types::MessageItemType;
use crate::storage::state_dir::accounts_dir;

/// In-memory token store keyed by `account:user`.
#[derive(Debug, Default)]
pub struct ContextTokenStore {
    map: HashMap<String, String>,
    /// Base directory used for persistence.
    dir: PathBuf,
}

/// The inbound context handed to the host application, mirroring the
/// TypeScript `WeixinMsgContext`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MsgContext {
    pub body: String,
    pub from: String,
    pub to: String,
    pub account_id: String,
    pub originating_channel: String,
    pub originating_to: String,
    pub message_sid: String,
    /// Provider message id, kept losslessly for transcript metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_sid_full: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    pub provider: String,
    pub chat_type: String,
    /// Set by the host after routing so dispatch uses the right session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<MediaEntry>,
    /// Extra instructions handed to the agent about quoted attachments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channel_prompt_context: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to_body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to_quote_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to_is_quote: Option<bool>,
}

/// One entry of `MsgContext.media`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaEntry {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}

fn key(account_id: &str, user_id: &str) -> String {
    format!("{account_id}:{user_id}")
}

fn token_file(dir: &Path, account_id: &str) -> PathBuf {
    dir.join(format!("{account_id}.context-tokens.json"))
}

impl ContextTokenStore {
    /// Create a store rooted at the default accounts directory.
    pub fn new() -> Self {
        ContextTokenStore {
            map: HashMap::new(),
            dir: accounts_dir(),
        }
    }

    /// Create a store rooted at an explicit directory.
    pub fn with_dir(dir: impl Into<PathBuf>) -> Self {
        ContextTokenStore {
            map: HashMap::new(),
            dir: dir.into(),
        }
    }

    /// Store a token (memory + disk).
    pub fn set(&mut self, account_id: &str, user_id: &str, token: &str) {
        self.map.insert(key(account_id, user_id), token.to_string());
        self.persist(account_id);
    }

    /// Look up a token.
    pub fn get(&self, account_id: &str, user_id: &str) -> Option<&str> {
        self.map.get(&key(account_id, user_id)).map(String::as_str)
    }

    /// Every account id holding a live token for `user_id`.
    pub fn accounts_for_user<'a>(&self, account_ids: &'a [String], user_id: &str) -> Vec<&'a str> {
        account_ids
            .iter()
            .filter(|id| self.map.contains_key(&key(id, user_id)))
            .map(String::as_str)
            .collect()
    }

    /// Drop every token belonging to `account_id` (memory + disk).
    pub fn clear_account(&mut self, account_id: &str) {
        let prefix = format!("{account_id}:");
        self.map.retain(|k, _| !k.starts_with(&prefix));
        let _ = std::fs::remove_file(token_file(&self.dir, account_id));
    }

    /// Restore persisted tokens for one account. Returns how many were loaded.
    pub fn restore(&mut self, account_id: &str) -> usize {
        let path = token_file(&self.dir, account_id);
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return 0;
        };
        let Ok(tokens) = serde_json::from_str::<HashMap<String, String>>(&raw) else {
            return 0;
        };
        let mut count = 0;
        for (user, token) in tokens {
            if !token.is_empty() {
                self.map.insert(key(account_id, &user), token);
                count += 1;
            }
        }
        count
    }

    fn persist(&self, account_id: &str) {
        let prefix = format!("{account_id}:");
        let mut tokens: HashMap<&str, &str> = HashMap::new();
        for (k, v) in &self.map {
            if let Some(user) = k.strip_prefix(&prefix) {
                tokens.insert(user, v.as_str());
            }
        }
        let _ = std::fs::create_dir_all(&self.dir);
        if let Ok(json) = serde_json::to_string(&tokens) {
            let _ = std::fs::write(token_file(&self.dir, account_id), json);
        }
    }
}

/// `true` when the item is a media type (image, video, file, or voice).
pub fn is_media_item(item: &crate::api::types::MessageItem) -> bool {
    matches!(
        item.r#type.and_then(MessageItemType::from_value),
        Some(MessageItemType::Image)
            | Some(MessageItemType::Video)
            | Some(MessageItemType::File)
            | Some(MessageItemType::Voice)
    )
}

/// Human-readable placeholder used as the message body for a media item.
pub fn media_label(item_type: Option<i64>) -> &'static str {
    match item_type.and_then(MessageItemType::from_value) {
        Some(MessageItemType::Image) => "[图片]",
        Some(MessageItemType::Video) => "[视频]",
        Some(MessageItemType::File) => "[文件]",
        Some(MessageItemType::Voice) => "[语音]",
        _ => "",
    }
}

/// The provider-side message id: top-level `message_id`, else the first
/// `msg_id` in the item list.
pub fn message_id(msg: &crate::api::types::WeixinMessage) -> Option<String> {
    if let Some(id) = msg.message_id.as_deref().filter(|s| !s.trim().is_empty()) {
        return Some(id.trim().to_string());
    }
    msg.item_list.as_ref()?.iter().find_map(|item| {
        item.msg_id
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
    })
}

/// Extract the text body of an inbound message.
///
/// A voice item with a transcript yields that transcript; anything else that
/// is media yields its placeholder label. The first matching item wins.
pub fn body_from_items(items: &[crate::api::types::MessageItem]) -> String {
    for item in items {
        match item.r#type.and_then(MessageItemType::from_value) {
            Some(MessageItemType::Text) => {
                if let Some(text) = item.text_item.as_ref().and_then(|t| t.text.clone()) {
                    return text;
                }
            }
            Some(MessageItemType::Voice) => {
                if let Some(text) = item
                    .voice_item
                    .as_ref()
                    .and_then(|v| v.text.clone())
                    .filter(|t| !t.is_empty())
                {
                    return text;
                }
                // A voice item without a transcript still needs a placeholder
                // so the pipeline sees "[语音]" rather than an empty body.
                return media_label(item.r#type).to_string();
            }
            other => {
                let label = media_label(other.map(MessageItemType::value));
                if !label.is_empty() {
                    return label.to_string();
                }
            }
        }
    }
    String::new()
}

/// Build the inbound context for the host application.
pub fn message_to_context(
    msg: &crate::api::types::WeixinMessage,
    account_id: &str,
    media: &crate::cdn::download::InboundMedia,
) -> MsgContext {
    let from = msg.from_user_id.clone().unwrap_or_default();
    let mut ctx = MsgContext {
        body: body_from_items(msg.item_list.as_deref().unwrap_or(&[])),
        from: from.clone(),
        to: from.clone(),
        account_id: account_id.to_string(),
        originating_channel: "openclaw-weixin".into(),
        originating_to: from.clone(),
        message_sid: crate::util::generate_id("openclaw-weixin"),
        timestamp: msg.create_time_ms,
        provider: "openclaw-weixin".into(),
        chat_type: "direct".into(),
        ..Default::default()
    };

    if let Some(token) = msg.context_token.as_deref().filter(|t| !t.is_empty()) {
        ctx.context_token = Some(token.to_string());
    }
    ctx.message_sid_full = message_id(msg);

    if let Some(p) = &media.image_path {
        ctx.media_path = Some(p.clone());
        ctx.media_type = Some("image/*".into());
    } else if let Some(p) = &media.video_path {
        ctx.media_path = Some(p.clone());
        ctx.media_type = Some("video/mp4".into());
    } else if let Some(p) = &media.file_path {
        ctx.media_path = Some(p.clone());
        ctx.media_type = Some(
            media
                .file_mime
                .clone()
                .unwrap_or_else(|| "application/octet-stream".into()),
        );
    } else if let Some(p) = &media.voice_path {
        ctx.media_path = Some(p.clone());
        ctx.media_type = Some(
            media
                .voice_mime
                .clone()
                .unwrap_or_else(|| "audio/wav".into()),
        );
    }

    if media.referenced && ctx.media_path.is_some() {
        ctx.media_paths.push(ctx.media_path.clone().unwrap());
        ctx.media_types
            .push(ctx.media_type.clone().unwrap_or_default());
        ctx.media_path = None;
        ctx.media_type = None;
    }

    apply_inline_quote(&mut ctx, msg);
    ctx
}

fn find_reference_item(
    msg: &crate::api::types::WeixinMessage,
) -> Option<&crate::api::types::MessageItem> {
    msg.item_list.as_ref()?.iter().find(|i| i.ref_msg.is_some())
}

/// Fill `ReplyTo*` fields from an inline quoted message.
fn apply_inline_quote(ctx: &mut MsgContext, msg: &crate::api::types::WeixinMessage) {
    let Some(item) = find_reference_item(msg) else {
        return;
    };
    let Some(reference) = item.ref_msg.as_ref() else {
        return;
    };

    ctx.reply_to_is_quote = Some(true);
    let reply_to_id = reference
        .svr_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            reference
                .message_item
                .as_ref()
                .and_then(|m| m.msg_id.clone())
        });
    if let Some(id) = reply_to_id.filter(|s| !s.is_empty()) {
        ctx.reply_to_id = Some(id);
    }

    let mut parts = Vec::new();
    if let Some(title) = reference.title.as_deref().filter(|t| !t.trim().is_empty()) {
        parts.push(title.trim().to_string());
    }
    if let Some(inner) = reference.message_item.as_ref() {
        let body = body_from_items(std::slice::from_ref(inner));
        if !body.is_empty() {
            parts.push(body);
        }
    }
    if !parts.is_empty() {
        ctx.reply_to_body = Some(parts.join(" | "));
    }
}

/// Attach the `context_token` of a context back onto a store.
pub fn remember_context_token(store: &mut ContextTokenStore, ctx: &MsgContext) -> bool {
    match ctx.context_token.as_deref().filter(|t| !t.is_empty()) {
        Some(token) => {
            store.set(&ctx.account_id, &ctx.from, token);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{
        MessageItem, MessageItemType, PartialText, RefMessage, TextItem, WeixinMessage,
    };

    fn text_item(text: &str) -> MessageItem {
        MessageItem {
            r#type: Some(MessageItemType::Text.value()),
            text_item: Some(TextItem {
                text: Some(text.into()),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn token_store_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = ContextTokenStore::with_dir(dir.path());
        store.set("acct", "u@im.wechat", "tok");
        assert_eq!(store.get("acct", "u@im.wechat"), Some("tok"));
        assert_eq!(store.get("acct", "other"), None);

        let mut reloaded = ContextTokenStore::with_dir(dir.path());
        assert_eq!(reloaded.restore("acct"), 1);
        assert_eq!(reloaded.get("acct", "u@im.wechat"), Some("tok"));
    }

    #[test]
    fn clear_account_removes_only_that_account() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = ContextTokenStore::with_dir(dir.path());
        store.set("a", "u", "1");
        store.set("b", "u", "2");
        store.clear_account("a");
        assert_eq!(store.get("a", "u"), None);
        assert_eq!(store.get("b", "u"), Some("2"));
    }

    #[test]
    fn accounts_for_user_lists_matches() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = ContextTokenStore::with_dir(dir.path());
        store.set("a", "u", "1");
        store.set("b", "u", "2");
        store.set("c", "v", "3");
        let ids = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(store.accounts_for_user(&ids, "u"), vec!["a", "b"]);
        assert_eq!(store.accounts_for_user(&ids, "v"), vec!["c"]);
        assert!(store.accounts_for_user(&ids, "zz").is_empty());
    }

    #[test]
    fn is_media_item_detects_media_types() {
        assert!(super::is_media_item(&MessageItem {
            r#type: Some(MessageItemType::Image.value()),
            ..Default::default()
        }));
        assert!(!super::is_media_item(&text_item("hi")));
    }

    #[test]
    fn media_labels() {
        assert_eq!(media_label(Some(MessageItemType::Image.value())), "[图片]");
        assert_eq!(media_label(Some(MessageItemType::Video.value())), "[视频]");
        assert_eq!(media_label(Some(MessageItemType::Voice.value())), "[语音]");
        assert_eq!(media_label(Some(MessageItemType::Text.value())), "");
        assert_eq!(media_label(None), "");
    }

    #[test]
    fn message_id_prefers_top_level() {
        let msg = WeixinMessage {
            message_id: Some("99".into()),
            item_list: Some(vec![MessageItem {
                msg_id: Some("1".into()),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert_eq!(message_id(&msg).as_deref(), Some("99"));
    }

    #[test]
    fn message_id_falls_back_to_item() {
        let msg = WeixinMessage {
            item_list: Some(vec![
                text_item("a"),
                MessageItem {
                    msg_id: Some("7".into()),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };
        assert_eq!(message_id(&msg).as_deref(), Some("7"));
    }

    #[test]
    fn body_prefers_text_then_voice_transcript_then_label() {
        assert_eq!(body_from_items(&[text_item("hi")]), "hi");

        let voice = MessageItem {
            r#type: Some(MessageItemType::Voice.value()),
            voice_item: Some(crate::api::types::VoiceItem {
                text: Some("spoken words".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(body_from_items(&[voice]), "spoken words");

        let voice_no_text = MessageItem {
            r#type: Some(MessageItemType::Voice.value()),
            voice_item: Some(crate::api::types::VoiceItem::default()),
            ..Default::default()
        };
        assert_eq!(body_from_items(&[voice_no_text]), "[语音]");
    }

    #[test]
    fn context_carries_core_fields() {
        let msg = WeixinMessage {
            from_user_id: Some("u@im.wechat".into()),
            message_id: Some("5".into()),
            create_time_ms: Some(1234),
            context_token: Some("ct".into()),
            item_list: Some(vec![text_item("hello")]),
            ..Default::default()
        };
        let ctx = message_to_context(&msg, "acct", &Default::default());
        assert_eq!(ctx.body, "hello");
        assert_eq!(ctx.from, "u@im.wechat");
        assert_eq!(ctx.to, "u@im.wechat");
        assert_eq!(ctx.account_id, "acct");
        assert_eq!(ctx.chat_type, "direct");
        assert_eq!(ctx.message_sid_full.as_deref(), Some("5"));
        assert_eq!(ctx.context_token.as_deref(), Some("ct"));
        assert_eq!(ctx.timestamp, Some(1234));
        assert!(ctx.message_sid.starts_with("openclaw-weixin:"));
    }

    #[test]
    fn context_fills_quote_fields() {
        let quote = MessageItem {
            r#type: Some(MessageItemType::Text.value()),
            ref_msg: Some(RefMessage {
                title: Some("Summary".into()),
                svr_id: Some("42".into()),
                message_item: Some(Box::new(text_item("original"))),
                partial_text: None,
            }),
            ..Default::default()
        };
        let msg = WeixinMessage {
            item_list: Some(vec![quote, text_item("reply")]),
            ..Default::default()
        };
        let ctx = message_to_context(&msg, "a", &Default::default());
        assert_eq!(ctx.reply_to_is_quote, Some(true));
        assert_eq!(ctx.reply_to_id.as_deref(), Some("42"));
        assert_eq!(ctx.reply_to_body.as_deref(), Some("Summary | original"));
    }

    #[test]
    fn quote_without_content_still_sets_id() {
        let quote = MessageItem {
            ref_msg: Some(RefMessage {
                svr_id: Some("9".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let msg = WeixinMessage {
            item_list: Some(vec![quote]),
            ..Default::default()
        };
        let ctx = message_to_context(&msg, "a", &Default::default());
        assert_eq!(ctx.reply_to_id.as_deref(), Some("9"));
        assert_eq!(ctx.reply_to_is_quote, Some(true));
    }

    #[test]
    fn remember_context_token_returns_whether_set() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = ContextTokenStore::with_dir(dir.path());
        let ctx = MsgContext {
            account_id: "a".into(),
            from: "u".into(),
            context_token: Some("tok".into()),
            ..Default::default()
        };
        assert!(remember_context_token(&mut store, &ctx));
        assert_eq!(store.get("a", "u"), Some("tok"));

        let no_token = MsgContext {
            account_id: "a".into(),
            from: "u".into(),
            ..Default::default()
        };
        assert!(!remember_context_token(&mut store, &no_token));
    }

    #[test]
    fn referenced_media_moves_path_into_paths() {
        let msg = WeixinMessage {
            from_user_id: Some("u".into()),
            item_list: Some(vec![text_item("hi")]),
            ..Default::default()
        };
        let media = crate::cdn::download::InboundMedia {
            image_path: Some("/tmp/x.jpg".into()),
            referenced: true,
            ..Default::default()
        };
        let ctx = message_to_context(&msg, "a", &media);
        assert!(ctx.media_path.is_none());
        assert_eq!(ctx.media_paths, vec!["/tmp/x.jpg".to_string()]);
        assert_eq!(ctx.media_types, vec!["image/*".to_string()]);
    }

    #[test]
    fn partial_text_deserializes() {
        let p: PartialText = serde_json::from_str(
            r#"{"start":"a","end":"b","startindex":0,"endindex":1,"quotemd5":"m"}"#,
        )
        .unwrap();
        assert_eq!(p.start, "a");
        assert_eq!(p.quotemd5, "m");
    }
}
