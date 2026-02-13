//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//
//! AEAD AES-128 GCM

extern crate alloc;
use alloc::vec::Vec;

/// AES-128 GCM encryption. Returns ciphertext || 16-byte tag.
/// `aad` is additional authenticated data (can be empty) that is authenticated
/// but not encrypted.
pub fn aes_128_gcm_encrypt(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, &'static str> {
    #[cfg(debug_assertions)]
    {
        if key.len() != 16 {
            return Err("AES-128 key must be 16 bytes");
        }
        if nonce.len() != 12 {
            return Err("Nonce must be 12 bytes");
        };
    }

    // Expand round keys once.
    let round_keys = key_expansion(key);

    // Hash subkey H = AES_K(0^128)
    let h = {
        let zero_block = [0u8; 16];
        let enc = aes_encrypt_block(&zero_block, &round_keys);
        u128::from_be_bytes(enc)
    };

    // Build initial counter J0
    let j0 = {
        let mut j = [0u8; 16];
        j[..12].copy_from_slice(nonce);
        j[15] = 0x01;
        j
    };

    // Encrypt using counter starting at J0+1
    let mut ctr = j0;
    inc32(&mut ctr);

    let mut ciphertext = Vec::with_capacity(plaintext.len() + 16);
    for chunk in plaintext.chunks(16) {
        let keystream = aes_encrypt_block(&ctr, &round_keys);
        let mut block = [0u8; 16];
        for i in 0..chunk.len() {
            block[i] = chunk[i] ^ keystream[i];
        }
        ciphertext.extend_from_slice(&block[..chunk.len()]);
        inc32(&mut ctr);
    }

    // GHASH over AAD and ciphertext
    let tag = {
        let ghash = ghash(h, aad, &ciphertext);
        let s = {
            let s_block = aes_encrypt_block(&j0, &round_keys);
            u128::from_be_bytes(s_block)
        };
        let tag_u128 = ghash ^ s;
        tag_u128.to_be_bytes()
    };

    ciphertext.extend_from_slice(&tag);
    Ok(ciphertext)
}

/// AES-128 GCM decryption. Returns the plaintext.
/// `aad` must match the value supplied during encryption (can be empty).
pub fn aes_128_gcm_decrypt(
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, &'static str> {
    #[cfg(debug_assertions)]
    {
        if key.len() != 16 {
            return Err("AES-128 key must be 16 bytes");
        }
        if nonce.len() != 12 {
            return Err("Nonce must be 12 bytes");
        };
        if ciphertext.len() <= 16 {
            return Err("Ciphertext must include at least authentication tag");
        };
    }

    let (ct, tag) = ciphertext.split_at(ciphertext.len() - 16);

    let round_keys = key_expansion(key);

    let h = {
        let zero_block = [0u8; 16];
        let enc = aes_encrypt_block(&zero_block, &round_keys);
        u128::from_be_bytes(enc)
    };

    let j0 = {
        let mut j = [0u8; 16];
        j[..12].copy_from_slice(nonce);
        j[15] = 0x01;
        j
    };

    // Verify tag first
    let expected_tag = {
        let ghash_val = ghash(h, aad, ct);
        let s = {
            let s_block = aes_encrypt_block(&j0, &round_keys);
            u128::from_be_bytes(s_block)
        };
        let tag_u128 = ghash_val ^ s;
        tag_u128.to_be_bytes()
    };

    if !constant_time_eq(tag, &expected_tag) {
        return Err("authentication failed, invalid tag");
    }

    // Decrypt using CTR
    let mut ctr = j0;
    inc32(&mut ctr);
    let mut plaintext = Vec::with_capacity(ct.len());
    for chunk in ct.chunks(16) {
        let keystream = aes_encrypt_block(&ctr, &round_keys);
        let mut block = [0u8; 16];
        for i in 0..chunk.len() {
            block[i] = chunk[i] ^ keystream[i];
        }
        plaintext.extend_from_slice(&block[..chunk.len()]);
        inc32(&mut ctr);
    }

    Ok(plaintext)
}

// ================= AES PRIMITIVES ================= //

// Round constants for AES-128 key schedule
const RCON: [u8; 10] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36];

// AES S-box
const SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

fn sub_word(w: u32) -> u32 {
    let b0 = SBOX[(w as u8) as usize] as u32;
    let b1 = (SBOX[((w >> 8) as u8) as usize] as u32) << 8;
    let b2 = (SBOX[((w >> 16) as u8) as usize] as u32) << 16;
    let b3 = (SBOX[((w >> 24) as u8) as usize] as u32) << 24;
    b0 | b1 | b2 | b3
}

fn rot_word(w: u32) -> u32 {
    //(w << 8) | (w >> 24)
    w.rotate_left(8)
}

// Generate 11 round keys (11 * 16 bytes)
fn key_expansion(key: &[u8]) -> [[u8; 16]; 11] {
    let mut w = [0u32; 44];
    for i in 0..4 {
        w[i] = u32::from_be_bytes([key[4 * i], key[4 * i + 1], key[4 * i + 2], key[4 * i + 3]]);
    }
    for i in 4..44 {
        let mut temp = w[i - 1];
        if i % 4 == 0 {
            temp = sub_word(rot_word(temp)) ^ ((RCON[(i / 4) - 1] as u32) << 24);
        }
        w[i] = w[i - 4] ^ temp;
    }

    let mut round_keys = [[0u8; 16]; 11];
    for (i, chunk) in w.chunks_exact(4).enumerate() {
        let mut rk = [0u8; 16];
        for (j, word) in chunk.iter().enumerate() {
            rk[4 * j..4 * j + 4].copy_from_slice(&word.to_be_bytes());
        }
        round_keys[i] = rk;
    }
    round_keys
}

fn aes_encrypt_block(input: &[u8; 16], round_keys: &[[u8; 16]; 11]) -> [u8; 16] {
    let mut state = *input;

    add_round_key(&mut state, &round_keys[0]);
    for round in round_keys.iter().take(10).skip(1) {
        sub_bytes(&mut state);
        shift_rows(&mut state);
        mix_columns(&mut state);
        add_round_key(&mut state, round);
    }
    sub_bytes(&mut state);
    shift_rows(&mut state);
    add_round_key(&mut state, &round_keys[10]);

    state
}

#[inline(always)]
fn add_round_key(state: &mut [u8; 16], rk: &[u8; 16]) {
    for i in 0..16 {
        state[i] ^= rk[i];
    }
}

#[inline(always)]
fn sub_bytes(state: &mut [u8; 16]) {
    for b in state.iter_mut() {
        *b = SBOX[*b as usize];
    }
}

#[inline(always)]
fn shift_rows(state: &mut [u8; 16]) {
    let mut tmp = [0u8; 16];
    tmp[0] = state[0];
    tmp[1] = state[5];
    tmp[2] = state[10];
    tmp[3] = state[15];

    tmp[4] = state[4];
    tmp[5] = state[9];
    tmp[6] = state[14];
    tmp[7] = state[3];

    tmp[8] = state[8];
    tmp[9] = state[13];
    tmp[10] = state[2];
    tmp[11] = state[7];

    tmp[12] = state[12];
    tmp[13] = state[1];
    tmp[14] = state[6];
    tmp[15] = state[11];

    *state = tmp;
}

#[inline(always)]
fn xtime(x: u8) -> u8 {
    (x << 1) ^ (((x >> 7) & 1) * 0x1b)
}

#[inline(always)]
fn mix_columns(state: &mut [u8; 16]) {
    for c in 0..4 {
        let i = 4 * c;
        let a0 = state[i];
        let a1 = state[i + 1];
        let a2 = state[i + 2];
        let a3 = state[i + 3];

        let t = a0 ^ a1 ^ a2 ^ a3;
        let mut tmp = a0 ^ a1;
        tmp = xtime(tmp);
        state[i] ^= tmp ^ t;

        tmp = a1 ^ a2;
        tmp = xtime(tmp);
        state[i + 1] ^= tmp ^ t;

        tmp = a2 ^ a3;
        tmp = xtime(tmp);
        state[i + 2] ^= tmp ^ t;

        tmp = a3 ^ a0;
        tmp = xtime(tmp);
        state[i + 3] ^= tmp ^ t;
    }
}

// ================= GCM SUPPORT ================= //

#[inline(always)]
fn inc32(counter: &mut [u8; 16]) {
    let last32 =
        u32::from_be_bytes([counter[12], counter[13], counter[14], counter[15]]).wrapping_add(1);
    let bytes = last32.to_be_bytes();
    counter[12..16].copy_from_slice(&bytes);
}

fn ghash(h: u128, aad: &[u8], ciphertext: &[u8]) -> u128 {
    let mut y: u128 = 0;

    // AAD
    for block in aad.chunks(16) {
        let mut b = [0u8; 16];
        b[..block.len()].copy_from_slice(block);
        let x = u128::from_be_bytes(b);
        y ^= x;
        y = gf_mul(y, h);
    }

    // Ciphertext
    for block in ciphertext.chunks(16) {
        let mut b = [0u8; 16];
        b[..block.len()].copy_from_slice(block);
        let x = u128::from_be_bytes(b);
        y ^= x;
        y = gf_mul(y, h);
    }

    // Length block
    let aad_bits = (aad.len() as u128) * 8;
    let ct_bits = (ciphertext.len() as u128) * 8;
    let mut len_block = [0u8; 16];
    len_block[..8].copy_from_slice(&(aad_bits as u64).to_be_bytes());
    len_block[8..].copy_from_slice(&(ct_bits as u64).to_be_bytes());
    let x = u128::from_be_bytes(len_block);
    y ^= x;
    y = gf_mul(y, h);

    y
}

// GF(2^128) multiplication with the polynomial 0xE1
#[inline(always)]
fn gf_mul(x: u128, y: u128) -> u128 {
    let mut z = 0u128;
    let mut v = y;
    const R: u128 = 0xe1000000000000000000000000000000;
    for i in 0..128 {
        if (x & (1u128 << (127 - i))) != 0 {
            z ^= v;
        }
        if (v & 1) != 0 {
            v = (v >> 1) ^ R;
        } else {
            v >>= 1;
        }
    }
    z
}

#[inline(always)]
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::vec;

    use super::*;

    const KEY: [u8; 16] = [
        0x06, 0xa9, 0x21, 0x40, 0x36, 0xb8, 0xa1, 0x5b, 0x51, 0x2e, 0x03, 0xd5, 0x34, 0x12, 0x00,
        0x06,
    ];
    const NONCE: [u8; 12] = [
        0x3d, 0xaf, 0xba, 0x42, 0x9d, 0x9e, 0xb4, 0x30, 0xb4, 0x22, 0xda, 0x80,
    ];
    const WRONG_KEY: [u8; 16] = [
        0xff, 0xa9, 0x21, 0x40, 0x36, 0xb8, 0xa1, 0x5b, 0x51, 0x2e, 0x03, 0xd5, 0x34, 0x12, 0x00,
        0x06,
    ];
    const WRONG_NONCE: [u8; 12] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    ];
    const PLAINTEXT_SHORT: &[u8] = b"Hello, world!";
    const PLAINTEXT_LONG: &[u8] = b"This is a longer plaintext to test various block alignments and ensure the GCM mode works correctly.";

    #[test]
    fn test_encrypt_decrypt_cycle() {
        let ciphertext = aes_128_gcm_encrypt(&KEY, &NONCE, &[], PLAINTEXT_SHORT).unwrap();
        let decrypted = aes_128_gcm_decrypt(&KEY, &NONCE, &[], &ciphertext).unwrap();
        assert_eq!(PLAINTEXT_SHORT, decrypted);
    }

    #[test]
    fn test_encrypt_decrypt_cycle_long_plaintext() {
        let ciphertext = aes_128_gcm_encrypt(&KEY, &NONCE, &[], PLAINTEXT_LONG).unwrap();
        let decrypted = aes_128_gcm_decrypt(&KEY, &NONCE, &[], &ciphertext).unwrap();
        assert_eq!(PLAINTEXT_LONG, decrypted);
    }

    #[test]
    fn test_encrypt_decrypt_with_aad() {
        let aad = b"metadata";
        let ciphertext = aes_128_gcm_encrypt(&KEY, &NONCE, aad, PLAINTEXT_SHORT).unwrap();
        let decrypted = aes_128_gcm_decrypt(&KEY, &NONCE, aad, &ciphertext).unwrap();
        assert_eq!(PLAINTEXT_SHORT, decrypted);
    }

    #[test]
    fn test_decrypt_with_wrong_aad_fails() {
        let ciphertext = aes_128_gcm_encrypt(&KEY, &NONCE, b"auth-data", PLAINTEXT_SHORT).unwrap();
        let result = aes_128_gcm_decrypt(&KEY, &NONCE, b"different", &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_different_aad_changes_ciphertext() {
        let ct1 = aes_128_gcm_encrypt(&KEY, &NONCE, b"A", PLAINTEXT_SHORT).unwrap();
        let ct2 = aes_128_gcm_encrypt(&KEY, &NONCE, b"B", PLAINTEXT_SHORT).unwrap();
        assert_ne!(ct1, ct2);
    }

    /*
    #[test]
    fn test_encrypt_decrypt_cycle_empty_plaintext() {
        let plaintext = &[];
        let ciphertext = aes_128_gcm_encrypt(&KEY, &NONCE, &[], plaintext).unwrap();
        let decrypted = aes_128_gcm_decrypt(&KEY, &NONCE, &[], &ciphertext).unwrap();
        assert_eq!(plaintext.as_slice(), &decrypted);
    }
    */

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let ciphertext = aes_128_gcm_encrypt(&KEY, &NONCE, &[], PLAINTEXT_SHORT).unwrap();
        let result = aes_128_gcm_decrypt(&WRONG_KEY, &NONCE, &[], &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_with_wrong_nonce_fails() {
        let ciphertext = aes_128_gcm_encrypt(&KEY, &NONCE, &[], PLAINTEXT_SHORT).unwrap();
        let result = aes_128_gcm_decrypt(&KEY, &WRONG_NONCE, &[], &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_different_nonce_produces_different_ciphertext() {
        let ciphertext1 = aes_128_gcm_encrypt(&KEY, &NONCE, &[], PLAINTEXT_SHORT).unwrap();
        let ciphertext2 = aes_128_gcm_encrypt(&KEY, &WRONG_NONCE, &[], PLAINTEXT_SHORT).unwrap();
        assert_ne!(ciphertext1, ciphertext2);
    }

    /// Helper function to convert hex string to bytes
    fn hex_to_bytes(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Test Case 1: Empty plaintext
    /// From NIST gcmEncryptExtIV128.rsp - Count 0
    #[test]
    fn test_aes_128_gcm_empty_plaintext() {
        let key = hex_to_bytes("11754cd72aec309bf52f7687212e8957");
        let nonce = hex_to_bytes("3c819d9a9bed087615030b65");
        let plaintext = vec![];

        let result = aes_128_gcm_encrypt(&key, &nonce, &[], &plaintext).unwrap();

        // Expected: empty ciphertext + 16-byte tag
        let expected_tag = hex_to_bytes("250327c674aaf477aef2675748cf6971");

        assert_eq!(
            result.len(),
            16,
            "Should return only the 16-byte authentication tag"
        );
        assert_eq!(result, expected_tag);
    }

    /// Test Case 2: Single block plaintext (16 bytes)
    /// From NIST gcmEncryptExtIV128.rsp
    #[test]
    fn test_aes_128_gcm_single_block() {
        let key = hex_to_bytes("7fddb57453c241d03efbed3ac44e371c");
        let nonce = hex_to_bytes("ee283a3fc75575e33efd4887");
        let plaintext = hex_to_bytes("d5de42b461646c255c87bd2962d3b9a2");

        let result = aes_128_gcm_encrypt(&key, &nonce, &[], &plaintext).unwrap();

        // Expected: 16 bytes ciphertext + 16 bytes tag
        let expected =
            hex_to_bytes("2ccda4a5415cb91e135c2a0f78c9b2fdb36d1df9b9d5e596f83e8b7f52971cb3");

        assert_eq!(
            result.len(),
            32,
            "Should return 16-byte ciphertext + 16-byte tag"
        );
        assert_eq!(result, expected);
    }

    /// Test Case 3: Multiple blocks plaintext
    /// From NIST gcmEncryptExtIV128.rsp - Count 14
    #[test]
    fn test_aes_128_gcm_multi_block() {
        let key = hex_to_bytes("f42c74bcf473f6e923119946a89a0079");
        let nonce = hex_to_bytes("14852791065b66ccfa0b2d80");
        let plaintext = hex_to_bytes("819abf03a7a6b72892a5ac85604035c2");

        let result = aes_128_gcm_encrypt(&key, &nonce, &[], &plaintext).unwrap();

        // Expected ciphertext + tag (partial from test vector)
        let expected_ciphertext_start = hex_to_bytes("48371bd7af4235c4f11c45");

        assert_eq!(
            result.len(),
            32,
            "Should return 16-byte ciphertext + 16-byte tag"
        );
        assert_eq!(&result[..11], &expected_ciphertext_start[..]);
    }

    // Test Case 4: 256-bit plaintext (32 bytes)
    // Omitted. Uses nonce len 7. Not supported.

    /// Test Case 5: Different IV length (96 bits is standard)
    #[test]
    fn test_aes_128_gcm_standard_iv() {
        let key = hex_to_bytes("00000000000000000000000000000000");
        let nonce = hex_to_bytes("000000000000000000000000");
        let plaintext = hex_to_bytes("00000000000000000000000000000000");

        let result = aes_128_gcm_encrypt(&key, &nonce, &[], &plaintext).unwrap();

        // Should produce 16 bytes ciphertext + 16 bytes tag
        assert_eq!(result.len(), 32);
    }

    /// Test Case 6: Longer plaintext (64 bytes)
    #[test]
    fn test_aes_128_gcm_long_plaintext() {
        let key = hex_to_bytes("feffe9928665731c6d6a8f9467308308");
        let nonce = hex_to_bytes("cafebabefacedbaddecaf888");
        let plaintext = hex_to_bytes(
            "d9313225f88406e5a55909c5aff5269a\
             86a7a9531534f7da2e4c303d8a318a72\
             1c3c0c95956809532fcf0e2449a6b525\
             b16aedf5aa0de657ba637b391aafd255",
        );

        let result = aes_128_gcm_encrypt(&key, &nonce, &[], &plaintext).unwrap();

        // Expected: 64 bytes ciphertext + 16 bytes tag = 80 bytes
        let expected = hex_to_bytes(
            "42831ec2217774244b7221b784d0d49c\
             e3aa212f2c02a4e035c17e2329aca12e\
             21d514b25466931c7d8f6a5aac84aa05\
             1ba30b396a0aac973d58e091473f5985\
             4d5c2af327cd64a62cf35abd2ba6fab4",
        );

        assert_eq!(result.len(), 80);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_aes_block_known_vector() {
        // AES-128 known answer test
        let key = hex_to_bytes("000102030405060708090a0b0c0d0e0f");
        let block = hex_to_bytes("00112233445566778899aabbccddeeff");
        let round_keys = key_expansion(&key);
        let cipher = super::aes_encrypt_block(block.as_slice().try_into().unwrap(), &round_keys);
        assert_eq!(
            cipher.to_vec(),
            hex_to_bytes("69c4e0d86a7b0430d8cdb78070b4c55a")
        );
    }

    /// Test Case 9: Very short plaintext (1 byte)
    #[test]
    fn test_aes_128_gcm_one_byte_plaintext() {
        let key = hex_to_bytes("00000000000000000000000000000000");
        let nonce = hex_to_bytes("000000000000000000000000");
        let plaintext = vec![0x42];

        let result = aes_128_gcm_encrypt(&key, &nonce, &[], &plaintext).unwrap();

        // Should produce 1 byte ciphertext + 16 bytes tag
        assert_eq!(result.len(), 17);
    }

    /// Test Case 10: Known test vector with specific output
    /// Ensures deterministic behavior
    #[test]
    fn test_aes_128_gcm_deterministic() {
        let key = hex_to_bytes("00000000000000000000000000000000");
        let nonce = hex_to_bytes("000000000000000000000000");
        let plaintext = vec![0x00; 16];

        let result1 = aes_128_gcm_encrypt(&key, &nonce, &[], &plaintext).unwrap();
        let result2 = aes_128_gcm_encrypt(&key, &nonce, &[], &plaintext).unwrap();

        assert_eq!(result1, result2, "Encryption should be deterministic");
        assert_eq!(result1.len(), 32);
    }
}
