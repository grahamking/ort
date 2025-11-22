//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//
//! SHA-256 digest

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

#[cfg(test)]
mod tests {
    use crate::tls::tests::string_to_bytes;

    #[test]
    fn sha256_empty() {
        let output = super::sha256(b"");
        assert_eq!(
            output,
            string_to_bytes("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
    }

    /// Shorter than block size
    #[test]
    fn sha256_short() {
        let input = "Hello";
        let output = super::sha256(input.as_bytes());
        let expected =
            string_to_bytes("185f8db32271fe25f561a6fc938b2e264306ec304eda518007d1764826381969");
        assert_eq!(output, expected);
    }

    /// Longer than block size
    #[test]
    fn sha256_long() {
        let input = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.";
        let output = super::sha256(input.as_bytes());
        let expected =
            string_to_bytes("1c81c608a616183cc4a38c09ecc944eb77eaff465dd87aae0290177f2b70b6f8");
        assert_eq!(output, expected);
    }
}
