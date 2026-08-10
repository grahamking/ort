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
//! Updated to Radix-51 field arithmetic inspired by curve25519-dalek by GPT-5.6 Sol.

type Fe = [u64; 5];

const MASK51: u64 = (1u64 << 51) - 1;
const P2_0: u64 = 2 * ((1u64 << 51) - 19);
const P2_I: u64 = 2 * MASK51;
const BASEPOINT: [u8; 32] = [
    9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const A24: Fe = [121665, 0, 0, 0, 0];

pub fn x25519_public_key(private: &[u8]) -> [u8; 32] {
    debug_assert!(private.len() >= 32, "private key must be 32 bytes");
    let scalar = unsafe { private.as_ptr().cast::<[u8; 32]>().read_unaligned() };
    x25519(&scalar, &BASEPOINT)
}

pub fn x25519_agreement(private_key: &[u8; 32], peer_public_key: &[u8; 32]) -> [u8; 32] {
    x25519(private_key, peer_public_key)
}

fn x25519(scalar: &[u8; 32], point: &[u8; 32]) -> [u8; 32] {
    let mut e = *scalar;
    e[0] &= 248;
    e[31] = (e[31] & 127) | 64;

    let x1 = fe_from_bytes(point);
    let mut x2 = [1, 0, 0, 0, 0];
    let mut z2 = [0; 5];
    let mut x3 = x1;
    let mut z3 = [1, 0, 0, 0, 0];
    let mut swap = 0;

    for pos in (0..255).rev() {
        let bit = ((e[pos >> 3] >> (pos & 7)) & 1) as u64;
        swap ^= bit;
        fe_cswap(&mut x2, &mut x3, swap);
        fe_cswap(&mut z2, &mut z3, swap);
        swap = bit;

        let a = fe_add(&x2, &z2);
        let aa = fe_square_n(&a, 1);
        let b = fe_sub(&x2, &z2);
        let bb = fe_square_n(&b, 1);
        let difference = fe_sub(&aa, &bb);
        let c = fe_add(&x3, &z3);
        let d = fe_sub(&x3, &z3);
        let da = fe_mul(&d, &a);
        let cb = fe_mul(&c, &b);
        x3 = fe_square_n(&fe_add(&da, &cb), 1);
        z3 = fe_mul(&x1, &fe_square_n(&fe_sub(&da, &cb), 1));
        x2 = fe_mul(&aa, &bb);
        z2 = fe_mul(&difference, &fe_add(&aa, &fe_mul(&difference, &A24)));
    }

    // The clamped scalar's low three bits are zero, so `swap` is zero here.
    fe_to_bytes(fe_mul(&x2, &fe_invert(&z2)))
}

fn fe_add(a: &Fe, b: &Fe) -> Fe {
    [
        a[0] + b[0],
        a[1] + b[1],
        a[2] + b[2],
        a[3] + b[3],
        a[4] + b[4],
    ]
}

#[inline(never)]
fn fe_sub(a: &Fe, b: &Fe) -> Fe {
    [
        a[0] + P2_0 - b[0],
        a[1] + P2_I - b[1],
        a[2] + P2_I - b[2],
        a[3] + P2_I - b[3],
        a[4] + P2_I - b[4],
    ]
}

fn fe_reduce(mut h: Fe) -> Fe {
    let c0 = h[0] >> 51;
    let c1 = h[1] >> 51;
    let c2 = h[2] >> 51;
    let c3 = h[3] >> 51;
    let c4 = h[4] >> 51;

    h[0] = (h[0] & MASK51) + 19 * c4;
    h[1] = (h[1] & MASK51) + c0;
    h[2] = (h[2] & MASK51) + c1;
    h[3] = (h[3] & MASK51) + c2;
    h[4] = (h[4] & MASK51) + c3;
    h
}

#[inline(never)]
fn fe_mul(a: &Fe, b: &Fe) -> Fe {
    let b1_19 = b[1] * 19;
    let b2_19 = b[2] * 19;
    let b3_19 = b[3] * 19;
    let b4_19 = b[4] * 19;

    let mut c0 = m(a[0], b[0]) + m(a[4], b1_19) + m(a[3], b2_19) + m(a[2], b3_19) + m(a[1], b4_19);
    let mut c1 = m(a[1], b[0]) + m(a[0], b[1]) + m(a[4], b2_19) + m(a[3], b3_19) + m(a[2], b4_19);
    let mut c2 = m(a[2], b[0]) + m(a[1], b[1]) + m(a[0], b[2]) + m(a[4], b3_19) + m(a[3], b4_19);
    let mut c3 = m(a[3], b[0]) + m(a[2], b[1]) + m(a[1], b[2]) + m(a[0], b[3]) + m(a[4], b4_19);
    let mut c4 = m(a[4], b[0]) + m(a[3], b[1]) + m(a[2], b[2]) + m(a[1], b[3]) + m(a[0], b[4]);

    c1 += (c0 >> 51) as u64 as u128;
    c0 &= MASK51 as u128;
    c2 += (c1 >> 51) as u64 as u128;
    c1 &= MASK51 as u128;
    c3 += (c2 >> 51) as u64 as u128;
    c2 &= MASK51 as u128;
    c4 += (c3 >> 51) as u64 as u128;
    c3 &= MASK51 as u128;

    let carry = (c4 >> 51) as u64;
    let mut out = [
        c0 as u64 + carry * 19,
        c1 as u64,
        c2 as u64,
        c3 as u64,
        c4 as u64 & MASK51,
    ];
    out[1] += out[0] >> 51;
    out[0] &= MASK51;
    out
}

#[inline(never)]
fn fe_square_n(x: &Fe, mut n: u32) -> Fe {
    let mut a = *x;
    while n != 0 {
        let a3_19 = a[3] * 19;
        let a4_19 = a[4] * 19;
        let mut c0 = m(a[0], a[0]) + 2 * (m(a[1], a4_19) + m(a[2], a3_19));
        let mut c1 = m(a[3], a3_19) + 2 * (m(a[0], a[1]) + m(a[2], a4_19));
        let mut c2 = m(a[1], a[1]) + 2 * (m(a[0], a[2]) + m(a[4], a3_19));
        let mut c3 = m(a[4], a4_19) + 2 * (m(a[0], a[3]) + m(a[1], a[2]));
        let mut c4 = m(a[2], a[2]) + 2 * (m(a[0], a[4]) + m(a[1], a[3]));

        c1 += (c0 >> 51) as u64 as u128;
        c0 &= MASK51 as u128;
        c2 += (c1 >> 51) as u64 as u128;
        c1 &= MASK51 as u128;
        c3 += (c2 >> 51) as u64 as u128;
        c2 &= MASK51 as u128;
        c4 += (c3 >> 51) as u64 as u128;
        c3 &= MASK51 as u128;

        let carry = (c4 >> 51) as u64;
        a = [
            c0 as u64 + carry * 19,
            c1 as u64,
            c2 as u64,
            c3 as u64,
            c4 as u64 & MASK51,
        ];
        a[1] += a[0] >> 51;
        a[0] &= MASK51;
        n -= 1;
    }
    a
}

fn m(a: u64, b: u64) -> u128 {
    a as u128 * b as u128
}

fn fe_invert(z: &Fe) -> Fe {
    let z2 = fe_square_n(z, 1);
    let z3 = fe_mul(&z2, z);
    let z4 = fe_square_n(&z2, 1);
    let z7 = fe_mul(&z4, &z3);
    let z8 = fe_square_n(&z4, 1);
    let z11 = fe_mul(&z8, &z3);
    let z14 = fe_square_n(&z7, 1);
    let z15 = fe_mul(&z14, z);

    // p - 2 = 0x7fff...ffeb: append 61 f-nibbles, then e and b.
    let mut c = z7;
    for _ in 0..61 {
        c = fe_mul(&fe_square_n(&c, 4), &z15);
    }
    c = fe_mul(&fe_square_n(&c, 4), &z14);
    fe_mul(&fe_square_n(&c, 4), &z11)
}

fn fe_cswap(a: &mut Fe, b: &mut Fe, swap: u64) {
    let mask = 0u64.wrapping_sub(swap);
    for i in 0..5 {
        let t = mask & (a[i] ^ b[i]);
        a[i] ^= t;
        b[i] ^= t;
    }
}

fn fe_from_bytes(bytes: &[u8; 32]) -> Fe {
    fn load(bytes: &[u8; 32], offset: usize) -> u64 {
        // SAFETY: every fixed offset below leaves eight bytes in the array.
        u64::from_le(unsafe { bytes.as_ptr().add(offset).cast::<u64>().read_unaligned() })
    }
    [
        load(bytes, 0) & MASK51,
        (load(bytes, 6) >> 3) & MASK51,
        (load(bytes, 12) >> 6) & MASK51,
        (load(bytes, 19) >> 1) & MASK51,
        (load(bytes, 24) >> 12) & MASK51,
    ]
}

fn fe_to_bytes(f: Fe) -> [u8; 32] {
    let mut h = fe_reduce(f);
    let mut q = (h[0] + 19) >> 51;
    q = (h[1] + q) >> 51;
    q = (h[2] + q) >> 51;
    q = (h[3] + q) >> 51;
    q = (h[4] + q) >> 51;
    h[0] += 19 * q;
    for i in 0..4 {
        h[i + 1] += h[i] >> 51;
        h[i] &= MASK51;
    }
    h[4] &= MASK51;

    let words = [
        h[0] | (h[1] << 51),
        (h[1] >> 13) | (h[2] << 38),
        (h[2] >> 26) | (h[3] << 25),
        (h[3] >> 39) | (h[4] << 12),
    ];
    let mut out = [0; 32];
    for (i, word) in words.into_iter().enumerate() {
        // SAFETY: i is in 0..4, writes are unaligned and stay within out.
        unsafe {
            out.as_mut_ptr()
                .add(8 * i)
                .cast::<u64>()
                .write_unaligned(word.to_le());
        }
    }
    out
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
