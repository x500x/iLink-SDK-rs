//! MIME helpers, mirroring `src/media/mime.ts`.

use std::path::Path;

/// Extension (lowercase, with leading dot) to MIME type.
const EXTENSION_TO_MIME: &[(&str, &str)] = &[
    (".pdf", "application/pdf"),
    (".doc", "application/msword"),
    (
        ".docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
    (".xls", "application/vnd.ms-excel"),
    (
        ".xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ),
    (".ppt", "application/vnd.ms-powerpoint"),
    (
        ".pptx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    ),
    (".txt", "text/plain"),
    (".csv", "text/csv"),
    (".zip", "application/zip"),
    (".tar", "application/x-tar"),
    (".gz", "application/gzip"),
    (".mp3", "audio/mpeg"),
    (".ogg", "audio/ogg"),
    (".wav", "audio/wav"),
    (".silk", "audio/silk"),
    (".mp4", "video/mp4"),
    (".mov", "video/quicktime"),
    (".webm", "video/webm"),
    (".mkv", "video/x-matroska"),
    (".avi", "video/x-msvideo"),
    (".png", "image/png"),
    (".jpg", "image/jpeg"),
    (".jpeg", "image/jpeg"),
    (".gif", "image/gif"),
    (".webp", "image/webp"),
    (".bmp", "image/bmp"),
];

/// MIME type to extension (with leading dot).
const MIME_TO_EXTENSION: &[(&str, &str)] = &[
    ("image/jpeg", ".jpg"),
    ("image/jpg", ".jpg"),
    ("image/png", ".png"),
    ("image/gif", ".gif"),
    ("image/webp", ".webp"),
    ("image/bmp", ".bmp"),
    ("video/mp4", ".mp4"),
    ("video/quicktime", ".mov"),
    ("video/webm", ".webm"),
    ("video/x-matroska", ".mkv"),
    ("video/x-msvideo", ".avi"),
    ("audio/mpeg", ".mp3"),
    ("audio/ogg", ".ogg"),
    ("audio/wav", ".wav"),
    ("audio/silk", ".silk"),
    ("application/pdf", ".pdf"),
    ("application/zip", ".zip"),
    ("application/x-tar", ".tar"),
    ("application/gzip", ".gz"),
    ("text/plain", ".txt"),
    ("text/csv", ".csv"),
];

/// MIME type for an unknown extension.
pub const OCTET_STREAM: &str = "application/octet-stream";

/// Fallback extension when nothing matches.
pub const BIN_EXTENSION: &str = ".bin";

/// Get the MIME type from a filename extension.
///
/// Returns [`OCTET_STREAM`] for unknown extensions.
pub fn mime_from_filename(filename: &str) -> String {
    mime_from_extension(&extension_of(filename))
}

/// Get the MIME type for a lowercase-or-uppercase extension.
pub fn mime_from_extension(ext: &str) -> String {
    let ext = ext.to_ascii_lowercase();
    EXTENSION_TO_MIME
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, m)| m.to_string())
        .unwrap_or_else(|| OCTET_STREAM.to_string())
}

/// Get the file extension for a MIME type.
///
/// Returns [`BIN_EXTENSION`] for unknown types.
pub fn extension_from_mime(mime: &str) -> String {
    let ct = mime
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    MIME_TO_EXTENSION
        .iter()
        .find(|(m, _)| *m == ct)
        .map(|(_, e)| e.to_string())
        .unwrap_or_else(|| BIN_EXTENSION.to_string())
}

/// Get the file extension from a `Content-Type` header value or a URL.
///
/// `content_type` may be `None`, empty, or carry parameters (`; charset=...`).
pub fn extension_from_content_type_or_url(content_type: Option<&str>, url: &str) -> String {
    if let Some(ct) = content_type {
        let ext = extension_from_mime(ct);
        if ext != BIN_EXTENSION {
            return ext;
        }
    }
    let path_ext = url
        .parse::<url::Url>()
        .ok()
        .map(|u| extension_of(u.path()))
        .unwrap_or_else(|| extension_of(url));
    let path_ext = path_ext.to_ascii_lowercase();
    if EXTENSION_TO_MIME.iter().any(|(e, _)| *e == path_ext) {
        path_ext
    } else {
        BIN_EXTENSION.to_string()
    }
}

/// Lowercase extension of a path-like string, including the leading dot.
///
/// Returns an empty string when there is no extension.
pub fn extension_of(path: &str) -> String {
    // Take the last segment so a dot in a directory name is not mistaken
    // for an extension separator.
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());
    match name.rfind('.') {
        Some(0) | None => String::new(),
        Some(idx) => name[idx..].to_ascii_lowercase(),
    }
}

/// Media-type routing used by the outbound pipeline: `video/*`, `image/*`, else file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
    File,
}

impl MediaKind {
    /// Classify a local file path by MIME type.
    pub fn from_path(path: &str) -> Self {
        let mime = mime_from_filename(path);
        if mime.starts_with("video/") {
            MediaKind::Video
        } else if mime.starts_with("image/") {
            MediaKind::Image
        } else {
            MediaKind::File
        }
    }
}

/// `true` when `path` points at a video file.
pub fn is_video(path: &str) -> bool {
    mime_from_filename(path).starts_with("video/")
}

/// `true` when `path` points at an image file.
pub fn is_image(path: &str) -> bool {
    mime_from_filename(path).starts_with("image/")
}

/// `true` when `path` points at an audio file.
pub fn is_audio(path: &str) -> bool {
    mime_from_filename(path).starts_with("audio/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_from_extensions() {
        assert_eq!(mime_from_filename("a.PNG"), "image/png");
        assert_eq!(mime_from_filename("a.pdf"), "application/pdf");
        assert_eq!(mime_from_filename("a.unknown"), OCTET_STREAM);
        assert_eq!(mime_from_filename("noext"), OCTET_STREAM);
        assert_eq!(mime_from_filename("/a/b.c/d"), OCTET_STREAM);
    }

    #[test]
    fn extensions_from_mime() {
        assert_eq!(extension_from_mime("image/jpeg"), ".jpg");
        assert_eq!(extension_from_mime("Image/JPEG; charset=binary"), ".jpg");
        assert_eq!(extension_from_mime("application/x-weird"), ".bin");
        assert_eq!(extension_from_mime("audio/silk"), ".silk");
    }

    #[test]
    fn extension_of_edge_cases() {
        assert_eq!(extension_of("a.tar.gz"), ".gz");
        assert_eq!(extension_of("/x.y/.hidden"), "");
        assert_eq!(extension_of("plain"), "");
    }

    #[test]
    fn extension_from_content_type_or_url_test() {
        assert_eq!(
            extension_from_content_type_or_url(Some("image/png"), "https://x/y"),
            ".png"
        );
        assert_eq!(
            extension_from_content_type_or_url(Some("application/octet-stream"), "https://x/a.mp4"),
            ".mp4"
        );
        assert_eq!(
            extension_from_content_type_or_url(None, "https://x/a.unknownext"),
            ".bin"
        );
        assert_eq!(
            extension_from_content_type_or_url(None, "https://x/"),
            ".bin"
        );
    }

    #[test]
    fn media_kind_routing() {
        assert_eq!(MediaKind::from_path("a.mp4"), MediaKind::Video);
        assert_eq!(MediaKind::from_path("a.gif"), MediaKind::Image);
        assert_eq!(MediaKind::from_path("a.pdf"), MediaKind::File);
    }
}
