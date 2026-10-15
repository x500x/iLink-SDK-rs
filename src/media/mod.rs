//! MIME helpers.

pub mod mime;

pub use mime::{
    extension_from_content_type_or_url, extension_from_mime, extension_of, is_audio, is_image,
    is_video, mime_from_extension, mime_from_filename, MediaKind, BIN_EXTENSION, OCTET_STREAM,
};
