//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

use crate::{OrtResult, Write};

/// No escapes or special characters, just write the bytes
pub(crate) fn write_json_str_simple<W: Write>(w: &mut W, s: &str) -> OrtResult<()> {
    w.write_char('"')?;
    w.write_str(s)?;
    w.write_char('"')?;
    Ok(())
}

// Writes a JSON string (with surrounding quotes) with proper escaping, no allocations.
pub fn write_json_str<W: Write>(w: &mut W, s: &str) -> OrtResult<()> {
    w.write_char('"')?;
    write_encoded_bytes(w, s.as_bytes())?;
    w.write_char('"')?;

    Ok(())
}

pub fn write_encoded_bytes<W: Write>(w: &mut W, bytes: &[u8]) -> OrtResult<()> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut start = 0;

    for i in 0..bytes.len() {
        let b = bytes[i];
        let esc = match b {
            b'"' => Some(b"\\\""), // as &[u8]),
            b'\\' => Some(b"\\\\"),
            b'\n' => Some(b"\\n"),
            b'\r' => Some(b"\\r"),
            b'\t' => Some(b"\\t"),
            0x08 => Some(b"\\b"),
            0x0C => Some(b"\\f"),
            0x00..=0x1F => None, // will use \u00XX
            _ => continue,
        };

        if start < i {
            w.write(&bytes[start..i])?;
        }

        if let Some(e) = esc {
            w.write(e)?;
        } else {
            // Generic control char: \u00XX
            let mut buf = [0u8; 6];
            buf[0] = b'\\';
            buf[1] = b'u';
            buf[2] = b'0';
            buf[3] = b'0';
            buf[4] = HEX[((b >> 4) & 0xF) as usize];
            buf[5] = HEX[(b & 0xF) as usize];
            w.write(&buf)?;
        }

        start = i + 1;
    }

    if start < bytes.len() {
        w.write(&bytes[start..])?;
    }
    Ok(())
}

/*
fn write_bool<W: Write>(w: &mut W, v: bool) -> OrtResult<usize> {
    if v {
        w.write_str("true")
    } else {
        w.write_str("false")
    }
}
*/

/*
fn write_u32<W: Write>(w: &mut W, mut n: u32) -> OrtResult<usize> {
    if n == 0 {
        return w.write_str("0");
    }
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    w.write(&buf[i..])
}
*/
