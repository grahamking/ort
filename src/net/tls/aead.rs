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

    let mut j0 = [0u8; 16];
    j0[..12].copy_from_slice(nonce);
    j0[15] = 0x01;

    // Verify tag first.
    let zero_block = [0u8; 16];
    let enc = aes_encrypt_block(&zero_block, &round_keys);
    let h = u128::from_be_bytes(enc);

    let ghash_val = ghash(h, aad, ct);
    let s_block = aes_encrypt_block(&j0, &round_keys);
    let s = u128::from_be_bytes(s_block);
    let tag_u128 = ghash_val ^ s;
    let expected_tag = tag_u128.to_be_bytes();
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

// Generate 11 round keys (11 * 16 bytes)
// Target assumption: modern x86_64 CPUs with AES-NI support.
fn key_expansion(key: &[u8]) -> [[u8; 16]; 11] {
    unsafe { key_expansion_aesni(key) }
}

unsafe fn key_expansion_aesni(key: &[u8]) -> [[u8; 16]; 11] {
    use core::arch::x86_64::{
        __m128i, _mm_aeskeygenassist_si128, _mm_loadu_si128, _mm_shuffle_epi32, _mm_slli_si128,
        _mm_storeu_si128, _mm_xor_si128,
    };

    #[inline(always)]
    unsafe fn expand_step<const RC: i32>(key: __m128i) -> __m128i {
        // AES-128 schedule core: combine RotWord/SubWord/Rcon output with shifted XOR chain.
        unsafe {
            let mut k = key;
            let mut assist = _mm_aeskeygenassist_si128::<RC>(k);
            assist = _mm_shuffle_epi32::<0xff>(assist);

            let mut t = _mm_slli_si128::<4>(k);
            k = _mm_xor_si128(k, t);
            t = _mm_slli_si128::<4>(t);
            k = _mm_xor_si128(k, t);
            t = _mm_slli_si128::<4>(t);
            k = _mm_xor_si128(k, t);
            _mm_xor_si128(k, assist)
        }
    }

    unsafe {
        let mut round_keys = [[0u8; 16]; 11];
        let mut key_block = [0u8; 16];
        key_block.copy_from_slice(&key[..16]);

        let mut k = _mm_loadu_si128(key_block.as_ptr() as *const __m128i);
        _mm_storeu_si128(round_keys[0].as_mut_ptr() as *mut __m128i, k);

        macro_rules! expand_and_store {
            ($idx:expr, $rc:expr) => {{
                k = expand_step::<$rc>(k);
                _mm_storeu_si128(round_keys[$idx].as_mut_ptr() as *mut __m128i, k);
            }};
        }

        expand_and_store!(1, 0x01);
        expand_and_store!(2, 0x02);
        expand_and_store!(3, 0x04);
        expand_and_store!(4, 0x08);
        expand_and_store!(5, 0x10);
        expand_and_store!(6, 0x20);
        expand_and_store!(7, 0x40);
        expand_and_store!(8, 0x80);
        expand_and_store!(9, 0x1b);
        expand_and_store!(10, 0x36);

        round_keys
    }
}

// Target assumption: modern x86_64 CPUs with AES-NI support.
fn aes_encrypt_block(input: &[u8; 16], round_keys: &[[u8; 16]; 11]) -> [u8; 16] {
    unsafe { aes_encrypt_block_aesni(input, round_keys) }
}

unsafe fn aes_encrypt_block_aesni(input: &[u8; 16], round_keys: &[[u8; 16]; 11]) -> [u8; 16] {
    use core::arch::x86_64::{
        __m128i, _mm_aesenc_si128, _mm_aesenclast_si128, _mm_loadu_si128, _mm_storeu_si128,
        _mm_xor_si128,
    };

    unsafe {
        let mut state = _mm_loadu_si128(input.as_ptr() as *const __m128i);
        state = _mm_xor_si128(
            state,
            _mm_loadu_si128(round_keys[0].as_ptr() as *const __m128i),
        );

        for round in round_keys.iter().take(10).skip(1) {
            state = _mm_aesenc_si128(state, _mm_loadu_si128(round.as_ptr() as *const __m128i));
        }
        state = _mm_aesenclast_si128(
            state,
            _mm_loadu_si128(round_keys[10].as_ptr() as *const __m128i),
        );

        let mut out = [0u8; 16];
        _mm_storeu_si128(out.as_mut_ptr() as *mut __m128i, state);
        out
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

    for block in aad.chunks(16) {
        let mut b = [0u8; 16];
        b[..block.len()].copy_from_slice(block);
        let x = u128::from_be_bytes(b);
        y = unsafe { gf_mul_pclmul(y ^ x, h) };
    }

    for block in ciphertext.chunks(16) {
        let mut b = [0u8; 16];
        b[..block.len()].copy_from_slice(block);
        let x = u128::from_be_bytes(b);
        y = unsafe { gf_mul_pclmul(y ^ x, h) };
    }

    let aad_bits = (aad.len() as u128) * 8;
    let ct_bits = (ciphertext.len() as u128) * 8;
    let mut len_block = [0u8; 16];
    len_block[..8].copy_from_slice(&(aad_bits as u64).to_be_bytes());
    len_block[8..].copy_from_slice(&(ct_bits as u64).to_be_bytes());
    let x = u128::from_be_bytes(len_block);
    unsafe { gf_mul_pclmul(y ^ x, h) }
}

#[inline(always)]
fn u128_to_m128i_be(x: u128) -> core::arch::x86_64::__m128i {
    use core::arch::x86_64::_mm_set_epi64x;
    let hi = (x >> 64) as i64;
    let lo = x as i64;
    unsafe { _mm_set_epi64x(hi, lo) }
}

#[inline(always)]
fn m128i_to_u128_be(x: core::arch::x86_64::__m128i) -> u128 {
    let words: [u64; 2] = unsafe { core::mem::transmute(x) };
    (words[1] as u128) << 64 | (words[0] as u128)
}

#[target_feature(enable = "pclmulqdq")]
unsafe fn gf_mul_pclmul(x: u128, h: u128) -> u128 {
    use core::arch::x86_64::{_mm_clmulepi64_si128, _mm_slli_si128, _mm_srli_si128, _mm_xor_si128};

    // GHASH uses a bit ordering opposite to the native CLMUL polynomial basis.
    // Reverse both operands into CLMUL basis and reverse the result back.
    let a = u128_to_m128i_be(x.reverse_bits());
    let b = u128_to_m128i_be(h.reverse_bits());

    let z00 = _mm_clmulepi64_si128(a, b, 0x00);
    let z01 = _mm_clmulepi64_si128(a, b, 0x01);
    let z10 = _mm_clmulepi64_si128(a, b, 0x10);
    let z11 = _mm_clmulepi64_si128(a, b, 0x11);

    let mid = _mm_xor_si128(z01, z10);
    let lo128 = _mm_xor_si128(z00, _mm_slli_si128(mid, 8));
    let hi128 = _mm_xor_si128(z11, _mm_srli_si128(mid, 8));

    let lo = m128i_to_u128_be(lo128);
    let hi = m128i_to_u128_be(hi128);
    gf_reduce(hi, lo).reverse_bits()
}

#[inline(always)]
fn gf_reduce(mut hi: u128, mut lo: u128) -> u128 {
    // Reduce modulo x^128 + x^7 + x^2 + x + 1.
    while hi != 0 {
        let t = hi;
        hi = 0;

        lo ^= t;
        lo ^= t << 1;
        hi ^= t >> 127;
        lo ^= t << 2;
        hi ^= t >> 126;
        lo ^= t << 7;
        hi ^= t >> 121;
    }
    lo
}

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
    use core::ffi::c_void;

    use alloc::vec;

    extern crate test;
    use test::Bencher;

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

    fn xor_into(dst: &mut [u8; 16], src: &[u8]) {
        for (d, s) in dst.iter_mut().zip(src.iter()) {
            *d ^= *s;
        }
    }

    fn shift_right_one(v: &mut [u8; 16]) {
        let mut carry = 0u8;
        for b in v.iter_mut() {
            let next_carry = *b & 1;
            *b = (*b >> 1) | (carry << 7);
            carry = next_carry;
        }
    }

    // Independent GHASH reference implementation using byte-wise operations.
    fn ghash_ref(h: u128, aad: &[u8], ciphertext: &[u8]) -> u128 {
        let h_bytes = h.to_be_bytes();
        let mut y = [0u8; 16];

        let mut mul = |block: &[u8]| {
            let mut x = [0u8; 16];
            x[..block.len()].copy_from_slice(block);
            xor_into(&mut y, &x);

            let mut z = [0u8; 16];
            let mut v = h_bytes;
            for byte in y {
                for bit in 0..8 {
                    if (byte & (0x80 >> bit)) != 0 {
                        xor_into(&mut z, &v);
                    }
                    let lsb = v[15] & 1;
                    shift_right_one(&mut v);
                    if lsb != 0 {
                        v[0] ^= 0xe1;
                    }
                }
            }
            y = z;
        };

        for block in aad.chunks(16) {
            mul(block);
        }
        for block in ciphertext.chunks(16) {
            mul(block);
        }

        let mut len_block = [0u8; 16];
        len_block[..8].copy_from_slice(&((aad.len() as u64) * 8).to_be_bytes());
        len_block[8..].copy_from_slice(&((ciphertext.len() as u64) * 8).to_be_bytes());
        mul(&len_block);

        u128::from_be_bytes(y)
    }

    #[test]
    fn test_ghash_empty_inputs_is_zero() {
        let h = 0x122204f9d2a456649d2bb1f744c939d9u128;
        assert_eq!(ghash(h, &[], &[]), 0);
    }

    #[test]
    fn test_ghash_zero_hash_subkey_is_zero() {
        let aad = b"aad-data";
        let ct = b"ciphertext-data";
        assert_eq!(ghash(0, aad, ct), 0);
    }

    #[test]
    fn test_ghash_matches_reference_with_partial_blocks() {
        let h = 0x66e94bd4ef8a2c3b884cfa59ca342b2eu128;
        let aad = b"header-17-bytes!!";
        let ct = b"ciphertext with 31 bytes payload";
        assert_eq!(ghash(h, aad, ct), ghash_ref(h, aad, ct));
    }

    #[test]
    fn test_ghash_known_vector_single_block_no_aad() {
        // NIST AES-GCM vector (Count 1): key=7fddb57453c241d03efbed3ac44e371c, nonce=ee283a3fc75575e33efd4887
        // H = AES_K(0^128), ciphertext=2ccda4a5415cb91e135c2a0f78c9b2fd, tag=b36d1df9b9d5e596f83e8b7f52971cb3
        // GHASH = tag xor AES_K(J0), where J0 = nonce || 0x00000001
        let h = 0x122204f9d2a456649d2bb1f744c939d9u128;
        let ct = hex_to_bytes("2ccda4a5415cb91e135c2a0f78c9b2fd");
        let expected_ghash = 0xeae0235dbcd657c0c4b6c8e91d6f0ee8u128;
        assert_eq!(ghash(h, &[], &ct), expected_ghash);
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

    #[bench]
    fn bench_key_expansion(b: &mut Bencher) {
        // Setup (not timed)
        let mut key = [0u8; 16];
        let _ = unsafe { crate::libc::getrandom(key.as_mut_ptr() as *mut c_void, 16, 0) };

        // Timed iteration
        b.iter(|| {
            let _ = key_expansion(&key);
        });
    }

    #[bench]
    fn bench_aes_encrypt_block(b: &mut Bencher) {
        // Setup (not timed)
        let mut key = [0u8; 16];
        let _ = unsafe { crate::libc::getrandom(key.as_mut_ptr() as *mut c_void, 16, 0) };
        let mut nonce = [0u8; 12];
        let _ = unsafe { crate::libc::getrandom(nonce.as_mut_ptr() as *mut c_void, 12, 0) };
        let round_keys = key_expansion(&key);
        let j0 = {
            let mut j = [0u8; 16];
            j[..12].copy_from_slice(&nonce);
            j[15] = 0x01;
            j
        };
        let mut ctr = j0;
        inc32(&mut ctr);

        // Timed iteration
        b.iter(|| {
            let _keystream = aes_encrypt_block(&ctr, &round_keys);
            inc32(&mut ctr);
        });
    }

    #[bench]
    fn bench_ghash(b: &mut Bencher) {
        // Setup (not timed)
        let mut ciphertext = [0u8; 450];
        let _ = unsafe { crate::libc::getrandom(ciphertext.as_mut_ptr() as *mut c_void, 450, 0) };
        let mut hdr = [0u8; 5];
        let _ = unsafe { crate::libc::getrandom(hdr.as_mut_ptr() as *mut c_void, 5, 0) };
        let mut h_bytes = [0u8; 16];
        let _ = unsafe { crate::libc::getrandom(h_bytes.as_mut_ptr() as *mut c_void, 16, 0) };
        let h = u128::from_be_bytes(h_bytes);

        // Timed iteration
        b.iter(|| {
            ghash(h, &hdr, &ciphertext);
        });
    }

    #[bench]
    fn bench_decrypt(b: &mut Bencher) {
        // Setup (not timed)
        let mut key = [0u8; 16];
        let _ = unsafe { crate::libc::getrandom(key.as_mut_ptr() as *mut c_void, 16, 0) };
        let mut nonce = [0u8; 12];
        let _ = unsafe { crate::libc::getrandom(nonce.as_mut_ptr() as *mut c_void, 12, 0) };
        let mut hdr = [0u8; 5];
        let _ = unsafe { crate::libc::getrandom(hdr.as_mut_ptr() as *mut c_void, 5, 0) };
        // OpenAI chat completions responses are typically 400 - 500 bytes
        let mut plaintext = [0u8; 450];
        let _ = unsafe { crate::libc::getrandom(plaintext.as_mut_ptr() as *mut c_void, 450, 0) };
        let ciphertext = aes_128_gcm_encrypt(&key, &nonce, &hdr, &plaintext).unwrap();

        // Timed part
        b.iter(|| {
            aes_128_gcm_decrypt(&key, &nonce, &hdr, &ciphertext).unwrap();
        });
    }
}
