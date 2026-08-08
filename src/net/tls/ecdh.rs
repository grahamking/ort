//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025, 2026 Graham King
//
//! Public / private key generation X25519 (ECDH using Curve25519)
//! as described in RFC 7748.
//!
//! Originally contributed by GPT-5.
//! Re-written to specialized SIMD friendly code by GPT-5.5 xhigh.

type Fe = [u64; 5];

const MASK51: u64 = (1u64 << 51) - 1;
const MASK51_U128: u128 = MASK51 as u128;
const TWO_P0: u64 = 2 * ((1u64 << 51) - 19);
const TWO_PI: u64 = 2 * MASK51;
const BASEPOINT: [u8; 32] = [
    9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

pub fn x25519_public_key(private: &[u8]) -> [u8; 32] {
    assert!(private.len() >= 32, "private key must be 32 bytes");
    let mut scalar = [0u8; 32];
    scalar.copy_from_slice(&private[..32]);
    x25519(&scalar, &BASEPOINT)
}

pub fn x25519_agreement(private_key: &[u8; 32], peer_public_key: &[u8; 32]) -> [u8; 32] {
    x25519(private_key, peer_public_key)
}

fn x25519(scalar: &[u8; 32], point: &[u8; 32]) -> [u8; 32] {
    let mut e = *scalar;
    e[0] &= 248;
    e[31] &= 127;
    e[31] |= 64;

    let x1 = fe_from_bytes(point);
    let mut x2 = [1, 0, 0, 0, 0];
    let mut z2 = [0; 5];
    let mut x3 = x1;
    let mut z3 = [1, 0, 0, 0, 0];
    let mut swap = 0u64;

    for pos in (0..255).rev() {
        let bit = ((e[pos >> 3] >> (pos & 7)) & 1) as u64;
        swap ^= bit;
        fe_cswap(&mut x2, &mut x3, swap);
        fe_cswap(&mut z2, &mut z3, swap);
        swap = bit;

        let a = fe_add(&x2, &z2);
        let aa = fe_square(&a);
        let b = fe_sub(&x2, &z2);
        let bb = fe_square(&b);
        let e = fe_sub(&aa, &bb);
        let c = fe_add(&x3, &z3);
        let d = fe_sub(&x3, &z3);
        let da = fe_mul(&d, &a);
        let cb = fe_mul(&c, &b);
        let da_plus_cb = fe_add(&da, &cb);
        let da_minus_cb = fe_sub(&da, &cb);
        x3 = fe_square(&da_plus_cb);
        z3 = fe_mul(&x1, &fe_square(&da_minus_cb));
        x2 = fe_mul(&aa, &bb);
        z2 = fe_mul(&e, &fe_add(&aa, &fe_mul_const(&e, 121665)));
    }

    fe_cswap(&mut x2, &mut x3, swap);
    fe_cswap(&mut z2, &mut z3, swap);

    fe_to_bytes(&fe_mul(&x2, &fe_invert(&z2)))
}

#[inline(always)]
fn fe_from_bytes(bytes: &[u8; 32]) -> Fe {
    let mut bytes = *bytes;
    bytes[31] &= 127;

    [
        load64(&bytes, 0) & MASK51,
        (load64(&bytes, 6) >> 3) & MASK51,
        (load64(&bytes, 12) >> 6) & MASK51,
        (load64(&bytes, 19) >> 1) & MASK51,
        (load64(&bytes, 24) >> 12) & MASK51,
    ]
}

#[inline(always)]
fn fe_to_bytes(f: &Fe) -> [u8; 32] {
    let f = fe_freeze(f);
    let mut out = [0; 32];

    store64(&mut out, 0, f[0] | (f[1] << 51));
    store64(&mut out, 8, (f[1] >> 13) | (f[2] << 38));
    store64(&mut out, 16, (f[2] >> 26) | (f[3] << 25));
    store64(&mut out, 24, (f[3] >> 39) | (f[4] << 12));

    out
}

#[inline(always)]
fn load64(bytes: &[u8; 32], offset: usize) -> u64 {
    let mut word = [0; 8];
    word.copy_from_slice(&bytes[offset..offset + 8]);
    u64::from_le_bytes(word)
}

#[inline(always)]
fn store64(out: &mut [u8; 32], offset: usize, value: u64) {
    out[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[inline(always)]
fn fe_add(a: &Fe, b: &Fe) -> Fe {
    carry_reduce([
        a[0] as u128 + b[0] as u128,
        a[1] as u128 + b[1] as u128,
        a[2] as u128 + b[2] as u128,
        a[3] as u128 + b[3] as u128,
        a[4] as u128 + b[4] as u128,
    ])
}

#[inline(always)]
fn fe_sub(a: &Fe, b: &Fe) -> Fe {
    carry_reduce([
        a[0] as u128 + TWO_P0 as u128 - b[0] as u128,
        a[1] as u128 + TWO_PI as u128 - b[1] as u128,
        a[2] as u128 + TWO_PI as u128 - b[2] as u128,
        a[3] as u128 + TWO_PI as u128 - b[3] as u128,
        a[4] as u128 + TWO_PI as u128 - b[4] as u128,
    ])
}

#[inline(always)]
fn fe_mul(a: &Fe, b: &Fe) -> Fe {
    let a0 = a[0] as u128;
    let a1 = a[1] as u128;
    let a2 = a[2] as u128;
    let a3 = a[3] as u128;
    let a4 = a[4] as u128;
    let b0 = b[0] as u128;
    let b1 = b[1] as u128;
    let b2 = b[2] as u128;
    let b3 = b[3] as u128;
    let b4 = b[4] as u128;

    carry_reduce([
        a0 * b0 + 19 * (a1 * b4 + a2 * b3 + a3 * b2 + a4 * b1),
        a0 * b1 + a1 * b0 + 19 * (a2 * b4 + a3 * b3 + a4 * b2),
        a0 * b2 + a1 * b1 + a2 * b0 + 19 * (a3 * b4 + a4 * b3),
        a0 * b3 + a1 * b2 + a2 * b1 + a3 * b0 + 19 * a4 * b4,
        a0 * b4 + a1 * b3 + a2 * b2 + a3 * b1 + a4 * b0,
    ])
}

#[inline(always)]
fn fe_square(a: &Fe) -> Fe {
    let a0 = a[0] as u128;
    let a1 = a[1] as u128;
    let a2 = a[2] as u128;
    let a3 = a[3] as u128;
    let a4 = a[4] as u128;

    carry_reduce([
        a0 * a0 + 38 * (a1 * a4 + a2 * a3),
        2 * a0 * a1 + 19 * (2 * a2 * a4 + a3 * a3),
        2 * a0 * a2 + a1 * a1 + 38 * a3 * a4,
        2 * a0 * a3 + 2 * a1 * a2 + 19 * a4 * a4,
        2 * a0 * a4 + 2 * a1 * a3 + a2 * a2,
    ])
}

#[inline(always)]
fn fe_mul_const(a: &Fe, c: u64) -> Fe {
    let c = c as u128;

    carry_reduce([
        a[0] as u128 * c,
        a[1] as u128 * c,
        a[2] as u128 * c,
        a[3] as u128 * c,
        a[4] as u128 * c,
    ])
}

#[inline(always)]
fn carry_reduce(mut h: [u128; 5]) -> Fe {
    carry_round(&mut h);
    carry_round(&mut h);

    [
        h[0] as u64,
        h[1] as u64,
        h[2] as u64,
        h[3] as u64,
        h[4] as u64,
    ]
}

#[inline(always)]
fn carry_round(h: &mut [u128; 5]) {
    let c0 = h[0] >> 51;
    h[0] &= MASK51_U128;
    h[1] += c0;

    let c1 = h[1] >> 51;
    h[1] &= MASK51_U128;
    h[2] += c1;

    let c2 = h[2] >> 51;
    h[2] &= MASK51_U128;
    h[3] += c2;

    let c3 = h[3] >> 51;
    h[3] &= MASK51_U128;
    h[4] += c3;

    let c4 = h[4] >> 51;
    h[4] &= MASK51_U128;
    h[0] += 19 * c4;
}

#[inline(always)]
fn fe_freeze(f: &Fe) -> Fe {
    let mut h = [
        f[0] as u128,
        f[1] as u128,
        f[2] as u128,
        f[3] as u128,
        f[4] as u128,
    ];

    carry_round(&mut h);
    carry_round(&mut h);
    carry_round(&mut h);
    carry_round(&mut h);

    let reduced = [
        h[0] as u64,
        h[1] as u64,
        h[2] as u64,
        h[3] as u64,
        h[4] as u64,
    ];

    let mut wrapped = reduced;
    wrapped[0] += 19;
    let carry0 = wrapped[0] >> 51;
    wrapped[0] &= MASK51;
    wrapped[1] += carry0;
    let carry1 = wrapped[1] >> 51;
    wrapped[1] &= MASK51;
    wrapped[2] += carry1;
    let carry2 = wrapped[2] >> 51;
    wrapped[2] &= MASK51;
    wrapped[3] += carry2;
    let carry3 = wrapped[3] >> 51;
    wrapped[3] &= MASK51;
    wrapped[4] += carry3;
    let carry4 = wrapped[4] >> 51;
    wrapped[4] &= MASK51;

    let mask = 0u64.wrapping_sub(carry4);
    [
        (reduced[0] & !mask) | (wrapped[0] & mask),
        (reduced[1] & !mask) | (wrapped[1] & mask),
        (reduced[2] & !mask) | (wrapped[2] & mask),
        (reduced[3] & !mask) | (wrapped[3] & mask),
        (reduced[4] & !mask) | (wrapped[4] & mask),
    ]
}

fn fe_invert(z: &Fe) -> Fe {
    let mut c = *z;
    for a in (0..=253).rev() {
        c = fe_square(&c);
        if a != 2 && a != 4 {
            c = fe_mul(&c, z);
        }
    }
    c
}

#[inline(always)]
fn fe_cswap(a: &mut Fe, b: &mut Fe, swap: u64) {
    let mask = 0u64.wrapping_sub(swap);
    for i in 0..5 {
        let t = mask & (a[i] ^ b[i]);
        a[i] ^= t;
        b[i] ^= t;
    }
}

#[cfg(test)]
mod test {
    use super::{x25519_agreement, x25519_public_key};

    #[test]
    fn test_alice() {
        // 77076d0a7318a57d
        // 3c16c17251b26645
        // df4c2f87ebc0992a
        // b177fba51db92c2a
        let private: [u8; 32] = [
            0x77, 0x07, 0x6d, 0x0a, 0x73, 0x18, 0xa5, 0x7d, 0x3c, 0x16, 0xc1, 0x72, 0x51, 0xb2,
            0x66, 0x45, 0xdf, 0x4c, 0x2f, 0x87, 0xeb, 0xc0, 0x99, 0x2a, 0xb1, 0x77, 0xfb, 0xa5,
            0x1d, 0xb9, 0x2c, 0x2a,
        ];
        let public = x25519_public_key(&private);

        // 8520f0098930a754
        // 748b7ddcb43ef75a
        // 0dbf3a0d26381af4
        // eba4a98eaa9b4e6a
        assert_eq!(
            public,
            [
                0x85, 0x20, 0xf0, 0x09, 0x89, 0x30, 0xa7, 0x54, 0x74, 0x8b, 0x7d, 0xdc, 0xb4, 0x3e,
                0xf7, 0x5a, 0x0d, 0xbf, 0x3a, 0x0d, 0x26, 0x38, 0x1a, 0xf4, 0xeb, 0xa4, 0xa9, 0x8e,
                0xaa, 0x9b, 0x4e, 0x6a,
            ]
        );
    }

    #[test]
    fn test_bob() {
        let private: [u8; 32] = [
            0x5d, 0xab, 0x08, 0x7e, 0x62, 0x4a, 0x8a, 0x4b, 0x79, 0xe1, 0x7f, 0x8b, 0x83, 0x80,
            0x0e, 0xe6, 0x6f, 0x3b, 0xb1, 0x29, 0x26, 0x18, 0xb6, 0xfd, 0x1c, 0x2f, 0x8b, 0x27,
            0xff, 0x88, 0xe0, 0xeb,
        ];
        let public = x25519_public_key(&private);
        assert_eq!(
            public,
            [
                0xde, 0x9e, 0xdb, 0x7d, 0x7b, 0x7d, 0xc1, 0xb4, 0xd3, 0x5b, 0x61, 0xc2, 0xec, 0xe4,
                0x35, 0x37, 0x3f, 0x83, 0x43, 0xc8, 0x5b, 0x78, 0x67, 0x4d, 0xad, 0xfc, 0x7e, 0x14,
                0x6f, 0x88, 0x2b, 0x4f,
            ]
        );
    }

    #[test]
    fn test_from_ring() {
        // d21a4de6614fbc2a
        // 904b29489db4c159
        // 00b67b6ddad250e1
        // f9cf4369aa6c2b3b
        let private = [
            0xd2, 0x1a, 0x4d, 0xe6, 0x61, 0x4f, 0xbc, 0x2a, 0x90, 0x4b, 0x29, 0x48, 0x9d, 0xb4,
            0xc1, 0x59, 0x00, 0xb6, 0x7b, 0x6d, 0xda, 0xd2, 0x50, 0xe1, 0xf9, 0xcf, 0x43, 0x69,
            0xaa, 0x6c, 0x2b, 0x3b,
        ];
        let public = x25519_public_key(&private);
        // 7a07c60f370f5a94a528a77d598153ac4b822aa4198965480cc0dfd7575d7329
        assert_eq!(
            public,
            [
                0x7a, 0x07, 0xc6, 0x0f, 0x37, 0x0f, 0x5a, 0x94, 0xa5, 0x28, 0xa7, 0x7d, 0x59, 0x81,
                0x53, 0xac, 0x4b, 0x82, 0x2a, 0xa4, 0x19, 0x89, 0x65, 0x48, 0x0c, 0xc0, 0xdf, 0xd7,
                0x57, 0x5d, 0x73, 0x29
            ]
        );
    }

    #[test]
    fn test_agreement_alice_bob() {
        let alice_private =
            string_to_bytes("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
        let bob_public =
            string_to_bytes("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f");
        let expected_shared_secret =
            string_to_bytes("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");

        let got_shared_secret = x25519_agreement(&alice_private, &bob_public);
        assert_eq!(expected_shared_secret, got_shared_secret);
    }

    #[test]
    fn test_agreement_from_ring() {
        let client_private_key =
            string_to_bytes("354436c2a2aacc8245e3a89b325a779ebf97cc61df5b85d1afa24fdd6006ff38");
        let server_public_key =
            string_to_bytes("d84ca3df6f987da964f6b34b10a2e3e07057e74e5503458b12246ebcae0fda59");
        let expected_shared_secret =
            string_to_bytes("d323f80c636d877a327d24b20a562bfaecf13a52baf80a2ed74102703c3ee778");

        let got_shared_secret = x25519_agreement(&client_private_key, &server_public_key);
        assert_eq!(expected_shared_secret, got_shared_secret);
    }

    fn string_to_bytes(s: &str) -> [u8; 32] {
        fn hex_val(b: u8) -> u8 {
            match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => panic!("invalid hex character"),
            }
        }

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
}
