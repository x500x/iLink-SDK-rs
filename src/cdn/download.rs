//! CDN media download + decryption, mirroring `src/cdn/pic-decrypt.ts` and
//! `src/media/media-download.ts`.

use std::path::Path;

use crate::api::types::{CdnMedia, MessageItemType};
use crate::cdn::aes_ecb::decrypt_aes_ecb;
use crate::cdn::cdn_url::resolve_download_url;
use crate::error::{Error, ErrorKind, Result};
use crate::media::mime::{mime_from_filename, MediaKind};

/// Maximum size we will pull from the CDN, 100 MiB like the TypeScript plugin.
pub const MEDIA_MAX_BYTES: usize = 100 * 1024 * 1024;

/// Download raw bytes from the CDN without decrypting.
pub async fn fetch_cdn_bytes(url: &str, label: &str) -> Result<Vec<u8>> {
    tracing::debug!(%label, url = %crate::util::redact_url(url), "cdn fetch");

    let resp = reqwest::Client::builder()
        .use_preconfigured_tls(crate::tls::default_tls_config()?)
        .build()?
        .get(url)
        .send()
        .await
        .map_err(Error::from_reqwest_wrap)?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::new(
            ErrorKind::Unknown,
            format!("{label}: CDN download {status} body={body}"),
        ));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(Error::from_reqwest_wrap)?
        .to_vec();
    tracing::debug!(%label, bytes = bytes.len(), "cdn fetch done");
    Ok(bytes)
}

/// Parse a `CdnMedia.aes_key` into a raw 16-byte key.
///
/// Two encodings occur in the wild:
/// * `base64(16 raw bytes)` — images (`media.aes_key`),
/// * `base64(32-char hex string)` — file / voice / video.
///
/// In the second case base64-decoding yields 32 ASCII hex characters that
/// must then be parsed as hex to recover the real key.
pub fn parse_aes_key(aes_key_base64: &str, label: &str) -> Result<[u8; 16]> {
    let decoded = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        aes_key_base64.as_bytes(),
    )
    .map_err(|e| {
        Error::new(
            ErrorKind::Unknown,
            format!("{label}: aes_key base64 decode: {e}"),
        )
    })?;

    if decoded.len() == 16 {
        let mut key = [0u8; 16];
        key.copy_from_slice(&decoded);
        return Ok(key);
    }
    if decoded.len() == 32 {
        if let Ok(text) = std::str::from_utf8(&decoded) {
            if text.chars().all(|c| c.is_ascii_hexdigit()) {
                let bytes = hex::decode(text).map_err(|e| {
                    Error::new(ErrorKind::Unknown, format!("{label}: aes_key hex: {e}"))
                })?;
                let mut key = [0u8; 16];
                key.copy_from_slice(&bytes);
                return Ok(key);
            }
        }
    }

    Err(Error::new(
        ErrorKind::Unknown,
        format!(
            "{label}: aes_key must decode to 16 raw bytes or a 32-char hex string, got {} bytes",
            decoded.len()
        ),
    ))
}

/// Resolve the download URL for a CDN reference, preferring `full_url`.
pub fn media_download_url(media: &CdnMedia, cdn_base_url: &str, label: &str) -> Result<String> {
    resolve_download_url(
        media.full_url.as_deref(),
        media.encrypt_query_param.as_deref().unwrap_or(""),
        cdn_base_url,
        label,
    )
}

/// Download and AES-128-ECB decrypt a CDN media reference.
pub async fn download_and_decrypt(
    media: &CdnMedia,
    aes_key_base64: &str,
    cdn_base_url: &str,
    label: &str,
) -> Result<Vec<u8>> {
    let key = parse_aes_key(aes_key_base64, label)?;
    let url = media_download_url(media, cdn_base_url, label)?;
    let encrypted = fetch_cdn_bytes(&url, label).await?;
    tracing::debug!(%label, bytes = encrypted.len(), "decrypting");
    decrypt_aes_ecb(&encrypted, &key)
}

/// Download a plain (unencrypted) CDN reference.
pub async fn download_plain(media: &CdnMedia, cdn_base_url: &str, label: &str) -> Result<Vec<u8>> {
    let url = media_download_url(media, cdn_base_url, label)?;
    fetch_cdn_bytes(&url, label).await
}

/// Everything the inbound pipeline needs about a downloaded media item.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InboundMedia {
    /// Local path of the decrypted image.
    pub image_path: Option<String>,
    /// Local path of the voice file (`.wav` or `.silk`).
    pub voice_path: Option<String>,
    /// MIME type of the voice file.
    pub voice_mime: Option<String>,
    /// Local path of the file attachment.
    pub file_path: Option<String>,
    /// MIME type of the file attachment.
    pub file_mime: Option<String>,
    /// Local path of the video.
    pub video_path: Option<String>,
    /// The media came from an inline quoted message.
    pub referenced: bool,
}

impl InboundMedia {
    /// `true` when nothing was downloaded.
    pub fn is_empty(&self) -> bool {
        self.image_path.is_none()
            && self.voice_path.is_none()
            && self.file_path.is_none()
            && self.video_path.is_none()
    }
}

/// Where to write downloaded media.
#[derive(Debug, Clone)]
pub struct MediaSink {
    dir: std::path::PathBuf,
    subdir: Option<String>,
}

impl MediaSink {
    /// Create a sink rooted at `dir`.
    pub fn new(dir: impl Into<std::path::PathBuf>) -> Self {
        MediaSink {
            dir: dir.into(),
            subdir: None,
        }
    }

    /// Nest downloads under `subdir`.
    pub fn with_subdir(mut self, subdir: impl Into<String>) -> Self {
        self.subdir = Some(subdir.into());
        self
    }

    fn target_dir(&self) -> std::path::PathBuf {
        match &self.subdir {
            Some(sub) => self.dir.join(sub),
            None => self.dir.clone(),
        }
    }

    /// Write `bytes` and return the resulting path.
    pub async fn save(&self, bytes: &[u8], extension: &str) -> Result<std::path::PathBuf> {
        let dir = self.target_dir();
        tokio::fs::create_dir_all(&dir).await?;
        let name = crate::util::temp_file_name("weixin-inbound", extension);
        let path = dir.join(name);
        tokio::fs::write(&path, bytes).await?;
        Ok(path)
    }
}

/// Download the media of one [`crate::api::types::MessageItem`].
///
/// Priority when several media types are present: image > video > file >
/// voice, mirroring the TypeScript `processOneMessage` selection. Voice items
/// carrying a transcript (`voice_item.text`) are skipped, because the
/// transcript is already the message body.
pub async fn download_item_media(
    item: &crate::api::types::MessageItem,
    cdn_base_url: &str,
    sink: &MediaSink,
    label: &str,
) -> InboundMedia {
    let item_type = item.r#type.and_then(MessageItemType::from_value);
    let mut out = InboundMedia::default();

    match item_type {
        Some(MessageItemType::Image) => {
            let Some(img) = item.image_item.as_ref() else {
                return out;
            };
            let Some(media) = img.media.as_ref() else {
                return out;
            };
            if !has_reference(media) {
                return out;
            }
            // Prefer image_item.aeskey (hex) over media.aes_key.
            let key_b64 = match &img.aeskey {
                Some(hexkey) => match base64_encode_hex_text(hexkey) {
                    Some(v) => Some(v),
                    None => {
                        tracing::warn!(%label, "invalid image_item.aeskey, falling back to media.aes_key");
                        media.aes_key.clone()
                    }
                },
                None => media.aes_key.clone(),
            };

            let bytes = match &key_b64 {
                Some(k) => {
                    download_and_decrypt(media, k, cdn_base_url, &format!("{label} image")).await
                }
                None => download_plain(media, cdn_base_url, &format!("{label} image-plain")).await,
            };
            match bytes {
                Ok(buf) => match sink.save(&buf, ".jpg").await {
                    Ok(p) => out.image_path = Some(p.to_string_lossy().into_owned()),
                    Err(e) => tracing::error!(%label, error = %e, "failed to save inbound image"),
                },
                Err(e) => tracing::error!(%label, error = %e, "image download/decrypt failed"),
            }
        }
        Some(MessageItemType::Video) => {
            let Some(v) = item.video_item.as_ref() else {
                return out;
            };
            let Some(media) = v.media.as_ref() else {
                return out;
            };
            if !has_reference(media) || media.aes_key.is_none() {
                return out;
            }
            let name = format!("{label} video");
            match download_and_decrypt(
                media,
                media.aes_key.as_deref().unwrap(),
                cdn_base_url,
                &name,
            )
            .await
            {
                Ok(buf) => match sink.save(&buf, ".mp4").await {
                    Ok(p) => out.video_path = Some(p.to_string_lossy().into_owned()),
                    Err(e) => tracing::error!(%label, error = %e, "failed to save inbound video"),
                },
                Err(e) => tracing::error!(%label, error = %e, "video download failed"),
            }
        }
        Some(MessageItemType::File) => {
            let Some(f) = item.file_item.as_ref() else {
                return out;
            };
            let Some(media) = f.media.as_ref() else {
                return out;
            };
            if !has_reference(media) || media.aes_key.is_none() {
                return out;
            }
            let file_name = f
                .file_name
                .clone()
                .unwrap_or_else(|| "file.bin".to_string());
            let mime = mime_from_filename(&file_name);
            let ext = crate::media::mime::extension_from_mime(&mime);
            let name = format!("{label} file");
            match download_and_decrypt(
                media,
                media.aes_key.as_deref().unwrap(),
                cdn_base_url,
                &name,
            )
            .await
            {
                Ok(buf) => match sink.save(&buf, &ext).await {
                    Ok(p) => {
                        out.file_path = Some(p.to_string_lossy().into_owned());
                        out.file_mime = Some(mime);
                    }
                    Err(e) => tracing::error!(%label, error = %e, "failed to save inbound file"),
                },
                Err(e) => tracing::error!(%label, error = %e, "file download failed"),
            }
        }
        Some(MessageItemType::Voice) => {
            let Some(v) = item.voice_item.as_ref() else {
                return out;
            };
            let Some(media) = v.media.as_ref() else {
                return out;
            };
            // A transcript is already usable as the message body.
            if v.text.as_deref().is_some_and(|t| !t.is_empty()) {
                return out;
            }
            if !has_reference(media) || media.aes_key.is_none() {
                return out;
            }
            let name = format!("{label} voice");
            match download_and_decrypt(
                media,
                media.aes_key.as_deref().unwrap(),
                cdn_base_url,
                &name,
            )
            .await
            {
                Ok(buf) => {
                    let (bytes, ext, mime) = transcode_voice(buf, v.encode_type);
                    match sink.save(&bytes, &ext).await {
                        Ok(p) => {
                            out.voice_path = Some(p.to_string_lossy().into_owned());
                            out.voice_mime = Some(mime);
                        }
                        Err(e) => {
                            tracing::error!(%label, error = %e, "failed to save inbound voice")
                        }
                    }
                }
                Err(e) => tracing::error!(%label, error = %e, "voice download failed"),
            }
        }
        _ => {}
    }

    out
}

fn has_reference(media: &CdnMedia) -> bool {
    media
        .encrypt_query_param
        .as_deref()
        .is_some_and(|p| !p.is_empty())
        || media.full_url.as_deref().is_some_and(|u| !u.is_empty())
}

/// Base64-encode a hex string's ASCII representation, the encoding the wire
/// protocol uses for `image_item.aeskey`.
fn base64_encode_hex_text(hex_key: &str) -> Option<String> {
    let trimmed = hex_key.trim();
    if trimmed.len() != 32 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        trimmed.as_bytes(),
    ))
}

/// Convert a downloaded voice buffer into something a browser can play.
///
/// Returns `(bytes, extension, mime)`. WeChat voice arrives as SILK, which
/// this SDK does not decode; the raw file is handed to the caller so the host
/// application can transcode it with `ffmpeg` or `silk-wasm`.
fn transcode_voice(buf: Vec<u8>, encode_type: Option<i64>) -> (Vec<u8>, String, String) {
    match encode_type {
        // 1=pcm, 7=mp3, 8=ogg-speex are already containerised.
        Some(1) => (
            wrap_pcm_in_wav(&buf, 24_000),
            ".wav".into(),
            "audio/wav".into(),
        ),
        Some(7) => (buf, ".mp3".into(), "audio/mpeg".into()),
        Some(8) => (buf, ".ogg".into(), "audio/ogg".into()),
        // 6=silk and anything unknown stays raw; the host must transcode.
        _ => (buf, ".silk".into(), "audio/silk".into()),
    }
}

/// Wrap raw PCM s16le mono bytes in a minimal 44-byte WAV header.
fn wrap_pcm_in_wav(pcm: &[u8], sample_rate: u32) -> Vec<u8> {
    let total = 44 + pcm.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((total - 8) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

/// Convenience wrapper for the common "download one item and save it" case.
pub async fn download_item_to_path(
    item: &crate::api::types::MessageItem,
    cdn_base_url: &str,
    dest_dir: &Path,
    label: &str,
) -> Result<Option<(std::path::PathBuf, String)>> {
    let sink = MediaSink::new(dest_dir);
    let media = download_item_media(item, cdn_base_url, &sink, label).await;
    let (path, mime) = if let Some(p) = media.image_path {
        (p, "image/*".to_string())
    } else if let Some(p) = media.video_path {
        (p, "video/mp4".to_string())
    } else if let Some(p) = media.file_path {
        (
            p,
            media
                .file_mime
                .unwrap_or_else(|| "application/octet-stream".into()),
        )
    } else if let Some(p) = media.voice_path {
        (p, media.voice_mime.unwrap_or_else(|| "audio/wav".into()))
    } else {
        return Ok(None);
    };
    Ok(Some((std::path::PathBuf::from(path), mime)))
}

/// Classify a local path for outbound routing (kept next to its inverse so
/// the mapping is obvious).
pub fn outbound_kind(path: &Path) -> MediaKind {
    MediaKind::from_path(path.to_string_lossy().as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_16_byte_key() {
        let raw = [0x0Au8; 16];
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, raw);
        let key = parse_aes_key(&b64, "t").unwrap();
        assert_eq!(key, raw);
    }

    #[test]
    fn parses_hex_text_key() {
        let hexkey = "00112233445566778899aabbccddeeff";
        let b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            hexkey.as_bytes(),
        );
        let key = parse_aes_key(&b64, "t").unwrap();
        assert_eq!(hex::encode(key), hexkey);
    }

    #[test]
    fn rejects_bad_keys() {
        assert!(parse_aes_key("!!!notbase64!!!", "t").is_err());
        // Decodes to 10 bytes.
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"0123456789");
        assert!(parse_aes_key(&b64, "t").is_err());
    }

    #[test]
    fn hex_key_helper_rejects_non_hex() {
        assert!(base64_encode_hex_text("zz").is_none());
        assert!(base64_encode_hex_text(&"a".repeat(32)).is_some());
        assert!(base64_encode_hex_text(&"a".repeat(31)).is_none());
    }

    #[test]
    fn wav_header_is_44_bytes_with_correct_size() {
        let pcm = vec![0u8; 100];
        let wav = wrap_pcm_in_wav(&pcm, 24_000);
        assert_eq!(wav.len(), 144);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        // data chunk length at offset 40
        assert_eq!(&wav[40..44], &100u32.to_le_bytes());
    }

    #[test]
    fn voice_transcode_routing() {
        let (b, e, m) = transcode_voice(vec![0u8; 8], Some(1));
        assert_eq!((e.as_str(), m.as_str()), (".wav", "audio/wav"));
        assert_eq!(b.len(), 52);
        let (b2, e2, _) = transcode_voice(vec![0u8; 8], Some(7));
        assert_eq!(e2, ".mp3");
        assert_eq!(b2.len(), 8);
        let (_, e3, _) = transcode_voice(vec![0u8; 8], Some(6));
        assert_eq!(e3, ".silk");
        let (_, e4, _) = transcode_voice(vec![0u8; 8], None);
        assert_eq!(e4, ".silk");
    }

    #[tokio::test]
    async fn media_sink_writes_files() {
        let dir = tempfile::tempdir().unwrap();
        let sink = MediaSink::new(dir.path()).with_subdir("inbound");
        let p = sink.save(b"hello", ".txt").await.unwrap();
        assert!(p.exists());
        assert_eq!(tokio::fs::read(&p).await.unwrap(), b"hello");
        assert!(p.to_string_lossy().contains("inbound"));
    }

    #[tokio::test]
    async fn unsupported_item_yields_empty_media() {
        let item = crate::api::types::MessageItem {
            r#type: Some(crate::api::types::MessageItemType::Text.value()),
            ..Default::default()
        };
        let dir = tempfile::tempdir().unwrap();
        let out = download_item_media(
            &item,
            "https://cdn.invalid",
            &MediaSink::new(dir.path()),
            "t",
        )
        .await;
        assert!(out.is_empty());
    }
}
