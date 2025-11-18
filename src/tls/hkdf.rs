//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//
//! HKDF - HMAC SHA-256 Key Derivation, RFC 5869

use crate::tls::hmac;

/// HKDF-Extract as defined in RFC 5869 using SHA-256.
pub fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    let zero_salt = [0u8; 32];
    let key = if salt.is_empty() {
        &zero_salt[..]
    } else {
        salt
    };
    hmac::sign(key, ikm)
}

/// HKDF-Expand as defined in RFC 5869 using SHA-256.
pub fn hkdf_expand(prk: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    const HASH_LEN: usize = 32;
    assert!(prk.len() == HASH_LEN, "prk must be 32 bytes for SHA-256");
    assert!(len <= 255 * HASH_LEN, "length too large for HKDF expand");

    if len == 0 {
        return Vec::new();
    }

    let mut okm = Vec::with_capacity(len);
    let mut previous = [0u8; HASH_LEN];
    let mut counter: u8 = 0;
    let mut block_input = Vec::with_capacity(HASH_LEN + info.len() + 1);

    while okm.len() < len {
        counter = counter.checked_add(1).expect("HKDF block counter overflow");
        block_input.clear();
        if counter > 1 {
            block_input.extend_from_slice(&previous);
        }
        block_input.extend_from_slice(info);
        block_input.push(counter);

        previous = hmac::sign(prk, &block_input);
        let remaining = len - okm.len();
        let take = remaining.min(HASH_LEN);
        okm.extend_from_slice(&previous[..take]);
    }

    okm
}

#[cfg(test)]
mod tests {
    use crate::tls::tests::{hex_to_vec, string_to_bytes};

    #[test]
    fn test_hkdf_extract_rfc5869_case1() {
        let ikm = [0x0bu8; 22];
        let salt = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
        ];
        let expected =
            string_to_bytes("077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5");
        let prk = super::hkdf_extract(&salt, &ikm);
        assert_eq!(prk, expected);
    }

    #[test]
    fn test_hkdf_expand_rfc5869_case1() {
        let prk =
            string_to_bytes("077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5");
        let info = [0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9];
        let okm = super::hkdf_expand(&prk, &info, 42);
        let expected = hex_to_vec(
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
        );
        assert_eq!(okm, expected);
    }

    #[test]
    fn test_hkdf_extract_empty_salt() {
        let ikm = b"handshake secret";
        let prk = super::hkdf_extract(&[], ikm);
        let zero_salt = [0u8; 32];
        let expected = crate::tls::hmac::sign(&zero_salt, ikm);
        assert_eq!(prk, expected);
    }

    #[test]
    fn test_hkdf_expand_zero_len() {
        let prk = [0xabu8; 32];
        let okm = super::hkdf_expand(&prk, b"", 0);
        assert!(okm.is_empty());
    }
}
