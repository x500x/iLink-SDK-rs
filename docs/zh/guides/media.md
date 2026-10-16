# 媒体（CDN）

> English: [English](../../guides/media.md)

微信媒体（图片、视频、文件、语音）从不内联发送。出站媒体被上传到一个 CDN，并用
**AES-128-ECB** 加密；入站媒体从 CDN 下载并用同一算法解密。这些都位于
[`cdn`](../../../src/cdn) 模块。

## AES-128-ECB：为什么逐块实现？

后端期望的是**真正的 ECB**：每个 16 字节明文块独立加密。Rust 的 `aes` crate 没有
独立的 ECB 模式，而带零 IV 的 CBC **并不等价**——它会把每个明文块与前一个密文块
异或，因此相同的明文块会产生不同的密文。

因此 SDK 显式地逐块实现 ECB（[`encrypt_aes_ecb`] / [`decrypt_aes_ecb`]），并采用
PKCS#7 填充。有测试断言其输出与 Node 的 `aes-128-ecb` 逐字节一致。

```rust,no_run
use ilink_sdk::cdn::{encrypt_aes_ecb, decrypt_aes_ecb};

# fn run() -> ilink_sdk::Result<()> {
let key = [0x42u8; 16];
let ct = encrypt_aes_ecb(b"hello world", &key)?;  // 16 字节
let pt = decrypt_aes_ecb(&ct, &key)?;             // b"hello world"
assert_eq!(pt, b"hello world");
# Ok(())
# }
```

`aes_ecb_padded_size(n)` 返回密文长度 `(n/16 + 1) * 16`。

## 上传（出站）

你通常不需要直接调用上传函数——`MessageSender::send_file` 会替你完成。如果需要
原始控制，流程如下：

```text
读取文件 → MD5 → 生成 AES 密钥 + filekey → 填充后长度
  → get_upload_url → AES-128-ECB 加密 → 将密文 POST 到 CDN
  → 读取 x-encrypted-param 响应头 → 把（download_param + aeskey）交给消息
```

```rust,no_run
# async fn run(api: ilink_sdk::WeixinApiClient, path: &std::path::Path) -> ilink_sdk::Result<()> {
use ilink_sdk::cdn::{upload_image, upload_video, upload_file_attachment, UploadedFile};
use ilink_sdk::storage::accounts::CDN_BASE_URL;

let to = "someone@im.wechat";
let cdn = CDN_BASE_URL;

// 返回要嵌入 CdnMedia 的下载参数 + 十六进制 AES 密钥。
let uploaded: UploadedFile = upload_image(to, path, &api, cdn).await?;
let _ = uploaded.filekey;
let _ = uploaded.download_param;
let _ = uploaded.aeskey_hex;
let _ = uploaded.file_size;            // 明文字节数
let _ = uploaded.file_size_ciphertext; // AES-128-ECB + PKCS#7 后的密文字节数
let _ = uploaded.aes_key_base64();     // 十六进制密钥的 base64（线上编码）
# Ok(())
# }
```

`upload_video` 与 `upload_file_attachment` 形态相同。共享的 [`upload_media`] 负责
缩略图、MD5、密钥生成与重试。

### 重试

[`upload_buffer_to_cdn`] 把密文 POST 出去。成功要求 HTTP 200 **且** 非空
`x-encrypted-param` 响应头。HTTP 4xx 立即中止；其他失败最多重试
[`UPLOAD_MAX_RETRIES`]（3）次，使用指数退避（[`backoff_delay`]，250ms → 500ms → …，
上限 4s）。

## 下载（入站）

```rust,no_run
# async fn run(media: ilink_sdk::api::types::CdnMedia, aes_key_b64: &str, cdn_base_url: &str) -> ilink_sdk::Result<()> {
use ilink_sdk::cdn::{download_and_decrypt, download_plain, parse_aes_key, MediaSink, download_item_media};

// 加密媒体（图片/视频/文件/语音）：
let bytes = download_and_decrypt(&media, aes_key_b64, cdn_base_url, "inbound").await?;

// 未加密媒体（当没有 aes_key 时）：
let plain = download_plain(&media, cdn_base_url, "inbound").await?;

// 便捷：下载 + 解密单个项并保存到磁盘。
let sink = MediaSink::new("./downloads").with_subdir("inbound");
let inbound = download_item_media(&Default::default(), cdn_base_url, &sink, "inbound").await;
let _ = inbound.image_path;
# Ok(())
# }
```

### 线上的 AES 密钥编码

`CdnMedia.aes_key` 是 base64，但线上存在两种布局：

- **16 个原始字节** —— 图片常见（`media.aes_key`）。[`parse_aes_key`] 直接解码并返回
  16 字节密钥。
- **32 字符十六进制字符串，本身再被 base64 编码** —— 文件 / 语音 / 视频常见。base64
  解码后会得到 32 个 ASCII 十六进制字符，再按十六进制解析。

`parse_aes_key` 两者都处理；其他情况均为错误。

### 媒体选择优先级

`download_item_media` 按 **图片 > 视频 > 文件 > 语音** 的顺序选择，与 TypeScript 的
`processOneMessage` 行为一致。已携带转写（`voice_item.text`）的语音项会被跳过，因为
转写本身就是消息正文。

## 语音转码

微信语音以 SILK 格式到达，本 SDK 不解码它。原始缓冲区会被直接交还，由主机进行转码
（如 `ffmpeg`）。已容器化的编码会被规范化：

| `encode_type` | 输出 | MIME |
| --- | --- | --- |
| `1` (PCM) | 包裹 WAV 头 | `audio/wav` |
| `7` (MP3) | 原样透传 | `audio/mpeg` |
| `8` (OGG-Speex) | 原样透传 | `audio/ogg` |
| `6` (SILK) / 其他 | 原样 | `audio/silk` |

## CDN URL 构造

[`resolve_download_url`] / [`resolve_upload_url`] 优先使用服务端返回的 `full_url`。
当只有 `encrypt_query_param` / `upload_param` 时，会基于 `cdn_base_url` 构造 URL
（见 [`build_cdn_download_url`]、[`build_cdn_upload_url`]）。回退构造由
[`ENABLE_CDN_URL_FALLBACK`] 控制（默认开启）。

## 大小限制

- `MEDIA_MAX_BYTES`（100 MiB）限制 [`fetch_cdn_bytes`] 拉取的上限。
- 引用缓存的媒体预算见[存储与状态](../guides/storage.md)。
