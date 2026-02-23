//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//
//! SHA-256 digest

const INITIAL_STATE: [u32; 8] = [
    0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19,
];
const K32X4: [[u32; 4]; 16] = [
    [0xE9B5DBA5, 0xB5C0FBCF, 0x71374491, 0x428A2F98],
    [0xAB1C5ED5, 0x923F82A4, 0x59F111F1, 0x3956C25B],
    [0x550C7DC3, 0x243185BE, 0x12835B01, 0xD807AA98],
    [0xC19BF174, 0x9BDC06A7, 0x80DEB1FE, 0x72BE5D74],
    [0x240CA1CC, 0x0FC19DC6, 0xEFBE4786, 0xE49B69C1],
    [0x76F988DA, 0x5CB0A9DC, 0x4A7484AA, 0x2DE92C6F],
    [0xBF597FC7, 0xB00327C8, 0xA831C66D, 0x983E5152],
    [0x14292967, 0x06CA6351, 0xD5A79147, 0xC6E00BF3],
    [0x53380D13, 0x4D2C6DFC, 0x2E1B2138, 0x27B70A85],
    [0x92722C85, 0x81C2C92E, 0x766A0ABB, 0x650A7354],
    [0xC76C51A3, 0xC24B8B70, 0xA81A664B, 0xA2BFE8A1],
    [0x106AA070, 0xF40E3585, 0xD6990624, 0xD192E819],
    [0x34B0BCB5, 0x2748774C, 0x1E376C08, 0x19A4C116],
    [0x682E6FF3, 0x5B9CCA4F, 0x4ED8AA4A, 0x391C0CB3],
    [0x8CC70208, 0x84C87814, 0x78A5636F, 0x748F82EE],
    [0xC67178F2, 0xBEF9A3F7, 0xA4506CEB, 0x90BEFFFA],
];

use core::arch::x86_64::{
    __m128i, _mm_add_epi32, _mm_alignr_epi8, _mm_blend_epi16, _mm_loadu_si128, _mm_set_epi32,
    _mm_set_epi64x, _mm_sha256msg1_epu32, _mm_sha256msg2_epu32, _mm_sha256rnds2_epu32,
    _mm_shuffle_epi8, _mm_shuffle_epi32, _mm_storeu_si128,
};

#[allow(unsafe_op_in_unsafe_fn)]
#[target_feature(enable = "sha,sse2,ssse3,sse4.1")]
unsafe fn schedule(v0: __m128i, v1: __m128i, v2: __m128i, v3: __m128i) -> __m128i {
    let t1 = _mm_sha256msg1_epu32(v0, v1);
    let t2 = _mm_alignr_epi8(v3, v2, 4);
    let t3 = _mm_add_epi32(t1, t2);
    _mm_sha256msg2_epu32(t3, v3)
}

macro_rules! rounds4 {
    ($abef:ident, $cdgh:ident, $rest:expr, $i:expr) => {{
        let k = K32X4[$i];
        let kv = _mm_set_epi32(k[0] as i32, k[1] as i32, k[2] as i32, k[3] as i32);
        let t1 = _mm_add_epi32($rest, kv);
        $cdgh = _mm_sha256rnds2_epu32($cdgh, $abef, t1);
        let t2 = _mm_shuffle_epi32(t1, 0x0E);
        $abef = _mm_sha256rnds2_epu32($abef, $cdgh, t2);
    }};
}

macro_rules! schedule_rounds4 {
    ($abef:ident, $cdgh:ident, $w0:expr, $w1:expr, $w2:expr, $w3:expr, $w4:expr, $i:expr) => {{
        $w4 = schedule($w0, $w1, $w2, $w3);
        rounds4!($abef, $cdgh, $w4, $i);
    }};
}

#[allow(unsafe_op_in_unsafe_fn)]
#[target_feature(enable = "sha,sse2,ssse3,sse4.1")]
unsafe fn compress_blocks(h: &mut [u32; 8], blocks: &[[u8; 64]]) {
    let mask = _mm_set_epi64x(
        0x0C0D_0E0F_0809_0A0Bu64 as i64,
        0x0405_0607_0001_0203u64 as i64,
    );

    let state_ptr = h.as_ptr() as *const __m128i;
    let dcba = _mm_loadu_si128(state_ptr);
    let efgh = _mm_loadu_si128(state_ptr.add(1));

    let cdab = _mm_shuffle_epi32(dcba, 0xB1);
    let efgh = _mm_shuffle_epi32(efgh, 0x1B);
    let mut abef = _mm_alignr_epi8(cdab, efgh, 8);
    let mut cdgh = _mm_blend_epi16(efgh, cdab, 0xF0);

    for block in blocks {
        let abef_save = abef;
        let cdgh_save = cdgh;

        let data_ptr = block.as_ptr() as *const __m128i;
        let mut w0 = _mm_shuffle_epi8(_mm_loadu_si128(data_ptr), mask);
        let mut w1 = _mm_shuffle_epi8(_mm_loadu_si128(data_ptr.add(1)), mask);
        let mut w2 = _mm_shuffle_epi8(_mm_loadu_si128(data_ptr.add(2)), mask);
        let mut w3 = _mm_shuffle_epi8(_mm_loadu_si128(data_ptr.add(3)), mask);
        let mut w4;

        rounds4!(abef, cdgh, w0, 0);
        rounds4!(abef, cdgh, w1, 1);
        rounds4!(abef, cdgh, w2, 2);
        rounds4!(abef, cdgh, w3, 3);
        schedule_rounds4!(abef, cdgh, w0, w1, w2, w3, w4, 4);
        schedule_rounds4!(abef, cdgh, w1, w2, w3, w4, w0, 5);
        schedule_rounds4!(abef, cdgh, w2, w3, w4, w0, w1, 6);
        schedule_rounds4!(abef, cdgh, w3, w4, w0, w1, w2, 7);
        schedule_rounds4!(abef, cdgh, w4, w0, w1, w2, w3, 8);
        schedule_rounds4!(abef, cdgh, w0, w1, w2, w3, w4, 9);
        schedule_rounds4!(abef, cdgh, w1, w2, w3, w4, w0, 10);
        schedule_rounds4!(abef, cdgh, w2, w3, w4, w0, w1, 11);
        schedule_rounds4!(abef, cdgh, w3, w4, w0, w1, w2, 12);
        schedule_rounds4!(abef, cdgh, w4, w0, w1, w2, w3, 13);
        schedule_rounds4!(abef, cdgh, w0, w1, w2, w3, w4, 14);
        schedule_rounds4!(abef, cdgh, w1, w2, w3, w4, w0, 15);

        abef = _mm_add_epi32(abef, abef_save);
        cdgh = _mm_add_epi32(cdgh, cdgh_save);
    }

    let feba = _mm_shuffle_epi32(abef, 0x1B);
    let dchg = _mm_shuffle_epi32(cdgh, 0xB1);
    let dcba = _mm_blend_epi16(feba, dchg, 0xF0);
    let hgef = _mm_alignr_epi8(dchg, feba, 8);

    let state_ptr = h.as_mut_ptr() as *mut __m128i;
    _mm_storeu_si128(state_ptr, dcba);
    _mm_storeu_si128(state_ptr.add(1), hgef);
}

/// Calculate the SHA-256 digest of the input string.
pub fn sha256(b: &[u8]) -> [u8; 32] {
    let mut h = INITIAL_STATE;
    let bit_len = (b.len() as u64) * 8;

    let full_blocks = b.len() / 64;
    if full_blocks != 0 {
        let blocks =
            unsafe { core::slice::from_raw_parts(b.as_ptr() as *const [u8; 64], full_blocks) };
        unsafe { compress_blocks(&mut h, blocks) };
    }
    let remaining = &b[(full_blocks * 64)..];

    let rem_len = remaining.len();
    let mut tail = [0u8; 128];
    tail[..rem_len].copy_from_slice(remaining);
    tail[rem_len] = 0x80;
    if rem_len < 56 {
        tail[56..64].copy_from_slice(&bit_len.to_be_bytes());
    } else {
        tail[120..128].copy_from_slice(&bit_len.to_be_bytes());
    }
    let tail_blocks_count = if rem_len < 56 { 1 } else { 2 };
    let tail_blocks =
        unsafe { core::slice::from_raw_parts(tail.as_ptr() as *const [u8; 64], tail_blocks_count) };
    unsafe { compress_blocks(&mut h, tail_blocks) };

    let mut out = [0u8; 32];
    for (chunk, word) in out.chunks_exact_mut(4).zip(h.iter()) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use core::ffi::c_void;

    extern crate test;
    use test::Bencher;

    use crate::net::tls::tests::string_to_bytes;

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

    #[bench]
    fn bench_sha256_short(b: &mut Bencher) {
        let input = b"Hello";
        b.iter(|| {
            let _ = super::sha256(input);
        });
    }

    #[bench]
    fn bench_sha256_450_bytes(b: &mut Bencher) {
        // OpenAI chat completion responses are often a few hundred bytes.
        let mut input = [0u8; 450];
        let _ = unsafe { crate::libc::getrandom(input.as_mut_ptr() as *mut c_void, 450, 0) };

        b.iter(|| {
            let _ = super::sha256(&input);
        });
    }
}
