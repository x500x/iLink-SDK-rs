//! Outbound message construction, mirroring `src/messaging/send.ts`.
//!
//! Every outbound message is a single `SendMessageReq` whose `item_list`
//! holds exactly one item. Media items are uploaded first and reference the
//! CDN object through `encrypt_query_param` + `aes_key`.

use std::path::Path;

use crate::api::types::{
    CdnMedia, FileItem, ImageItem, MessageItem, MessageState, MessageType, SendMessageReq,
    VideoItem, WeixinMessage,
};
use crate::api::WeixinApiClient;
use crate::cdn::upload::{upload_file_attachment, upload_image, upload_video};
use crate::error::{Error, ErrorKind, Result};
use crate::media::mime::{self, MediaKind};
use crate::util::{generate_id, redact_token};

/// Parameters shared by every outbound send.
#[derive(Debug, Clone, Default)]
pub struct SendOpts {
    /// Conversation context token echoed from the inbound message.
    pub context_token: Option<String>,
    /// Optional generation / run id.
    pub run_id: Option<String>,
}

/// The result of a successful send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendResult {
    /// Client-generated id (`prefix:ts-hex`).
    pub message_id: String,
    /// Server-assigned message id, when the backend returned one.
    pub server_message_id: Option<String>,
}

/// Sends outbound messages through one API client.
pub struct MessageSender<'a> {
    api: &'a WeixinApiClient,
    account_id: String,
}

impl<'a> MessageSender<'a> {
    /// Create a sender.
    pub fn new(api: &'a WeixinApiClient, account_id: impl Into<String>) -> Self {
        MessageSender {
            api,
            account_id: account_id.into(),
        }
    }

    /// Access the account id this sender is bound to.
    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    fn client_id() -> String {
        generate_id("openclaw-weixin")
    }

    /// Send a plain text message.
    ///
    /// A missing `context_token` is logged as a warning but does not block the
    /// send, matching the TypeScript behaviour.
    pub async fn send_text(&self, to: &str, text: &str, opts: &SendOpts) -> Result<SendResult> {
        if opts.context_token.is_none() {
            tracing::warn!(
                to = to,
                "send_text: contextToken missing, sending without context"
            );
        }
        let client_id = Self::client_id();
        let item_list = if text.is_empty() {
            None
        } else {
            Some(vec![MessageItem {
                r#type: Some(crate::api::types::MessageItemType::Text.value()),
                text_item: Some(crate::api::types::TextItem {
                    text: Some(text.to_string()),
                }),
                ..Default::default()
            }])
        };
        self.send_items(to, vec![], item_list, opts, &client_id)
            .await
    }

    /// Send one pre-built [`MessageItem`].
    pub async fn send_item(
        &self,
        to: &str,
        item: MessageItem,
        opts: &SendOpts,
    ) -> Result<SendResult> {
        let client_id = Self::client_id();
        self.send_items(to, vec![item], None, opts, &client_id)
            .await
    }

    /// Upload a local file and send it as image, video or file according to
    /// its MIME type, preceded by an optional text caption.
    pub async fn send_file(
        &self,
        to: &str,
        path: &Path,
        caption: &str,
        opts: &SendOpts,
        cdn_base_url: &str,
    ) -> Result<SendResult> {
        let mime = mime::mime_from_filename(path.to_string_lossy().as_ref());
        let cdn = cdn_base_url.to_string();

        let item = match MediaKind::from_path(path.to_string_lossy().as_ref()) {
            MediaKind::Video => {
                let uploaded = upload_video(to, path, self.api, &cdn).await?;
                self.media_item(
                    crate::api::types::MessageItemType::Video.value(),
                    Some(CdnMedia {
                        encrypt_query_param: Some(uploaded.download_param.clone()),
                        aes_key: Some(uploaded.aes_key_base64()),
                        encrypt_type: Some(1),
                        full_url: None,
                    }),
                    None,
                    None,
                )?
                .with_video_size(uploaded.file_size_ciphertext as i64)
            }
            MediaKind::Image => {
                let uploaded = upload_image(to, path, self.api, &cdn).await?;
                self.media_item(
                    crate::api::types::MessageItemType::Image.value(),
                    Some(CdnMedia {
                        encrypt_query_param: Some(uploaded.download_param.clone()),
                        aes_key: Some(uploaded.aes_key_base64()),
                        encrypt_type: Some(1),
                        full_url: None,
                    }),
                    None,
                    None,
                )?
                .with_mid_size(uploaded.file_size_ciphertext as i64)
            }
            MediaKind::File => {
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "file.bin".to_string());
                let uploaded = upload_file_attachment(to, path, &file_name, self.api, &cdn).await?;
                self.media_item(
                    crate::api::types::MessageItemType::File.value(),
                    Some(CdnMedia {
                        encrypt_query_param: Some(uploaded.download_param.clone()),
                        aes_key: Some(uploaded.aes_key_base64()),
                        encrypt_type: Some(1),
                        full_url: None,
                    }),
                    Some(file_name.clone()),
                    Some(uploaded.file_size.to_string()),
                )?
            }
        };

        tracing::info!(
            to = to,
            mime = %mime,
            "send_file: uploaded and sending media"
        );

        // Caption first, then the media item; each becomes its own request.
        let mut prelude = Vec::new();
        if !caption.is_empty() {
            prelude.push(MessageItem {
                r#type: Some(crate::api::types::MessageItemType::Text.value()),
                text_item: Some(crate::api::types::TextItem {
                    text: Some(caption.to_string()),
                }),
                ..Default::default()
            });
        }

        let mut last = None;
        for it in prelude.into_iter().chain(std::iter::once(item)) {
            let client_id = Self::client_id();
            last = Some(
                self.send_items(to, vec![it], None, opts, &client_id)
                    .await?,
            );
        }
        Ok(last.expect("at least one item"))
    }

    /// Send raw bytes as a file attachment.
    pub async fn send_bytes(
        &self,
        to: &str,
        data: &[u8],
        file_name: &str,
        caption: &str,
        opts: &SendOpts,
        cdn_base_url: &str,
    ) -> Result<SendResult> {
        let dir = std::env::temp_dir();
        let path = dir.join(sanitize_file_name(file_name));
        std::fs::write(&path, data).map_err(|e| Error::new(ErrorKind::Unknown, e.to_string()))?;
        let result = self.send_file(to, &path, caption, opts, cdn_base_url).await;
        let _ = std::fs::remove_file(&path);
        result
    }

    /// Send raw bytes as an image.
    pub async fn send_image_bytes(
        &self,
        to: &str,
        data: &[u8],
        extension: &str,
        caption: &str,
        opts: &SendOpts,
        cdn_base_url: &str,
    ) -> Result<SendResult> {
        let dir = std::env::temp_dir();
        let ext = if extension.starts_with('.') {
            extension.to_string()
        } else {
            format!(".{extension}")
        };
        let path = dir.join(crate::util::temp_file_name("weixin-out", &ext));
        std::fs::write(&path, data).map_err(|e| Error::new(ErrorKind::Unknown, e.to_string()))?;
        let result = self.send_file(to, &path, caption, opts, cdn_base_url).await;
        let _ = std::fs::remove_file(&path);
        result
    }

    /// Perform the actual `sendMessage` request for one item list.
    async fn send_items(
        &self,
        to: &str,
        extra_items: Vec<MessageItem>,
        item_list: Option<Vec<MessageItem>>,
        opts: &SendOpts,
        client_id: &str,
    ) -> Result<SendResult> {
        let items = match item_list {
            Some(list) => list,
            None => extra_items,
        };

        if opts.context_token.is_none() {
            tracing::warn!(
                to = to,
                account = %self.account_id,
                "sendMessage: contextToken missing, sending without context"
            );
        }

        let req = SendMessageReq {
            msg: Some(WeixinMessage {
                from_user_id: Some(String::new()),
                to_user_id: Some(to.to_string()),
                client_id: Some(client_id.to_string()),
                message_type: Some(MessageType::Bot.value()),
                message_state: Some(MessageState::Finish.value()),
                item_list: if items.is_empty() { None } else { Some(items) },
                context_token: opts.context_token.clone(),
                run_id: opts.run_id.clone(),
                ..Default::default()
            }),
        };

        let resp = self.api.send_message(&req).await?;
        tracing::info!(
            to = to,
            client_id = client_id,
            server_id = ?resp.message_id.as_deref().map(|m| redact_token(Some(m), None)),
            "sendMessage: ok"
        );
        Ok(SendResult {
            message_id: client_id.to_string(),
            server_message_id: resp.message_id,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn media_item(
        &self,
        item_type: i64,
        media: Option<CdnMedia>,
        file_name: Option<String>,
        len: Option<String>,
    ) -> Result<MessageItem> {
        let _ = &self;
        Ok(MessageItem {
            r#type: Some(item_type),
            ..Default::default()
        }
        .with_media(media)
        .with_file_item(file_name, len))
    }
}

/// Small helpers so a [`MessageItem`] can be built fluently.
trait ItemExt {
    fn with_media(self, media: Option<CdnMedia>) -> Self;
    fn with_file_item(self, name: Option<String>, len: Option<String>) -> Self;
    fn with_video_size(self, size: i64) -> Self;
    fn with_mid_size(self, size: i64) -> Self;
}

impl ItemExt for MessageItem {
    fn with_media(self, media: Option<CdnMedia>) -> Self {
        let mut out = self;
        match out.r#type.map(MessageItemTypeTag::from_i64) {
            Some(MessageItemTypeTag::Image) => {
                out.image_item = Some(ImageItem {
                    media,
                    ..Default::default()
                });
            }
            Some(MessageItemTypeTag::Video) => {
                out.video_item = Some(VideoItem {
                    media,
                    ..Default::default()
                });
            }
            _ => {
                out.file_item = Some(FileItem {
                    media,
                    ..Default::default()
                });
            }
        }
        out
    }

    fn with_file_item(self, name: Option<String>, len: Option<String>) -> Self {
        let mut out = self;
        if let Some(item) = out.file_item.as_mut() {
            item.file_name = name;
            item.len = len;
        }
        out
    }

    fn with_video_size(self, size: i64) -> Self {
        let mut out = self;
        if let Some(item) = out.video_item.as_mut() {
            item.video_size = Some(size);
        }
        out
    }

    fn with_mid_size(self, size: i64) -> Self {
        let mut out = self;
        if let Some(item) = out.image_item.as_mut() {
            item.mid_size = Some(size);
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessageItemTypeTag {
    Image,
    Video,
    Other,
}

impl MessageItemTypeTag {
    fn from_i64(v: i64) -> Self {
        match crate::api::types::MessageItemType::from_value(v) {
            Some(crate::api::types::MessageItemType::Image) => MessageItemTypeTag::Image,
            Some(crate::api::types::MessageItemType::Video) => MessageItemTypeTag::Video,
            _ => MessageItemTypeTag::Other,
        }
    }
}

/// Reduce a caller-supplied file name to a safe single path segment.
fn sanitize_file_name(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let cleaned: String = last
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '-' => c,
            _ => '_',
        })
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        format!("file{}", crate::media::mime::BIN_EXTENSION)
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_id_prefix() {
        assert!(MessageSender::client_id().starts_with("openclaw-weixin:"));
    }

    #[test]
    fn send_result_round_trip() {
        let r = SendResult {
            message_id: "openclaw-weixin:1-abc".into(),
            server_message_id: Some("42".into()),
        };
        assert_eq!(r.message_id, "openclaw-weixin:1-abc");
        assert_eq!(r.server_message_id.as_deref(), Some("42"));
    }

    #[test]
    fn opts_default_empty() {
        let o = SendOpts::default();
        assert!(o.context_token.is_none());
        assert!(o.run_id.is_none());
    }
}
