//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//
//! HMAC SHA-256

use crate::tls::sha2::sha256;

/// HMAC SHA-256
pub fn sign(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;

    // If the key is longer than the block size, shorten it using SHA-256.
    let mut key_block = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let hashed_key = sha256(key);
        key_block[..hashed_key.len()].copy_from_slice(&hashed_key);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    // Prepare the inner and outer key pads.
    let mut inner_pad = [0u8; BLOCK_SIZE];
    let mut outer_pad = [0u8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        inner_pad[i] = key_block[i] ^ 0x36;
        outer_pad[i] = key_block[i] ^ 0x5c;
    }

    // inner hash = SHA256((K ^ ipad) || data)
    let mut inner_buf = Vec::with_capacity(BLOCK_SIZE + data.len());
    inner_buf.extend_from_slice(&inner_pad);
    inner_buf.extend_from_slice(data);
    let inner_hash = sha256(&inner_buf);

    // outer hash = SHA256((K ^ opad) || inner_hash)
    const OUTER_INPUT_SIZE: usize = BLOCK_SIZE + 32;
    let mut outer_buf = [0u8; OUTER_INPUT_SIZE];
    outer_buf[..BLOCK_SIZE].copy_from_slice(&outer_pad);
    outer_buf[BLOCK_SIZE..].copy_from_slice(&inner_hash);

    sha256(&outer_buf)
}

#[cfg(test)]
mod tests {
    use crate::tls::tests::string_to_bytes;

    #[test]
    fn test_hmac_sha256_short() {
        let key = "secret";
        let data = "Hello";
        let output = super::sign(key.as_bytes(), data.as_bytes());
        let expected =
            string_to_bytes("0cc692f2177b42b6e5cd82488ee6c5d526a007c571e7de1fec07c1e2b1dfa2e2");
        assert_eq!(output, expected);
    }

    #[test]
    fn test_hmac_sha256_long() {
        let key = "secret";
        let data = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.";
        let output = super::sign(key.as_bytes(), data.as_bytes());
        let expected =
            string_to_bytes("602a9c4d44feea742c6775c21d686ccd899ee4c8363d7c03535b949c16a6b6d8");
        assert_eq!(output, expected);
    }
}
