//! AES-128-ECB crypto plus the CDN media upload/download pipeline.

pub mod aes_ecb;
pub mod cdn_url;
pub mod download;
pub mod upload;

pub use aes_ecb::{aes_ecb_padded_size, decrypt_aes_ecb, encrypt_aes_ecb, BLOCK_SIZE};
pub use cdn_url::{
    build_cdn_download_url, build_cdn_upload_url, resolve_download_url, resolve_upload_url,
    ENABLE_CDN_URL_FALLBACK,
};
pub use download::{
    download_and_decrypt, download_plain, fetch_cdn_bytes, media_download_url, parse_aes_key,
};
pub use upload::{
    download_remote_to_temp, upload_buffer_to_cdn, upload_file_attachment, upload_image,
    upload_media, upload_video, UploadedFile, UPLOAD_MAX_RETRIES,
};
