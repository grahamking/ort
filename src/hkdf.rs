//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//
//! HKDF - HMAC SHA-256 Key Derivation, RFC 5869

/// HKDF-Extract as defined in RFC 5869 using SHA-256.
pub fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    let zero_salt = [0u8; 32];
    let key = if salt.is_empty() {
        &zero_salt[..]
    } else {
        salt
    };
    hmac_sign(key, ikm)
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

        previous = hmac_sign(prk, &block_input);
        let remaining = len - okm.len();
        let take = remaining.min(HASH_LEN);
        okm.extend_from_slice(&previous[..take]);
    }

    okm
}

/// HMAC SHA-256
fn hmac_sign(key: &[u8], data: &[u8]) -> [u8; 32] {
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

/// Calculate the SHA-256 digest of the input string.
pub fn sha256(b: &[u8]) -> [u8; 32] {
    const INITIAL_STATE: [u32; 8] = [
        0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB,
        0x5BE0CD19,
    ];
    const K: [u32; 64] = [
        0x428A2F98, 0x71374491, 0xB5C0FBCF, 0xE9B5DBA5, 0x3956C25B, 0x59F111F1, 0x923F82A4,
        0xAB1C5ED5, 0xD807AA98, 0x12835B01, 0x243185BE, 0x550C7DC3, 0x72BE5D74, 0x80DEB1FE,
        0x9BDC06A7, 0xC19BF174, 0xE49B69C1, 0xEFBE4786, 0x0FC19DC6, 0x240CA1CC, 0x2DE92C6F,
        0x4A7484AA, 0x5CB0A9DC, 0x76F988DA, 0x983E5152, 0xA831C66D, 0xB00327C8, 0xBF597FC7,
        0xC6E00BF3, 0xD5A79147, 0x06CA6351, 0x14292967, 0x27B70A85, 0x2E1B2138, 0x4D2C6DFC,
        0x53380D13, 0x650A7354, 0x766A0ABB, 0x81C2C92E, 0x92722C85, 0xA2BFE8A1, 0xA81A664B,
        0xC24B8B70, 0xC76C51A3, 0xD192E819, 0xD6990624, 0xF40E3585, 0x106AA070, 0x19A4C116,
        0x1E376C08, 0x2748774C, 0x34B0BCB5, 0x391C0CB3, 0x4ED8AA4A, 0x5B9CCA4F, 0x682E6FF3,
        0x748F82EE, 0x78A5636F, 0x84C87814, 0x8CC70208, 0x90BEFFFA, 0xA4506CEB, 0xBEF9A3F7,
        0xC67178F2,
    ];

    #[inline(always)]
    fn rotr(x: u32, n: u32) -> u32 {
        (x >> n) | (x << (32 - n))
    }

    #[inline(always)]
    fn ch(x: u32, y: u32, z: u32) -> u32 {
        (x & y) ^ (!x & z)
    }

    #[inline(always)]
    fn maj(x: u32, y: u32, z: u32) -> u32 {
        (x & y) ^ (x & z) ^ (y & z)
    }

    #[inline(always)]
    fn big_sigma0(x: u32) -> u32 {
        rotr(x, 2) ^ rotr(x, 13) ^ rotr(x, 22)
    }

    #[inline(always)]
    fn big_sigma1(x: u32) -> u32 {
        rotr(x, 6) ^ rotr(x, 11) ^ rotr(x, 25)
    }

    #[inline(always)]
    fn small_sigma0(x: u32) -> u32 {
        rotr(x, 7) ^ rotr(x, 18) ^ (x >> 3)
    }

    #[inline(always)]
    fn small_sigma1(x: u32) -> u32 {
        rotr(x, 17) ^ rotr(x, 19) ^ (x >> 10)
    }

    let bit_len = (b.len() as u64) * 8;
    //let mut data = Vec::with_capacity(((b.len() + 9 + 63) / 64) * 64);
    let mut data = Vec::with_capacity((b.len() + 9).div_ceil(64));
    data.extend_from_slice(b);
    data.push(0x80);
    while (data.len() % 64) != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());

    let mut h = INITIAL_STATE;
    let mut w = [0u32; 64];

    for chunk in data.chunks_exact(64) {
        for (i, word_bytes) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word_bytes[0], word_bytes[1], word_bytes[2], word_bytes[3]]);
        }
        for i in 16..64 {
            let s0 = small_sigma0(w[i - 15]);
            let s1 = small_sigma1(w[i - 2]);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b_ = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut h_ = h[7];

        for i in 0..64 {
            let t1 = h_
                .wrapping_add(big_sigma1(e))
                .wrapping_add(ch(e, f, g))
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let t2 = big_sigma0(a).wrapping_add(maj(a, b_, c));

            h_ = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b_;
            b_ = a;
            a = t1.wrapping_add(t2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b_);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(h_);
    }

    let mut out = [0u8; 32];
    for (chunk, word) in out.chunks_exact_mut(4).zip(h.iter()) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Shorter than block size
#[test]
fn test_sha256_short() {
    let input = "Hello";
    let output = sha256(input.as_bytes());
    let expected =
        string_to_bytes("185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969");
    assert_eq!(output, expected);
}

/// Longer than block size
#[test]
fn test_sha256_long() {
    let input = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.";
    let output = sha256(input.as_bytes());
    let expected =
        string_to_bytes("1c81c608a616183cc4a38c09ecc944eb77eaff465dd87aae0290177f2b70b6f8");
    assert_eq!(output, expected);
}

#[test]
fn test_hmac_sha256_short() {
    let key = "secret";
    let data = "Hello";
    let output = hmac_sign(key.as_bytes(), data.as_bytes());
    let expected =
        string_to_bytes("0cc692f2177b42b6e5cd82488ee6c5d526a007c571e7de1fec07c1e2b1dfa2e2");
    assert_eq!(output, expected);
}

#[test]
fn test_hmac_sha256_long() {
    let key = "secret";
    let data = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.";
    let output = hmac_sign(key.as_bytes(), data.as_bytes());
    let expected =
        string_to_bytes("602a9c4d44feea742c6775c21d686ccd899ee4c8363d7c03535b949c16a6b6d8");
    assert_eq!(output, expected);
}

#[test]
fn test_hkdf_extract_rfc5869_case1() {
    let ikm = [0x0bu8; 22];
    let salt = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
    ];
    let expected =
        string_to_bytes("077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5");
    let prk = hkdf_extract(&salt, &ikm);
    assert_eq!(prk, expected);
}

#[test]
fn test_hkdf_expand_rfc5869_case1() {
    let prk = string_to_bytes("077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5");
    let info = [0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9];
    let okm = hkdf_expand(&prk, &info, 42);
    let expected = hex_to_vec(
        "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865",
    );
    assert_eq!(okm, expected);
}

#[test]
fn test_hkdf_extract_empty_salt() {
    let ikm = b"handshake secret";
    let prk = hkdf_extract(&[], ikm);
    let zero_salt = [0u8; 32];
    let expected = hmac_sign(&zero_salt, ikm);
    assert_eq!(prk, expected);
}

#[test]
fn test_hkdf_expand_zero_len() {
    let prk = [0xabu8; 32];
    let okm = hkdf_expand(&prk, b"", 0);
    assert!(okm.is_empty());
}

#[cfg(test)]
fn string_to_bytes(s: &str) -> [u8; 32] {
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

#[cfg(test)]
fn hex_to_vec(s: &str) -> Vec<u8> {
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

#[cfg(test)]
fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => panic!("invalid hex character"),
    }
}
