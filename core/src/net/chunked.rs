//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{ErrorKind, OrtResult, Read, common::buf_read, libc, ort_error};

/// Read a transfer encoding chunked body, populating `out` with the
/// full re-constructed body.
/// We cannot return partial reads, chunk by chunk, because they might not be
/// valid UTF-8, the chunks can splita byte.
pub fn read_to_string<R: Read>(
    mut r: buf_read::OrtBufReader<R>,
    out: &mut Vec<u8>,
) -> OrtResult<usize> {
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
                return Err(ort_error(ErrorKind::ChunkedEofInSize, ""));
            }
            Ok(_) => {}
            Err(err) => {
                err.debug_print();
                return Err(ort_error(ErrorKind::ChunkedSizeReadError, ""));
            }
        }
        let size_str = size_buf.trim();
        if size_str.is_empty() {
            // Skip initial blank line
            continue;
        }
        let size = match usize::from_str_radix(size_str, 16) {
            Ok(n) => n,
            Err(_err) => {
                let c_s =
                    CString::new("ERROR invalid chunked size: ".to_string() + size_str).unwrap();
                unsafe {
                    libc::write(2, c_s.as_ptr().cast(), c_s.count_bytes());
                }
                return Err(ort_error(ErrorKind::ChunkedInvalidSize, ""));
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

        if let Err(_err) = r.read_exact(&mut data_buf) {
            // Original included err detail
            return Err(ort_error(ErrorKind::ChunkedDataReadError, ""));
        };
        bytes_read += size;

        out.append(&mut data_buf);
    }
    Ok(bytes_read)
}
