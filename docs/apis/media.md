# MIME detection API

The MIME module ([`../../src/media/mime.rs`](../../src/media/mime.rs)) provides media
type sniffing, the basis for outbound upload routing and inbound download
extension inference. It corresponds to TypeScript's `src/media/mime.ts`.

## Constants

```rust
/// MIME used when no extension matches.
pub const OCTET_STREAM: &str = "application/octet-stream";
/// Extension used when no type matches.
pub const BIN_EXTENSION: &str = ".bin";
```

## `MediaKind`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind { Image, Video, File }

impl MediaKind {
    /// Route by MIME: video/* -> Video, image/* -> Image, else File.
    pub fn from_path(path: &str) -> Self;
}
```

The outbound pipeline (`MessageSender::send_file`) relies on `from_path` to
decide the image / video / file upload branch.

## Function overview

```rust
/// MIME from filename extension; unknown -> OCTET_STREAM.
pub fn mime_from_filename(filename: &str) -> String;

/// MIME from a lower- (or upper-) cased extension.
pub fn mime_from_extension(ext: &str) -> String;

/// Extension (with leading dot) from MIME; unknown -> BIN_EXTENSION.
pub fn extension_from_mime(mime: &str) -> String;

/// Extension from a Content-Type header or URL.
/// content_type may be None / empty / with params ("; charset=...").
pub fn extension_from_content_type_or_url(content_type: Option<&str>, url: &str) -> String;

/// Extension (with leading dot, lower-cased) of a path string; no ext -> "".
pub fn extension_of(path: &str) -> String;

pub fn is_video(path: &str) -> bool;   // mime starts with "video/"
pub fn is_image(path: &str) -> bool;   // mime starts with "image/"
pub fn is_audio(path: &str) -> bool;   // mime starts with "audio/"
```

## Built-in tables

**Extension → MIME** (includes): `.pdf`, `.doc`, `.docx`, `.xls`, `.xlsx`,
`.ppt`, `.pptx`, `.txt`, `.csv`, `.zip`, `.tar`, `.gz`, `.mp3`, `.ogg`, `.wav`,
`.silk`, `.mp4`, `.mov`, `.webm`, `.mkv`, `.avi`, `.png`, `.jpg`, `.jpeg`,
`.gif`, `.webp`, `.bmp`.

**MIME → Extension** (includes): `image/jpeg`→`.jpg`, `image/png`→`.png`,
`image/gif`→`.gif`, `image/webp`→`.webp`, `image/bmp`→`.bmp`, `video/mp4`→`.mp4`,
`video/quicktime`→`.mov`, `video/webm`→`.webm`, `video/x-matroska`→`.mkv`,
`video/x-msvideo`→`.avi`, `audio/mpeg`→`.mp3`, `audio/ogg`→`.ogg`,
`audio/wav`→`.wav`, `audio/silk`→`.silk`, `application/pdf`→`.pdf`,
`application/zip`→`.zip`, `application/x-tar`→`.tar`, `application/gzip`→`.gz`,
`text/plain`→`.txt`, `text/csv`→`.csv`.

## Behavior details

- `extension_of` takes the extension of only the **last filename segment**; a dot in
  a directory name is not treated as an extension separator (`/a/b.c/d` has no
  extension); an `rfind('.')` at position 0 (hidden files like `.gitignore`) is
  treated as no extension.
- `mime_from_extension` lower-cases the extension; unknown -> `OCTET_STREAM`.
- `extension_from_content_type_or_url`: try `content_type` first; if the parsed
  extension is `BIN_EXTENSION`, fall back to the URL path extension; a failed URL
  parse falls back to `extension_of(url)`.
- `is_video` / `is_image` / `is_audio` are prefix checks on `mime_from_filename`.

## Usage example

```rust,no_run
use openclaw_weixin::media::{MediaKind, mime_from_filename, extension_from_mime, is_image};

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

## Design notes and caveats

1. **Outbound routing looks only at the MIME prefix.** `MediaKind::from_path` only
   distinguishes `video/`, `image/`, everything else is `File`. If you need a finer
   media-type split (e.g. audio as a separate upload branch), extend it yourself.
2. **Unknown types fall back to `OCTET_STREAM` / `.bin`.** A download that yields an
   unknown extension lands as `.bin`; the caller may need a content-based second pass.
3. **`content_type` with params still parses.** `"; charset=..."` is stripped before
   matching, so `extension_from_content_type_or_url` can eat an HTTP `Content-Type`
   directly.
4. **The table coverage is limited.** Only common office/media types are included;
   a proprietary type falls back to `OCTET_STREAM` / `.bin` — expected, not an error.
