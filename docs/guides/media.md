# Media (CDN)

> 中文版: [中文文档](../zh/guides/media.md)

Weixin media (images, video, files, voice) is never sent inline. Outbound media
is uploaded to a CDN encrypted with **AES-128-ECB**; inbound media is downloaded
from the CDN and decrypted with the same cipher. All of this lives in the
[`cdn`](../../src/cdn) module.

## AES-128-ECB: why block-by-block?

The backend expects **true ECB**, where each 16-byte plaintext block is
encrypted independently. Rust's `aes` crate has no standalone ECB mode, and CBC
with a zero IV is *not* equivalent — it XORs each block against the previous
ciphertext, so identical plaintext blocks produce different ciphertext.

The SDK therefore implements ECB explicitly per block
([`encrypt_aes_ecb`] / [`decrypt_aes_ecb`]) with PKCS#7 padding. A test asserts
byte-for-byte equality with Node's `aes-128-ecb`.

```rust,no_run
use ilink_sdk::cdn::{encrypt_aes_ecb, decrypt_aes_ecb};

# fn run() -> ilink_sdk::Result<()> {
let key = [0x42u8; 16];
let ct = encrypt_aes_ecb(b"hello world", &key)?;  // 16 bytes
let pt = decrypt_aes_ecb(&ct, &key)?;             // b"hello world"
assert_eq!(pt, b"hello world");
# Ok(())
# }
```

`aes_ecb_padded_size(n)` returns the ciphertext length `(n/16 + 1) * 16`.

## Uploading (outbound)

You normally never call the upload functions directly — `MessageSender::send_file`
does it for you. If you need raw control, the pipeline is:

```text
read file → MD5 → generate AES key + filekey → padded size
  → get_upload_url → encrypt AES-128-ECB → POST ciphertext to CDN
  → read x-encrypted-param header → hand (download_param + aeskey) to the message
```

```rust,no_run
# async fn run(api: ilink_sdk::WeixinApiClient, path: &std::path::Path) -> ilink_sdk::Result<()> {
use ilink_sdk::cdn::{upload_image, upload_video, upload_file_attachment, UploadedFile};
use ilink_sdk::storage::accounts::CDN_BASE_URL;

let to = "someone@im.wechat";
let cdn = CDN_BASE_URL;

// Returns the download param + hex AES key to embed in CdnMedia.
let uploaded: UploadedFile = upload_image(to, path, &api, cdn).await?;
let _ = uploaded.filekey;
let _ = uploaded.download_param;
let _ = uploaded.aeskey_hex;
let _ = uploaded.file_size;            // plaintext bytes
let _ = uploaded.file_size_ciphertext; // AES-128-ECB + PKCS#7 bytes
let _ = uploaded.aes_key_base64();     // base64 of the hex key (wire encoding)
# Ok(())
# }
```

`upload_video` and `upload_file_attachment` follow the same shape. The shared
[`upload_media`] handles thumbnails, MD5, key generation, and retries.

### Retries

[`upload_buffer_to_cdn`] POSTs the ciphertext. Success requires HTTP 200 **and**
a non-empty `x-encrypted-param` header. HTTP 4xx aborts immediately; other
failures retry up to [`UPLOAD_MAX_RETRIES`] (3) times with exponential backoff
([`backoff_delay`], 250ms → 500ms → … capped at 4s).

## Downloading (inbound)

```rust,no_run
# async fn run(media: ilink_sdk::api::types::CdnMedia, aes_key_b64: &str, cdn_base_url: &str) -> ilink_sdk::Result<()> {
use ilink_sdk::cdn::{download_and_decrypt, download_plain, parse_aes_key, MediaSink, download_item_media};

// Encrypted media (image/video/file/voice):
let bytes = download_and_decrypt(&media, aes_key_b64, cdn_base_url, "inbound").await?;

// Unencrypted media (when no aes_key present):
let plain = download_plain(&media, cdn_base_url, "inbound").await?;

// Convenience: download + decrypt one item and save it to disk.
let sink = MediaSink::new("./downloads").with_subdir("inbound");
let inbound = download_item_media(&Default::default(), cdn_base_url, &sink, "inbound").await;
let _ = inbound.image_path;
# Ok(())
# }
```

### AES key encoding on the wire

`CdnMedia.aes_key` is base64, but two layouts occur in the wild:

- **16 raw bytes** — typical for images (`media.aes_key`). [`parse_aes_key`]
  decodes and returns the 16-byte key directly.
- **32-char hex string, itself base64-encoded** — typical for file / voice /
  video. Decoding base64 yields 32 ASCII hex chars that are then hex-parsed.

`parse_aes_key` handles both; anything else is an error.

### Media selection priority

`download_item_media` picks, in order, **image > video > file > voice**,
matching the TypeScript `processOneMessage` behavior. A voice item that already
carries a transcript (`voice_item.text`) is skipped, because the transcript is
the message body.

## Voice transcoding

WeChat voice arrives as SILK, which this SDK does not decode. The raw buffer is
handed back so the host can transcode it (e.g. `ffmpeg`). Containerized codecs
are normalized:

| `encode_type` | Output | MIME |
| --- | --- | --- |
| `1` (PCM) | wrapped in a WAV header | `audio/wav` |
| `7` (MP3) | passthrough | `audio/mpeg` |
| `8` (OGG-Speex) | passthrough | `audio/ogg` |
| `6` (SILK) / other | raw | `audio/silk` |

## CDN URL construction

[`resolve_download_url`] / [`resolve_upload_url`] prefer a server-returned
`full_url`. When only `encrypt_query_param` / `upload_param` are present, the
URL is built against `cdn_base_url` (see [`build_cdn_download_url`],
[`build_cdn_upload_url`]). Fallback construction is controlled by
[`ENABLE_CDN_URL_FALLBACK`] (on by default).

## Size limits

- `MEDIA_MAX_BYTES` (100 MiB) caps what [`fetch_cdn_bytes`] pulls.
- Quote-cache media budgets are documented under
  [Storage & state](storage.md).
