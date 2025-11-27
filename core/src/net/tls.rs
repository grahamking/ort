//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//
//! ---------------------- Minimal TLS 1.3 client (AES-128-GCM + X25519) -------

pub mod aead;
pub mod ecdh;
pub mod hkdf;
pub mod hmac;
pub mod sha2;

#[cfg(test)]
pub mod tests {
    extern crate alloc;
    use alloc::vec::Vec;

    pub fn string_to_bytes(s: &str) -> [u8; 32] {
        let mut bytes = s.as_bytes();
        if bytes.len() >= 2 && bytes[0] == b'0' && (bytes[1] == b'x' || bytes[1] == b'X') {
            bytes = &bytes[2..];
        }
        assert!(
            bytes.len() == 64,
            "hex string must be exactly 64 hex chars (32 bytes)"
        );

        let mut out = [0u8; 32];
        for i in 0..32 {
            let hi = hex_val(bytes[2 * i]);
            let lo = hex_val(bytes[2 * i + 1]);
            out[i] = (hi << 4) | lo;
        }
        out
    }

    pub fn hex_to_vec(s: &str) -> Vec<u8> {
        let mut bytes = s.as_bytes();
        if bytes.len() >= 2 && bytes[0] == b'0' && (bytes[1] == b'X' || bytes[1] == b'x') {
            bytes = &bytes[2..];
        }
        assert_eq!(bytes.len() % 2, 0, "hex string must have even length");
        let mut out = Vec::with_capacity(bytes.len() / 2);
        for chunk in bytes.chunks_exact(2) {
            let hi = hex_val(chunk[0]);
            let lo = hex_val(chunk[1]);
            out.push((hi << 4) | lo);
        }
        out
    }

    fn hex_val(b: u8) -> u8 {
        match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => panic!("invalid hex character"),
        }
    }
}
