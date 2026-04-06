//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

const ALPHABET: [u8; 64] = [
    b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H', b'I', b'J', b'K', b'L', b'M', b'N', b'O', b'P',
    b'Q', b'R', b'S', b'T', b'U', b'V', b'W', b'X', b'Y', b'Z', b'a', b'b', b'c', b'd', b'e', b'f',
    b'g', b'h', b'i', b'j', b'k', b'l', b'm', b'n', b'o', b'p', b'q', b'r', b's', b't', b'u', b'v',
    b'w', b'x', b'y', b'z', b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'+', b'/',
];

const PAD: u8 = b'=';

/// Base64 encode the bytes
pub fn encode(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let (chunks, remainder): (&[[_; 3]], &_) = data.as_chunks();

    let mut backing = Vec::with_capacity(data.len().div_ceil(3) * 4);
    let out = backing.spare_capacity_mut();

    for (c_num, c) in chunks.iter().enumerate() {
        // c is 3 bytes, 24 bits
        // we need to re-interpret it as 4 * 6 bits
        let base = c_num * 4;

        // 0b11111100_00000000_00000000
        out[base].write(ALPHABET[(c[0] >> 2) as usize]);
        // 0b00000011_11110000_00000000
        out[base + 1].write(ALPHABET[(((c[0] & 0b11) << 4) | (c[1] >> 4)) as usize]);
        // 0b00000000_00001111_11000000
        out[base + 2].write(ALPHABET[(((c[1] & 0b1111) << 2) | (c[2] >> 6)) as usize]);
        // 0b00000000_00000000_00111111
        out[base + 3].write(ALPHABET[(c[2] & 0b111111) as usize]);
    }

    let base = chunks.len() * 4;
    match remainder.len() {
        1 => {
            out[base].write(ALPHABET[(remainder[0] >> 2) as usize]);
            out[base + 1].write(ALPHABET[((remainder[0] & 0b11) << 4) as usize]);
            out[base + 2].write(PAD);
            out[base + 3].write(PAD);
        }
        2 => {
            out[base].write(ALPHABET[(remainder[0] >> 2) as usize]);
            out[base + 1]
                .write(ALPHABET[(((remainder[0] & 0b11) << 4) | (remainder[1] >> 4)) as usize]);
            out[base + 2].write(ALPHABET[((remainder[1] & 0b1111) << 2) as usize]);
            out[base + 3].write(PAD);
        }
        _ => {}
    }

    unsafe {
        backing.set_len(backing.capacity());
        String::from_utf8_unchecked(backing)
    }
}

#[cfg(test)]
mod tests {
    extern crate test;
    use test::Bencher;

    use super::encode;

    #[test]
    fn encodes_rfc4648_vectors() {
        let cases = [
            (b"" as &[u8], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"fooba", "Zm9vYmE="),
            (b"foobar", "Zm9vYmFy"),
        ];

        for (input, expected) in cases {
            assert_eq!(encode(input), expected);
        }
    }

    #[test]
    fn encodes_binary_bytes() {
        let input = [0x00, 0x01, 0x02, 0xfd, 0xfe, 0xff];
        assert_eq!(encode(&input), "AAEC/f7/");
    }

    #[bench]
    fn bench_encode_short(b: &mut Bencher) {
        let input = b"Hello";
        b.iter(|| {
            let _ = encode(input);
        });
    }

    #[bench]
    fn bench_encode_1k(b: &mut Bencher) {
        let mut input = [0u8; 1024];
        crate::syscall::getrandom(&mut input);

        b.iter(|| {
            let _ = encode(&input);
        });
    }
}
