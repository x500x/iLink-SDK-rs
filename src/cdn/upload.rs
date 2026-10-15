//! CDN media upload pipeline, mirroring `src/cdn/upload.ts`.
//!
//! Flow: read file → MD5 → generate AES key + filekey → padded size →
//! `getUploadUrl` → AES-128-ECB encrypt → POST ciphertext → read the
//! `x-encrypted-param` response header → hand the download param and key to
//! the message builder.

use std::path::Path;

use crate::api::WeixinApiClient;
use md5::Digest;

use crate::api::types::{GetUploadUrlReq, UploadMediaType};
use crate::cdn::aes_ecb::{aes_ecb_padded_size, encrypt_aes_ecb};
use crate::cdn::cdn_url::resolve_upload_url;
use crate::error::{Error, ErrorKind, Result};
use crate::util::{file_key, redact_url};

/// Maximum upload attempts; HTTP 4xx aborts immediately, everything else is
/// retried up to this many attempts.
pub const UPLOAD_MAX_RETRIES: usize = 3;

/// Result of a successful upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadedFile {
    /// Client-generated file key.
    pub filekey: String,
    /// Download parameter returned by the CDN.
    pub download_param: String,
    /// AES-128 key, hex encoded.
    pub aeskey_hex: String,
    /// Plaintext size in bytes.
    pub file_size: u64,
    /// Ciphertext size in bytes (AES-128-ECB + PKCS#7).
    pub file_size_ciphertext: u64,
}

impl UploadedFile {
    /// The AES key as base64, the encoding the wire protocol expects in
    /// `CdnMedia.aes_key`.
    pub fn aes_key_base64(&self) -> String {
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            self.aeskey_hex.as_bytes(),
        )
    }
}

/// The shared upload pipeline used by image / video / file senders.
pub async fn upload_media(
    to_user_id: &str,
    file_path: &Path,
    media_type: UploadMediaType,
    api: &WeixinApiClient,
    cdn_base_url: &str,
) -> Result<UploadedFile> {
    upload_media_with_label(
        to_user_id,
        file_path,
        media_type,
        api,
        cdn_base_url,
        "uploadMedia",
    )
    .await
}

/// Same as [`upload_media`] with an explicit log label.
pub async fn upload_media_with_label(
    to_user_id: &str,
    file_path: &Path,
    media_type: UploadMediaType,
    api: &WeixinApiClient,
    cdn_base_url: &str,
    label: &str,
) -> Result<UploadedFile> {
    let plaintext = tokio::fs::read(file_path).await?;
    let rawsize = plaintext.len() as u64;
    let mut hasher = md5::Md5::default();
    hasher.update(&plaintext);
    let rawfilemd5 = format!("{:x}", hasher.finalize());
    let filesize = aes_ecb_padded_size(plaintext.len()) as u64;
    let filekey = file_key();
    let mut aeskey = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut aeskey);

    tracing::debug!(
        %label,
        file = %file_path.display(),
        rawsize,
        filesize,
        md5 = %rawfilemd5,
        %filekey,
        "{label}: requesting upload URL"
    );

    let resp = api
        .get_upload_url(&GetUploadUrlReq {
            filekey: Some(filekey.clone()),
            media_type: Some(media_type.value()),
            to_user_id: Some(to_user_id.to_string()),
            rawsize: Some(rawsize as i64),
            rawfilemd5: Some(rawfilemd5.clone()),
            filesize: Some(filesize as i64),
            thumb_rawsize: None,
            thumb_rawfilemd5: None,
            thumb_filesize: None,
            no_need_thumb: Some(true),
            aeskey: Some(hex::encode(aeskey)),
        })
        .await?;

    let url = resolve_upload_url(
        resp.upload_full_url.as_deref(),
        resp.upload_param.as_deref(),
        &filekey,
        cdn_base_url,
        label,
    )?;

    let ciphertext = encrypt_aes_ecb(&plaintext, &aeskey)?;
    let download_param = upload_buffer_to_cdn(&ciphertext, &url, label).await?;

    Ok(UploadedFile {
        filekey,
        download_param,
        aeskey_hex: hex::encode(aeskey),
        file_size: rawsize,
        file_size_ciphertext: filesize,
    })
}

/// Upload a local image file.
pub async fn upload_image(
    to_user_id: &str,
    file_path: &Path,
    api: &WeixinApiClient,
    cdn_base_url: &str,
) -> Result<UploadedFile> {
    upload_media_with_label(
        to_user_id,
        file_path,
        UploadMediaType::Image,
        api,
        cdn_base_url,
        "uploadImage",
    )
    .await
}

/// Upload a local video file.
pub async fn upload_video(
    to_user_id: &str,
    file_path: &Path,
    api: &WeixinApiClient,
    cdn_base_url: &str,
) -> Result<UploadedFile> {
    upload_media_with_label(
        to_user_id,
        file_path,
        UploadMediaType::Video,
        api,
        cdn_base_url,
        "uploadVideo",
    )
    .await
}

/// Upload a local file attachment (non-image, non-video).
pub async fn upload_file_attachment(
    to_user_id: &str,
    file_path: &Path,
    file_name: &str,
    api: &WeixinApiClient,
    cdn_base_url: &str,
) -> Result<UploadedFile> {
    let _ = file_name;
    upload_media_with_label(
        to_user_id,
        file_path,
        UploadMediaType::File,
        api,
        cdn_base_url,
        "uploadFileAttachment",
    )
    .await
}

/// POST ciphertext to the CDN and return the `x-encrypted-param` header.
///
/// Success requires HTTP 200 **and** a non-empty `x-encrypted-param` header.
/// HTTP 4xx aborts immediately; other failures are retried up to
/// [`UPLOAD_MAX_RETRIES`] times in total.
pub async fn upload_buffer_to_cdn(ciphertext: &[u8], url: &str, label: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .use_preconfigured_tls(crate::tls::default_tls_config()?)
        .build()?;

    let mut download_param: Option<String> = None;
    let mut last_error: Option<Error> = None;

    for attempt in 1..=UPLOAD_MAX_RETRIES {
        let result = client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(ciphertext.to_vec())
            .send()
            .await;

        match result {
            Ok(resp) => {
                let status = resp.status();
                if status.is_client_error() {
                    let header_msg = resp
                        .headers()
                        .get("x-error-message")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    let err = match header_msg {
                        Some(m) => m,
                        None => resp
                            .text()
                            .await
                            .unwrap_or_else(|_| status.as_str().to_string()),
                    };
                    tracing::error!(%label, attempt, %status, %err, "cdn upload client error");
                    return Err(Error::new(
                        ErrorKind::Unknown,
                        format!("CDN upload client error {status}: {err}"),
                    ));
                }
                if !status.is_success() {
                    let err = resp
                        .headers()
                        .get("x-error-message")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("status {status}"));
                    tracing::error!(%label, attempt, %status, %err, "cdn upload server error");
                    last_error = Some(Error::new(
                        ErrorKind::Unknown,
                        format!("CDN upload server error: {err}"),
                    ));
                } else {
                    match resp
                        .headers()
                        .get("x-encrypted-param")
                        .and_then(|v| v.to_str().ok())
                        .filter(|s| !s.is_empty())
                    {
                        Some(param) => {
                            tracing::debug!(%label, attempt, "cdn upload success");
                            download_param = Some(param.to_string());
                            break;
                        }
                        None => {
                            tracing::error!(
                                %label,
                                attempt,
                                "cdn upload missing x-encrypted-param header"
                            );
                            last_error = Some(Error::new(
                                ErrorKind::Unknown,
                                "CDN upload response missing x-encrypted-param header",
                            ));
                        }
                    }
                }
            }
            Err(err) => {
                let err = Error::from_reqwest_wrap(err);
                tracing::error!(%label, attempt, url = %redact_url(url), error = %err, "cdn upload attempt failed");
                last_error = Some(err);
            }
        }

        if attempt < UPLOAD_MAX_RETRIES {
            tokio::time::sleep(crate::cdn::upload::backoff_delay(attempt)).await;
        }
    }

    download_param.ok_or_else(|| {
        last_error.unwrap_or_else(|| {
            Error::new(
                ErrorKind::Unknown,
                format!("CDN upload failed after {UPLOAD_MAX_RETRIES} attempts"),
            )
        })
    })
}

/// Backoff before upload attempt `n` (exponential, capped).
pub fn backoff_delay(attempt: usize) -> std::time::Duration {
    let base_ms = 250u64.saturating_mul(1u64 << (attempt.saturating_sub(1).min(4) as u32));
    std::time::Duration::from_millis(base_ms)
}

/// Download a remote media URL to a temporary local file.
///
/// Returns the local path; the extension is inferred from `Content-Type` or
/// the URL path.
pub async fn download_remote_to_temp(url: &str, dest_dir: &Path) -> Result<std::path::PathBuf> {
    tracing::debug!(url = %redact_url(url), "downloading remote media");

    let resp = reqwest::Client::builder()
        .use_preconfigured_tls(crate::tls::default_tls_config()?)
        .build()?
        .get(url)
        .send()
        .await
        .map_err(Error::from_reqwest_wrap)?;

    if !resp.status().is_success() {
        return Err(Error::new(
            ErrorKind::Unknown,
            format!(
                "remote media download failed: {} {}",
                resp.status().as_u16(),
                resp.url().as_str()
            ),
        ));
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = resp.bytes().await.map_err(Error::from_reqwest_wrap)?;

    tokio::fs::create_dir_all(dest_dir).await?;
    let ext = crate::media::mime::extension_from_content_type_or_url(content_type.as_deref(), url);
    let name = crate::util::temp_file_name("weixin-remote", &ext);
    let path = dest_dir.join(name);
    tokio::fs::write(&path, &bytes).await?;
    tracing::debug!(path = %path.display(), bytes = bytes.len(), "remote media saved");
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_key_encoding() {
        let u = UploadedFile {
            filekey: "k".into(),
            download_param: "d".into(),
            aeskey_hex: "00112233445566778899aabbccddeeff".into(),
            file_size: 10,
            file_size_ciphertext: 16,
        };
        // base64 of the 32-character ASCII hex string.
        assert_eq!(
            u.aes_key_base64(),
            base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                "00112233445566778899aabbccddeeff".as_bytes()
            )
        );
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff_delay(1), std::time::Duration::from_millis(250));
        assert_eq!(backoff_delay(2), std::time::Duration::from_millis(500));
        assert_eq!(backoff_delay(8), std::time::Duration::from_millis(4000));
        assert_eq!(backoff_delay(99), std::time::Duration::from_millis(4000));
    }

    #[tokio::test]
    async fn upload_missing_file_errors() {
        // No network: the read of a nonexistent path fails first.
        let api = crate::api::WeixinApiClient::new(crate::api::WeixinApiOptions::new(
            "https://invalid.invalid",
        ))
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let result = upload_image(
            "u@im.wechat",
            &dir.path().join("nope.png"),
            &api,
            "https://cdn.invalid/c2c",
        )
        .await;
        assert!(result.is_err());
    }
}
