//! Prefixed unique-ID generation, mirroring `src/util/random.ts`.
//!
//! ```text
//! generate_id("openclaw-weixin") -> "openclaw-weixin:1761000000123-a1b2c3d4"
//! temp_file_name("weixin-remote", ".png") -> "weixin-remote-1761000000123-a1b2c3d4.png"
//! ```

use rand::RngCore;

/// Generate a prefixed unique ID using a millisecond timestamp plus 4 random
/// bytes rendered as hex.
pub fn generate_id(prefix: &str) -> String {
    let mut bytes = [0u8; 4];
    rand::rng().fill_bytes(&mut bytes);
    format!("{}:{}-{}", prefix, now_millis(), hex::encode(bytes))
}

/// Generate a temporary file name with a random suffix.
pub fn temp_file_name(prefix: &str, ext: &str) -> String {
    let mut bytes = [0u8; 4];
    rand::rng().fill_bytes(&mut bytes);
    format!("{}-{}-{}{}", prefix, now_millis(), hex::encode(bytes), ext)
}

/// Re-exported from [super::time] for backwards compatibility.
pub use super::time::now_millis;

/// Generate 16 random bytes rendered as a lowercase hex string (32 chars).
///
/// Used for the CDN `filekey`.
pub fn file_key() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Generate a random uint32 rendered as a decimal string.
pub fn random_u32() -> u32 {
    rand::rng().next_u32()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_id_format() {
        let a = generate_id("weixin");
        let b = generate_id("weixin");
        assert!(a.starts_with("weixin:"));
        assert_ne!(a, b);
        assert_eq!(a.split(':').count(), 2);
        let (_, rest) = a.split_once(':').unwrap();
        let (ts, rand) = rest.split_once('-').unwrap();
        assert!(ts.parse::<u128>().unwrap() > 1_600_000_000_000);
        assert_eq!(rand.len(), 8);
    }

    #[test]
    fn temp_file_name_format() {
        let name = temp_file_name("weixin-remote", ".png");
        assert!(name.starts_with("weixin-remote-"));
        assert!(name.ends_with(".png"));
        assert_ne!(name, temp_file_name("weixin-remote", ".png"));
    }

    #[test]
    fn file_key_is_32_hex_chars() {
        let k = file_key();
        assert_eq!(k.len(), 32);
        assert!(k.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn random_u32_differs() {
        assert_ne!(random_u32(), random_u32());
    }
}
