//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//
//! public / private key generation X25519 (ECDH using Curve25519)
//! as described in RFC 7748.
//! Kindly contributed by GPT-5

type GF = [i64; 16];

const GF_ZERO: GF = [0; 16];
const GF_ONE: GF = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const GF_121665: GF = [0xDB41, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

fn car25519(o: &mut GF) {
    for i in 0..16 {
        o[i] += 1 << 16;
        let c = o[i] >> 16;
        if i < 15 {
            o[i + 1] += c - 1;
        } else {
            o[0] += (c - 1) * 38;
        }
        o[i] -= c << 16;
    }
}

fn sel25519(p: &mut GF, q: &mut GF, b: i64) {
    let c = !(b - 1);
    for i in 0..16 {
        let t = c & (p[i] ^ q[i]);
        p[i] ^= t;
        q[i] ^= t;
    }
}

fn pack25519(o: &mut [u8; 32], n: &GF) {
    let mut t = *n;
    car25519(&mut t);
    car25519(&mut t);
    car25519(&mut t);

    for _ in 0..2 {
        let mut m = GF_ZERO;
        m[0] = t[0] - 0xffed;
        for i in 1..15 {
            m[i] = t[i] - 0xffff - ((m[i - 1] >> 16) & 1);
            m[i - 1] &= 0xffff;
        }
        m[15] = t[15] - 0x7fff - ((m[14] >> 16) & 1);
        let b = (m[15] >> 16) & 1;
        m[14] &= 0xffff;
        sel25519(&mut t, &mut m, 1 - b);
    }

    for i in 0..16 {
        o[2 * i] = (t[i] & 0xff) as u8;
        o[2 * i + 1] = ((t[i] >> 8) & 0xff) as u8;
    }
}

fn unpack25519(o: &mut GF, n: &[u8; 32]) {
    for i in 0..16 {
        o[i] = n[2 * i] as i64 + ((n[2 * i + 1] as i64) << 8);
    }
    o[15] &= 0x7fff;
}

fn add(a: &GF, b: &GF) -> GF {
    let mut o = GF_ZERO;
    for i in 0..16 {
        o[i] = a[i] + b[i];
    }
    o
}

fn sub(a: &GF, b: &GF) -> GF {
    let mut o = GF_ZERO;
    for i in 0..16 {
        o[i] = a[i] - b[i];
    }
    o
}

fn mul(a: &GF, b: &GF) -> GF {
    let a_vals = *a;
    let b_vals = *b;
    let mut t = [0i128; 31];

    for i in 0..16 {
        for j in 0..16 {
            t[i + j] += (a_vals[i] as i128) * (b_vals[j] as i128);
        }
    }

    for i in 0..15 {
        t[i] += 38 * t[i + 16];
    }

    let mut o = GF_ZERO;
    for i in 0..16 {
        o[i] = t[i] as i64;
    }

    car25519(&mut o);
    car25519(&mut o);
    o
}

fn square(a: &GF) -> GF {
    mul(a, a)
}

fn inv25519(i: &GF) -> GF {
    let mut c = *i;
    for a in (0..=253).rev() {
        c = square(&c);
        if a != 2 && a != 4 {
            c = mul(&c, i);
        }
    }
    c
}

fn crypto_scalarmult(q: &mut [u8; 32], n: &[u8; 32], p: &[u8; 32]) {
    let mut z = *n;
    z[0] &= 248;
    z[31] &= 127;
    z[31] |= 64;

    let mut base = GF_ZERO;
    unpack25519(&mut base, p);

    let mut a = GF_ONE;
    let mut b = base;
    let mut c = GF_ZERO;
    let mut d = GF_ONE;
    let mut e;
    let mut f;

    for i in (0..255).rev() {
        let bit = ((z[i >> 3] >> (i & 7)) & 1) as i64;
        sel25519(&mut a, &mut b, bit);
        sel25519(&mut c, &mut d, bit);
        e = add(&a, &c);
        a = sub(&a, &c);
        c = add(&b, &d);
        b = sub(&b, &d);
        d = square(&e);
        f = square(&a);
        a = mul(&c, &a);
        c = mul(&b, &e);
        e = add(&a, &c);
        a = sub(&a, &c);
        b = square(&a);
        c = sub(&d, &f);
        a = mul(&c, &GF_121665);
        a = add(&a, &d);
        c = mul(&c, &a);
        a = mul(&d, &f);
        d = mul(&b, &base);
        b = square(&e);
        sel25519(&mut a, &mut b, bit);
        sel25519(&mut c, &mut d, bit);
    }

    c = inv25519(&c);
    a = mul(&a, &c);
    pack25519(q, &a);
}

pub fn x25519_public_key(private: &[u8]) -> [u8; 32] {
    assert!(private.len() >= 32, "private key must be 32 bytes");
    let u = 9;

    let mut scalar = [0u8; 32];
    scalar.copy_from_slice(&private[..32]);

    let mut point = [0u8; 32];
    let mut value = u as u32;
    #[allow(clippy::needless_range_loop)]
    for i in 0..4 {
        point[i] = (value & 0xff) as u8;
        value >>= 8;
    }

    let mut out = [0u8; 32];
    crypto_scalarmult(&mut out, &scalar, &point);
    out
}

pub fn x25519_agreement(private_key: &[u8; 32], peer_public_key: &[u8; 32]) -> [u8; 32] {
    let mut shared = [0u8; 32];
    crypto_scalarmult(&mut shared, private_key, peer_public_key);
    shared
}

#[test]
fn test_alice() {
    // 77076d0a7318a57d
    // 3c16c17251b26645
    // df4c2f87ebc0992a
    // b177fba51db92c2a
    let private: [u8; 32] = [
        0x77, 0x07, 0x6d, 0x0a, 0x73, 0x18, 0xa5, 0x7d, 0x3c, 0x16, 0xc1, 0x72, 0x51, 0xb2, 0x66,
        0x45, 0xdf, 0x4c, 0x2f, 0x87, 0xeb, 0xc0, 0x99, 0x2a, 0xb1, 0x77, 0xfb, 0xa5, 0x1d, 0xb9,
        0x2c, 0x2a,
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
        0x5d, 0xab, 0x08, 0x7e, 0x62, 0x4a, 0x8a, 0x4b, 0x79, 0xe1, 0x7f, 0x8b, 0x83, 0x80, 0x0e,
        0xe6, 0x6f, 0x3b, 0xb1, 0x29, 0x26, 0x18, 0xb6, 0xfd, 0x1c, 0x2f, 0x8b, 0x27, 0xff, 0x88,
        0xe0, 0xeb,
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
        0xd2, 0x1a, 0x4d, 0xe6, 0x61, 0x4f, 0xbc, 0x2a, 0x90, 0x4b, 0x29, 0x48, 0x9d, 0xb4, 0xc1,
        0x59, 0x00, 0xb6, 0x7b, 0x6d, 0xda, 0xd2, 0x50, 0xe1, 0xf9, 0xcf, 0x43, 0x69, 0xaa, 0x6c,
        0x2b, 0x3b,
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

#[cfg(test)]
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
