# Media transport (CDN) API

> 中文版: [中文文档](../zh/apis/cdn.md)

The CDN module ([`../../src/cdn/`](../../src/cdn/)) handles encrypted upload and
download of media (images, videos, files, voice). The crypto core is **per-block
AES-128-ECB** — true ECB, not zero-IV CBC.

> **Why a per-block ECB implementation?** The `aes` crate has no standalone ECB
> mode, and CBC with a zero IV is **not** equivalent to ECB: CBC XORs each plaintext
> block with the previous ciphertext block, so repeated plaintext blocks produce
> different ciphertext. The Weixin backend expects true ECB, so
> [`encrypt_aes_ecb`](#aes_ecb-submodule) processes each 16-byte block independently. A
> unit test asserts byte-for-byte compatibility with Node's `aes-128-ecb`.

## Module composition

| Submodule | File | Responsibility |
| --- | --- | --- |
| `aes_ecb` | [`../../src/cdn/aes_ecb.rs`](../../src/cdn/aes_ecb.rs) | AES-128-ECB encrypt/decrypt |
| `cdn_url` | [`../../src/cdn/cdn_url.rs`](../../src/cdn/cdn_url.rs) | CDN URL construction |
| `download` | [`../../src/cdn/download.rs`](../../src/cdn/download.rs) | download + decrypt |
| `upload` | [`../../src/cdn/upload.rs`](../../src/cdn/upload.rs) | pre-sign + upload |

## `aes_ecb` submodule

```rust
pub const BLOCK_SIZE: usize = 16;

/// PKCS#7-padded ciphertext size (>= plaintext length, multiple of 16).
pub fn aes_ecb_padded_size(len: usize) -> usize;

/// Per-block AES-128-ECB encrypt; key must be 16 bytes.
pub fn encrypt_aes_ecb(plaintext: &[u8], key: &[u8; 16]) -> Result<Vec<u8>>;

/// Per-block AES-128-ECB decrypt; ciphertext length must be a multiple of 16 (PKCS#7 unpad).
pub fn decrypt_aes_ecb(ciphertext: &[u8], key: &[u8; 16]) -> Result<Vec<u8>>;
```

Example (illustrative):

```rust,no_run
use ilink_sdk::cdn::aes_ecb::{encrypt_aes_ecb, decrypt_aes_ecb, aes_ecb_padded_size};

# fn example() -> ilink_sdk::Result<()> {
let key = [0u8; 16];
let plain = b"hello weixin";
let padded = aes_ecb_padded_size(plain.len()); // multiple of 16
let cipher = encrypt_aes_ecb(plain, &key)?;
assert_eq!(cipher.len(), padded);
let back = decrypt_aes_ecb(&cipher, &key)?;
assert_eq!(&back[..plain.len()], plain);
# Ok(())
# }
```

## `cdn_url` submodule

```rust
pub const CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";
pub const ENABLE_CDN_URL_FALLBACK: bool = true;

pub fn build_cdn_download_url(encrypted_query_param: &str, cdn_base_url: &str) -> Result<String>;
//   "<cdn_base_url>/download?encrypted_query_param=<urlencoded param>"

pub fn build_cdn_upload_url(cdn_base_url: &str, upload_param: &str, filekey: &str) -> Result<String>;
//   "<cdn_base_url>/upload?encrypted_query_param=<upload_param>&filekey=<filekey>"

pub fn resolve_download_url(
    full_url: Option<&str>, encrypted_query_param: &str, cdn_base_url: &str, label: &str
) -> Result<String>;

pub fn resolve_upload_url(
    upload_full_url: Option<&str>, upload_param: Option<&str>, filekey: &str,
    cdn_base_url: &str, label: &str
) -> Result<String>;
```

- The server-provided `full_url` **always wins**; otherwise, when
  `ENABLE_CDN_URL_FALLBACK` is on, build from `encrypted_query_param` /
  `upload_param` + `filekey`.
- `build_cdn_download_url` normalizes a trailing `/` on the base URL to avoid
  `//download`.
- `resolve_*` returns `ErrorKind::Unknown` (download) or an error carrying `label`
  when it cannot resolve.

## `download` submodule

```rust
pub const MEDIA_MAX_BYTES: usize = 100 * 1024 * 1024;   // 100 MiB cap

pub async fn fetch_cdn_bytes(url: &str, label: &str) -> Result<Vec<u8>>;
pub fn parse_aes_key(aes_key_base64: &str, label: &str) -> Result<[u8; 16]>;
pub fn media_download_url(media: &CdnMedia, cdn_base_url: &str, label: &str) -> Result<String>;
pub async fn download_and_decrypt(
    media: &CdnMedia, aes_key_base64: &str, cdn_base_url: &str, label: &str
) -> Result<Vec<u8>>;
pub async fn download_plain(media: &CdnMedia, cdn_base_url: &str, label: &str) -> Result<Vec<u8>>;

pub struct InboundMedia {
    pub image_path: Option<String>,
    pub voice_path: Option<String>,
    pub voice_mime: Option<String>,
    pub file_path: Option<String>,
    pub file_mime: Option<String>,
    pub video_path: Option<String>,
    pub referenced: bool,   // media came from an inline ref message
}
impl InboundMedia { pub fn is_empty(&self) -> bool; }

pub struct MediaSink { /* dir + optional subdir */ }
impl MediaSink {
    pub fn new(dir: impl Into<PathBuf>) -> Self;
    pub fn with_subdir(mut self, subdir: impl Into<String>) -> Self;
    pub async fn save(&self, bytes: &[u8], extension: &str) -> Result<PathBuf>;
}

pub async fn download_item_media(
    item: &MessageItem, cdn_base_url: &str, sink: &MediaSink, label: &str
) -> InboundMedia;

pub async fn download_item_to_path(
    item: &MessageItem, cdn_base_url: &str, dest_dir: &Path, label: &str
) -> Result<Option<(PathBuf, String)>>;   // (path, mime)

pub fn outbound_kind(path: &Path) -> MediaKind;
```

### `parse_aes_key` dual encoding

Two `aes_key` encodings appear on the wire:

- `base64(16 raw bytes)` — images (`media.aes_key`)
- `base64(32 hex chars)` — files / voice / video (the hex string base64-encoded,
  which decodes to 32 ASCII hex chars needing a second hex parse)

`parse_aes_key` accepts both; other lengths / invalid base64 return an error.

### Media selection priority

`download_item_media` priority: **image > video > file > voice**, matching TS
`processOneMessage`. A voice item that already carries a transcript is **skipped**
(the transcript is the body). Voice is transcoded after decode by `encode_type`:
pcm(1) wrapped as WAV(24kHz), mp3(7)/ogg-speex(8) kept as-is, silk(6)/others kept
as raw `.silk` bytes for the host to transcode via `ffmpeg` / `silk-wasm`.

### Example: download inbound media

```rust,no_run
use ilink_sdk::cdn::download::{download_item_media, MediaSink, InboundMedia};
use ilink_sdk::api::types::MessageItem;

# async fn example(item: &MessageItem, cdn: &str) -> ilink_sdk::Result<()> {
let sink = MediaSink::new(std::env::temp_dir()).with_subdir("inbound");
let media: InboundMedia = download_item_media(item, cdn, &sink, "inbound").await;
if let Some(p) = media.image_path {
    println!("image at {}", p);
} else if let Some(p) = media.voice_path {
    println!("voice ({}): {}", media.voice_mime.unwrap_or_default(), p);
}
# Ok(())
# }
```

## `upload` submodule

```rust
pub const UPLOAD_MAX_RETRIES: usize = 3;

pub struct UploadedFile {
    pub filekey: String,
    pub download_param: String,    // x-encrypted-param returned by CDN
    pub aeskey_hex: String,        // hex-encoded AES-128 key
    pub file_size: u64,            // plaintext size
    pub file_size_ciphertext: u64, // ciphertext size (AES-128-ECB + PKCS#7)
}
impl UploadedFile {
    pub fn aes_key_base64(&self) -> String;   // encoding expected by wire CdnMedia.aes_key
}

pub async fn upload_media(
    to_user_id: &str, file_path: &Path, media_type: UploadMediaType,
    api: &WeixinApiClient, cdn_base_url: &str
) -> Result<UploadedFile>;

pub async fn upload_image(to_user_id: &str, file_path: &Path, api: &WeixinApiClient, cdn_base_url: &str) -> Result<UploadedFile>;
pub async fn upload_video(to_user_id: &str, file_path: &Path, api: &WeixinApiClient, cdn_base_url: &str) -> Result<UploadedFile>;
pub async fn upload_file_attachment(to_user_id: &str, file_path: &Path, file_name: &str, api: &WeixinApiClient, cdn_base_url: &str) -> Result<UploadedFile>;

/// POST ciphertext to CDN, return x-encrypted-param header (must be HTTP 200 + non-empty header).
pub async fn upload_buffer_to_cdn(ciphertext: &[u8], url: &str, label: &str) -> Result<String>;

/// Download a remote media URL to a temp local file (extension inferred from Content-Type or URL).
pub async fn download_remote_to_temp(url: &str, dest_dir: &Path) -> Result<PathBuf>;

pub fn backoff_delay(attempt: usize) -> Duration;   // 250ms, exponential, cap 4s
```

### Upload flow (as `upload_media`)

1. Read file → compute `md5` → compute padded size → generate `filekey` and a random
   16-byte `aeskey`.
2. Call `api.get_upload_url` (the `getUploadUrl` pre-sign) to get `upload_full_url` /
   `upload_param`.
3. `resolve_upload_url` resolves the final upload URL.
4. `encrypt_aes_ecb` encrypts the plaintext.
5. `upload_buffer_to_cdn` POSTs the ciphertext, reading `x-encrypted-param` as the
   `download_param`.

### Retries and backoff

- HTTP **4xx**: abort immediately (returns `ErrorKind::Unknown` with
  `x-error-message` or the response body).
- Other failures: retry up to `UPLOAD_MAX_RETRIES` (3); `sleep(backoff_delay(n))`
  between attempts.
- `backoff_delay`: `250ms * 2^(n-1)`, capped at 4s (`n>=8` holds 4000ms).

### Example: upload an image and get a sendable media reference

```rust,no_run
use ilink_sdk::prelude::*;
use ilink_sdk::cdn::upload::upload_image;
use std::path::Path;

# async fn example(api: &WeixinApiClient, cdn: &str) -> ilink_sdk::Result<()> {
let uploaded = upload_image("someone@im.wechat", Path::new("./pic.png"), api, cdn).await?;
println!("download_param = {}", uploaded.download_param);
// Build a CdnMedia from uploaded.download_param + uploaded.aes_key_base64()
let media = ilink_sdk::api::types::CdnMedia {
    encrypt_query_param: Some(uploaded.download_param),
    aes_key: Some(uploaded.aes_key_base64()),
    encrypt_type: Some(1),
    full_url: None,
};
# Ok(())
# }
```

## Design notes and caveats

1. **`MEDIA_MAX_BYTES` (100 MiB) is the download cap.** The caller should honor it
   before `fetch_cdn_bytes`; `download_item_media` itself does not enforce it but
   hosts should avoid unbounded memory growth.
2. **`aes_key` encoding differs by media type.** Images use base64(16 bytes); files/
   voice/video use base64(32 hex). On the send side, use
   `UploadedFile::aes_key_base64()` to get the wire-expected encoding — do not hand
   -write the hex.
3. **Voice transcoding is the host's job.** The SDK only wraps pcm as WAV; silk etc.
   are returned raw and need `ffmpeg` or `silk-wasm`.
4. **4xx is not retried.** Only network/transient server errors retry; 4xx (auth/
   param errors) returns immediately to surface the problem fast.
5. **`download_item_media` selects a single media.** One `MessageItem` downloads only
   the highest-priority kind (image>video>file>voice); transcribed voice is skipped.
6. **`image_item.aeskey` (hex) takes priority over `media.aes_key`.** For inbound
   image decryption, `image_item.aeskey` is tried first; on invalid, fall back to
   `media.aes_key`.
