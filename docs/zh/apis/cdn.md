# 媒体传输（CDN）API

CDN 模块 [`../../src/cdn/`](../../../src/cdn/) 负责媒体（图片、视频、文件、语音）的加密上传与
下载，密码学核心是 **逐块 AES-128-ECB**（真正的 ECB，而非零 IV 的 CBC）。

> **为什么是 ECB 逐块实现？** `aes` crate 没有独立的 ECB 模式，而 CBC + 零 IV
> **不等价**于 ECB：CBC 会把每个明文块与前一个密文块异或，导致重复明文块产生不同
> 密文。微信后端期望真正的 ECB，因此 [`encrypt_aes_ecb`](cdn.md#aes_ecb-子模块) 独立
> 处理每个 16 字节块。单元测试里有一条断言，要求与 Node 的 `aes-128-ecb` 逐字节一致。

模块构成：

| 子模块 | 文件 | 职责 |
| --- | --- | --- |
| `aes_ecb` | [`../../src/cdn/aes_ecb.rs`](../../../src/cdn/aes_ecb.rs) | AES-128-ECB 加解密 |
| `cdn_url` | [`../../src/cdn/cdn_url.rs`](../../../src/cdn/cdn_url.rs) | CDN URL 构造 |
| `download` | [`../../src/cdn/download.rs`](../../../src/cdn/download.rs) | 下载与解密 |
| `upload` | [`../../src/cdn/upload.rs`](../../../src/cdn/upload.rs) | 预签名与上传 |

## `aes_ecb` 子模块

```rust
pub const BLOCK_SIZE: usize = 16;

/// PKCS#7 填充后的密文大小（>= 明文长度的 16 字节倍数）。
pub fn aes_ecb_padded_size(len: usize) -> usize;

/// 逐块 AES-128-ECB 加密；key 必须为 16 字节。
pub fn encrypt_aes_ecb(plaintext: &[u8], key: &[u8; 16]) -> Result<Vec<u8>>;

/// 逐块 AES-128-ECB 解密；密文长度必须是 16 的倍数（PKCS#7 去填充）。
pub fn decrypt_aes_ecb(ciphertext: &[u8], key: &[u8; 16]) -> Result<Vec<u8>>;
```

示例（示意）：

```rust,no_run
use ilink_sdk::cdn::aes_ecb::{encrypt_aes_ecb, decrypt_aes_ecb, aes_ecb_padded_size};

# fn example() -> ilink_sdk::Result<()> {
let key = [0u8; 16];
let plain = b"hello weixin";
let padded = aes_ecb_padded_size(plain.len()); // 16 的倍数
let cipher = encrypt_aes_ecb(plain, &key)?;
assert_eq!(cipher.len(), padded);
let back = decrypt_aes_ecb(&cipher, &key)?;
assert_eq!(&back[..plain.len()], plain);
# Ok(())
# }
```

## `cdn_url` 子模块

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

- 服务器提供的 `full_url` **始终优先**；否则在 `ENABLE_CDN_URL_FALLBACK` 开启时，由
  `encrypted_query_param` / `upload_param` + `filekey` 构造。
- `build_cdn_download_url` 会规范化 base URL 结尾的 `/`，避免出现 `//download`。
- `resolve_*` 在无法解析时会返回 `ErrorKind::Unknown`（下载场景）或携带 `label` 的错误。

## `download` 子模块

```rust
pub const MEDIA_MAX_BYTES: usize = 100 * 1024 * 1024;   // 100 MiB 上限

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
    pub referenced: bool,   // 媒体来自内联引用消息
}
impl InboundMedia { pub fn is_empty(&self) -> bool; }

pub struct MediaSink { /* dir + 可选 subdir */ }
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

### `parse_aes_key` 的双编码

线上出现两种 `aes_key` 编码：

- `base64(16 字节原始)` —— 图片（`media.aes_key`）
- `base64(32 字符 hex 串)` —— 文件 / 语音 / 视频（hex 串 base64 后仍解码为 32 个
  ASCII hex 字符，需再次按 hex 解析）

`parse_aes_key` 两种都会解析，其它长度/非法 base64 返回错误。

### 媒体选择优先级

`download_item_media` 的优先级：**image > video > file > voice**，对应 TS 的
`processOneMessage` 选择。带转录文本的语音项会被**跳过**（转录文本已是正文）。语音
解码后按 `encode_type` 转码：pcm(1) 包成 WAV(24kHz)、mp3(7)/ogg-speex(8) 原样、
silk(6)/其它保持 `.silk` 原始字节，交由宿主用 `ffmpeg` / `silk-wasm` 转码。

### 示例：下载入站媒体

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

## `upload` 子模块

```rust
pub const UPLOAD_MAX_RETRIES: usize = 3;

pub struct UploadedFile {
    pub filekey: String,
    pub download_param: String,    // CDN 返回的下载参数（x-encrypted-param）
    pub aeskey_hex: String,        // hex 编码的 AES-128 key
    pub file_size: u64,            // 明文大小
    pub file_size_ciphertext: u64, // 密文大小（AES-128-ECB + PKCS#7）
}
impl UploadedFile {
    pub fn aes_key_base64(&self) -> String;   // 线上 CdnMedia.aes_key 期望的编码
}

pub async fn upload_media(
    to_user_id: &str, file_path: &Path, media_type: UploadMediaType,
    api: &WeixinApiClient, cdn_base_url: &str
) -> Result<UploadedFile>;

pub async fn upload_image(to_user_id: &str, file_path: &Path, api: &WeixinApiClient, cdn_base_url: &str) -> Result<UploadedFile>;
pub async fn upload_video(to_user_id: &str, file_path: &Path, api: &WeixinApiClient, cdn_base_url: &str) -> Result<UploadedFile>;
pub async fn upload_file_attachment(to_user_id: &str, file_path: &Path, file_name: &str, api: &WeixinApiClient, cdn_base_url: &str) -> Result<UploadedFile>;

/// POST 密文到 CDN，返回 x-encrypted-param 头（成功需 HTTP 200 且头非空）。
pub async fn upload_buffer_to_cdn(ciphertext: &[u8], url: &str, label: &str) -> Result<String>;

/// 下载远程媒体 URL 到临时本地文件（扩展名由 Content-Type 或 URL 推断）。
pub async fn download_remote_to_temp(url: &str, dest_dir: &Path) -> Result<PathBuf>;

pub fn backoff_delay(attempt: usize) -> Duration;   // 250ms，指数退避，封顶 4s
```

### 上传流程（以 `upload_media` 为例）

1. 读取文件 → 计算 `md5` → 计算 padded 大小 → 生成 `filekey` 与随机 16 字节 `aeskey`。
2. 调用 `api.get_upload_url`（`getUploadUrl` 预签名）拿到 `upload_full_url` /
   `upload_param`。
3. `resolve_upload_url` 解析最终上传 URL。
4. `encrypt_aes_ecb` 加密明文。
5. `upload_buffer_to_cdn` POST 密文，读取 `x-encrypted-param` 作为 `download_param`。

### 重试与退避

- HTTP **4xx**：立即中止（返回 `ErrorKind::Unknown`，带 `x-error-message` 或响应体）。
- 其它失败：重试至 `UPLOAD_MAX_RETRIES`（3）次；相邻重试之间 `sleep(backoff_delay(n))`。
- `backoff_delay`：`250ms * 2^(n-1)`，封顶 4s（`n>=8` 时保持 4000ms）。

### 示例：上传图片并拿到可发送的媒体引用

```rust,no_run
use ilink_sdk::prelude::*;
use ilink_sdk::cdn::upload::upload_image;
use std::path::Path;

# async fn example(api: &WeixinApiClient, cdn: &str) -> ilink_sdk::Result<()> {
let uploaded = upload_image("someone@im.wechat", Path::new("./pic.png"), api, cdn).await?;
println!("download_param = {}", uploaded.download_param);
// 用 uploaded.download_param 与 uploaded.aes_key_base64() 构造 CdnMedia
let media = ilink_sdk::api::types::CdnMedia {
    encrypt_query_param: Some(uploaded.download_param),
    aes_key: Some(uploaded.aes_key_base64()),
    encrypt_type: Some(1),
    full_url: None,
};
# Ok(())
# }
```

## 设计要点与注意事项

1. **`MEDIA_MAX_BYTES`（100 MiB）是下载上限。** 超过会在 `fetch_cdn_bytes` 之前
   由调用方控制；`download_item_media` 本身不强制，但宿主应尊重该常量避免内存暴涨。
2. **`aes_key` 编码因媒体类型而异。** 图片用 base64(16 字节)；文件/语音/视频用
   base64(32 hex)。发送侧用 `UploadedFile::aes_key_base64()` 即可得到线上期望编码，
   不要手写 hex。
3. **语音转码交给宿主。** SDK 仅对 pcm 包成 WAV；silk 等原样返回，需要 `ffmpeg` 或
   `silk-wasm` 进一步处理。
4. **4xx 不重试。** 仅网络/服务端临时错误才重试；4xx（如鉴权/参数错误）会立即返回，
   便于快速定位。
5. **`download_item_media` 选择单一媒体。** 一个 `MessageItem` 只会下载优先级最高的
   那一种（image>video>file>voice）；带转录的语音不下载。
6. **`image_item.aeskey`（hex）优先于 `media.aes_key`。** 入站图片解密时优先用
   `image_item.aeskey`；其非法时回退到 `media.aes_key`。
