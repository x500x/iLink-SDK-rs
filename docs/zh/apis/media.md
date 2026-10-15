# MIME 检测 API

MIME 模块 [`../../src/media/mime.rs`](../../src/media/mime.rs) 提供媒体类型探测，是出站
上传路由与入站下载落盘扩展名推断的基础。对应 TypeScript 的 `src/media/mime.ts`。

## 常量

```rust
/// 未知扩展名时的 MIME。
pub const OCTET_STREAM: &str = "application/octet-stream";
/// 未知类型时的扩展名。
pub const BIN_EXTENSION: &str = ".bin";
```

## `MediaKind`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind { Image, Video, File }

impl MediaKind {
    /// 按 MIME 路由：video/* -> Video，image/* -> Image，其余 -> File。
    pub fn from_path(path: &str) -> Self;
}
```

出站管线（`MessageSender::send_file`）依赖 `from_path` 决定走图片、视频还是文件
上传分支。

## 函数一览

```rust
/// 从文件名扩展名取 MIME；未知返回 OCTET_STREAM。
pub fn mime_from_filename(filename: &str) -> String;

/// 从小写（或大写）扩展名取 MIME。
pub fn mime_from_extension(ext: &str) -> String;

/// 从 MIME 取扩展名（含前导点）；未知返回 BIN_EXTENSION。
pub fn extension_from_mime(mime: &str) -> String;

/// 从 Content-Type 头或 URL 取扩展名。
/// content_type 可为 None / 空 / 带参数（"; charset=..."）。
pub fn extension_from_content_type_or_url(content_type: Option<&str>, url: &str) -> String;

/// 路径字符串的扩展名（含前导点，小写）；无扩展名返回空串。
pub fn extension_of(path: &str) -> String;

pub fn is_video(path: &str) -> bool;   // mime 以 "video/" 开头
pub fn is_image(path: &str) -> bool;   // mime 以 "image/" 开头
pub fn is_audio(path: &str) -> bool;   // mime 以 "audio/" 开头
```

## 内置映射表

**扩展名 → MIME**（含）：`.pdf`、`.doc`、`.docx`、`.xls`、`.xlsx`、`.ppt`、
`.pptx`、`.txt`、`.csv`、`.zip`、`.tar`、`.gz`、`.mp3`、`.ogg`、`.wav`、`.silk`、
`.mp4`、`.mov`、`.webm`、`.mkv`、`.avi`、`.png`、`.jpg`、`.jpeg`、`.gif`、
`.webp`、`.bmp`。

**MIME → 扩展名**（含）：`image/jpeg`→`.jpg`、`image/png`→`.png`、
`image/gif`→`.gif`、`image/webp`→`.webp`、`image/bmp`→`.bmp`、`video/mp4`→`.mp4`、
`video/quicktime`→`.mov`、`video/webm`→`.webm`、`video/x-matroska`→`.mkv`、
`video/x-msvideo`→`.avi`、`audio/mpeg`→`.mp3`、`audio/ogg`→`.ogg`、
`audio/wav`→`.wav`、`audio/silk`→`.silk`、`application/pdf`→`.pdf`、
`application/zip`→`.zip`、`application/x-tar`→`.tar`、`application/gzip`→`.gz`、
`text/plain`→`.txt`、`text/csv`→`.csv`。

## 行为细节

- `extension_of` 只取**最后一段文件名**的扩展名，目录名中的点不会被误判为扩展名
  分隔符（`/a/b.c/d` 无扩展名）；`rfind('.')` 在位置 0（隐藏文件如 `.gitignore`）
  视为无扩展名。
- `mime_from_extension` 对扩展名做小写处理；未知扩展名返回 `OCTET_STREAM`。
- `extension_from_content_type_or_url`：先尝试 `content_type`，若解析出的扩展名是
  `BIN_EXTENSION`，再回退到 URL 路径的扩展名；URL 解析失败则回退到 `extension_of(url)`。
- `is_video` / `is_image` / `is_audio` 都是对 `mime_from_filename` 结果的前缀判断。

## 用法示例

```rust,no_run
use ilink_sdk::media::{MediaKind, mime_from_filename, extension_from_mime, is_image};

# fn example() {
assert_eq!(mime_from_filename("a.PNG"), "image/png");
assert_eq!(mime_from_filename("a.unknown"), "application/octet-stream");
assert_eq!(extension_from_mime("Image/JPEG; charset=binary"), ".jpg");
assert_eq!(extension_from_mime("application/x-weird"), ".bin");

assert_eq!(MediaKind::from_path("a.mp4"), MediaKind::Video);
assert_eq!(MediaKind::from_path("a.gif"), MediaKind::Image);
assert_eq!(MediaKind::from_path("a.pdf"), MediaKind::File);

assert!(is_image("photo.png"));
assert!(!is_image("movie.mp4"));
# }
```

## 设计要点与注意事项

1. **出站路由只看 MIME 前缀。** `MediaKind::from_path` 只区分 `video/`、`image/`、
   其它一律 `File`。如果你需要更细的媒体类型区分（如音频作为独立上传分支），需要
   自行扩展。
2. **未知类型回退到 `OCTET_STREAM` / `.bin`。** 下载落盘时未知扩展名会得到 `.bin`，
   调用方可能需按实际内容二次判定。
3. **`content_type` 带参数也能解析。** `"; charset=..."` 等参数会被剥离后匹配，
   因此 `extension_from_content_type_or_url` 可直接吃 HTTP `Content-Type`。
4. **映射表覆盖有限。** 仅包含常见办公/媒体类型；遇到专有类型会回退到
   `OCTET_STREAM` / `.bin`，属预期行为而非错误。
