//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use crate::{OrtBufReader, OrtResult, Read, ort_err};

/// Read a transfer encoding chunked body, populating `out` with the
/// full re-constructed body.
/// We cannot return partial reads, chunk by chunk, because they might not be
/// valid UTF-8, the chunks can splita byte.
pub fn read_to_string<R: Read>(mut r: OrtBufReader<R>, out: &mut Vec<u8>) -> OrtResult<usize> {
    let mut bytes_read = 0;
    // The size is always valid UTF-8. It's an ASCII hex number.
    let mut size_buf = String::with_capacity(16);
    // The data chunks might be split on a UTF-8 byte
    let mut data_buf = Vec::with_capacity(4096);
    loop {
        // Read size line
        size_buf.clear();
        match r.read_line(&mut size_buf) {
            Ok(0) => {
                return ort_err("EOF while reading chunk size");
            }
            Ok(_) => {}
            Err(err) => {
                return ort_err(format!("Error reading chunk size: {err}"));
            }
        }
        let size_str = size_buf.trim();
        if size_str.is_empty() {
            // Skip initial blank line
            continue;
        }
        let size = match usize::from_str_radix(size_str, 16) {
            Ok(n) => n,
            Err(err) => {
                return ort_err(format!("invalid chunk size: '{size_str}': {err}"));
            }
        };
        if size == 0 {
            // How transfer-encoding chunked signals EOF
            break;
        }

        // Ensure buffer capacity (do not shrink)
        data_buf.clear();
        if data_buf.capacity() < size {
            data_buf.reserve_exact(size);
        }
        unsafe { data_buf.set_len(size) };

        if let Err(err) = r.read_exact(&mut data_buf) {
            return ort_err(format!("Error reading chunked data line: {err}"));
        };
        bytes_read += size;

        out.append(&mut data_buf);
    }
    Ok(bytes_read)
}
