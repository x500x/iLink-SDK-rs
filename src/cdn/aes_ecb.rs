//! Shared AES-128-ECB crypto utilities for CDN upload and download.
//!
//! Ports `src/cdn/aes-ecb.ts`. The algorithm is AES-128 in ECB mode with
//! PKCS#7 padding.
//!
//! ECB is implemented as a sequence of independent block encryptions rather
//! than through a chaining mode: the two are *not* interchangeable, because
//! CBC with a zero IV still XORs each plaintext block against the previous
//! ciphertext block, so repeated plaintext blocks produce different
//! ciphertext. The Weixin backend expects exact ECB output, so the blocks are
//! transformed one at a time here.

use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes128;

use crate::error::{Error, ErrorKind, Result};

/// AES block size in bytes.
pub const BLOCK_SIZE: usize = 16;

fn cipher(key: &[u8]) -> Aes128 {
    Aes128::new(&GenericArray::clone_from_slice(key))
}

/// Encrypt `plaintext` with AES-128-ECB using PKCS#7 padding.
///
/// # Errors
/// Returns an [`Error`] when `key` is not exactly 16 bytes.
pub fn encrypt_aes_ecb(plaintext: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    validate_key(key)?;
    let cipher = cipher(key);
    let mut out = Vec::with_capacity(aes_ecb_padded_size(plaintext.len()));

    for chunk in plaintext.chunks(BLOCK_SIZE) {
        let mut block = [0u8; BLOCK_SIZE];
        // PKCS#7 always appends between 1 and BLOCK_SIZE bytes.
        let pad_len = BLOCK_SIZE - chunk.len();
        block[..chunk.len()].copy_from_slice(chunk);
        for byte in block.iter_mut().skip(chunk.len()) {
            *byte = pad_len as u8;
        }
        let mut block = GenericArray::from(block);
        cipher.encrypt_block(&mut block);
        out.extend_from_slice(&block);
    }

    // An empty or block-aligned input still needs a full block of padding.
    if plaintext.len() % BLOCK_SIZE == 0 {
        let mut block = [0u8; BLOCK_SIZE];
        for byte in block.iter_mut() {
            *byte = BLOCK_SIZE as u8;
        }
        let mut block = GenericArray::from(block);
        cipher.encrypt_block(&mut block);
        out.extend_from_slice(&block);
    }

    Ok(out)
}

/// Decrypt `ciphertext` with AES-128-ECB and strip PKCS#7 padding.
///
/// # Errors
/// Returns an [`Error`] when `key` is not 16 bytes, the ciphertext length is
/// not a positive multiple of 16, or the padding is invalid.
pub fn decrypt_aes_ecb(ciphertext: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    validate_key(key)?;
    if ciphertext.is_empty() || ciphertext.len() % BLOCK_SIZE != 0 {
        return Err(Error::new(
            ErrorKind::Unknown,
            format!(
                "aes-128-ecb: ciphertext length {} is not a positive multiple of {}",
                ciphertext.len(),
                BLOCK_SIZE
            ),
        ));
    }
    let decipher = cipher(key);
    let mut out = Vec::with_capacity(ciphertext.len());
    for chunk in ciphertext.chunks(BLOCK_SIZE) {
        let mut block = GenericArray::clone_from_slice(chunk);
        decipher.decrypt_block(&mut block);
        out.extend_from_slice(&block);
    }
    strip_pkcs7(&out)
}

/// Compute the AES-128-ECB ciphertext size (PKCS#7 pads to a 16-byte boundary).
///
/// PKCS#7 always appends at least one byte and a full extra block when the
/// input is already block-aligned.
pub fn aes_ecb_padded_size(plaintext_size: usize) -> usize {
    (plaintext_size / BLOCK_SIZE + 1) * BLOCK_SIZE
}

fn validate_key(key: &[u8]) -> Result<()> {
    if key.len() != BLOCK_SIZE {
        return Err(Error::new(
            ErrorKind::Unknown,
            format!(
                "aes-128-ecb: key must be {} bytes, got {}",
                BLOCK_SIZE,
                key.len()
            ),
        ));
    }
    Ok(())
}

fn strip_pkcs7(data: &[u8]) -> Result<Vec<u8>> {
    let Some(&last) = data.last() else {
        return Err(Error::new(
            ErrorKind::Unknown,
            "aes-128-ecb: empty plaintext",
        ));
    };
    let pad = last as usize;
    if pad == 0 || pad > BLOCK_SIZE || pad > data.len() {
        return Err(Error::new(
            ErrorKind::Unknown,
            "aes-128-ecb: decryption failed (bad padding)",
        ));
    }
    if !data[data.len() - pad..].iter().all(|&b| b as usize == pad) {
        return Err(Error::new(
            ErrorKind::Unknown,
            "aes-128-ecb: decryption failed (bad padding)",
        ));
    }
    Ok(data[..data.len() - pad].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padded_size_always_grows_by_at_least_one_block() {
        assert_eq!(aes_ecb_padded_size(0), 16);
        assert_eq!(aes_ecb_padded_size(1), 16);
        assert_eq!(aes_ecb_padded_size(15), 16);
        assert_eq!(aes_ecb_padded_size(16), 32);
        assert_eq!(aes_ecb_padded_size(17), 32);
    }

    #[test]
    fn round_trips_text() {
        let key = [7u8; 16];
        let ct = encrypt_aes_ecb(b"hello world", &key).unwrap();
        assert_eq!(ct.len(), 16);
        let pt = decrypt_aes_ecb(&ct, &key).unwrap();
        assert_eq!(pt, b"hello world");
    }

    #[test]
    fn round_trips_block_aligned_input() {
        let key = [9u8; 16];
        let plaintext = vec![0xABu8; 32];
        let ct = encrypt_aes_ecb(&plaintext, &key).unwrap();
        assert_eq!(ct.len(), 48);
        let pt = decrypt_aes_ecb(&ct, &key).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn round_trips_empty_input() {
        let key = [1u8; 16];
        let ct = encrypt_aes_ecb(b"", &key).unwrap();
        assert_eq!(ct.len(), 16);
        assert_eq!(decrypt_aes_ecb(&ct, &key).unwrap(), b"");
    }

    #[test]
    fn round_trips_large_random_payload() {
        let key: Vec<u8> = (0..16u8).collect();
        let plaintext: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let ct = encrypt_aes_ecb(&plaintext, &key).unwrap();
        assert_eq!(ct.len(), aes_ecb_padded_size(plaintext.len()));
        assert_eq!(decrypt_aes_ecb(&ct, &key).unwrap(), plaintext);
    }

    #[test]
    fn rejects_wrong_key_length() {
        assert!(encrypt_aes_ecb(b"x", &[0u8; 8]).is_err());
        assert!(decrypt_aes_ecb(&[0u8; 32], &[0u8; 24]).is_err());
    }

    #[test]
    fn rejects_misaligned_ciphertext() {
        let key = [3u8; 16];
        assert!(decrypt_aes_ecb(&[0u8; 17], &key).is_err());
    }

    #[test]
    fn rejects_bad_padding() {
        let key = [3u8; 16];
        // All 0xFF bytes can never be valid PKCS#7 padding.
        assert!(decrypt_aes_ecb(&[0xFFu8; 16], &key).is_err());
    }

    #[test]
    fn identical_plaintext_blocks_produce_identical_ciphertext() {
        // ECB's defining property: equal plaintext blocks map to equal
        // ciphertext blocks. With a chaining mode (CBC) the second half would
        // differ because the first half feeds forward as an "IV".
        let key = [0x42u8; 16];
        let block = vec![0x41u8; 16];

        let ct_two = encrypt_aes_ecb(&[block.clone(), block.clone()].concat(), &key).unwrap();
        assert_eq!(ct_two.len(), 48);

        // Both data blocks must have produced the same ciphertext block.
        assert_eq!(ct_two[..16], ct_two[16..32]);

        // Determinism: the same input always yields the same ciphertext.
        let ct_two_again = encrypt_aes_ecb(&[block.clone(), block.clone()].concat(), &key).unwrap();
        assert_eq!(ct_two, ct_two_again);
    }
}
