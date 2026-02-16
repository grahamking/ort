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

/// Read a transfer encoding chunked body, chunk by chunk.
///
/// This normally returns the chunks as provided by upstream, except if that
/// would split a mutli-byte char in which case we return N chunks at once.
pub fn read<R: Read, const MAX_CHUNK_SIZE: usize>(
    r: buf_read::OrtBufReader<R>,
) -> ChunkedIterator<R, MAX_CHUNK_SIZE> {
    ChunkedIterator::new(r)
}

pub struct ChunkedIterator<R: Read, const MAX_CHUNK_SIZE: usize> {
    r: buf_read::OrtBufReader<R>,
    size_buf: String,
    data_buf: Vec<u8>,
}

/// Lending Iterator. This doesn't implement Iterator because that doesn't allow the Item
/// to borrow from the iterator (so Item couldn't be &str).
///
/// max_chunk_size: Estimated size of the biggest chunk we will receive.
/// Ideally a power of 2. It's OK if this is wrong, we will realloc.
impl<R: Read, const MAX_CHUNK_SIZE: usize> ChunkedIterator<R, MAX_CHUNK_SIZE> {
    fn new(r: buf_read::OrtBufReader<R>) -> ChunkedIterator<R, MAX_CHUNK_SIZE> {
        ChunkedIterator {
            r,
            size_buf: String::with_capacity(16),
            data_buf: Vec::with_capacity(MAX_CHUNK_SIZE),
        }
    }

    pub fn next_chunk(&mut self) -> Option<OrtResult<&str>> {
        let mut bytes_read = 0;
        // Usually we only go through the loop once per call.
        // Exceptions are the initial blank line, and splitting a multi-byte char.
        loop {
            // Read size line
            // The size is always valid UTF-8. It's an ASCII hex number.
            self.size_buf.clear();
            match self.r.read_line(&mut self.size_buf) {
                Ok(0) => {
                    return Some(Err(ort_error(ErrorKind::ChunkedEofInSize, "")));
                }
                Ok(_) => {}
                Err(err) => {
                    err.debug_print();
                    return Some(Err(ort_error(ErrorKind::ChunkedSizeReadError, "")));
                }
            }
            let size_str = self.size_buf.trim();
            if size_str.is_empty() {
                // Skip initial blank line
                continue;
            }
            let size = match usize::from_str_radix(size_str, 16) {
                Ok(n) => n,
                Err(_err) => {
                    let c_s = CString::new("ERROR invalid chunked size: ".to_string() + size_str)
                        .unwrap();
                    unsafe {
                        libc::write(2, c_s.as_ptr().cast(), c_s.count_bytes());
                    }
                    return Some(Err(ort_error(ErrorKind::ChunkedInvalidSize, "")));
                }
            };
            if size == 0 {
                // How transfer-encoding chunked signals EOF
                return None;
            }

            // Ensure buffer capacity (do not shrink)
            if bytes_read == 0 {
                self.data_buf.clear();
            }
            // no-op if already enough space, so we don't need to check
            self.data_buf.reserve_exact(size);
            unsafe { self.data_buf.set_len(size + bytes_read) };

            if let Err(_err) = self.r.read_exact(&mut self.data_buf[bytes_read..]) {
                // Original included err detail
                return Some(Err(ort_error(ErrorKind::ChunkedDataReadError, "")));
            };
            bytes_read += size;

            // If we split a UTF-8 multi-byte character on the end of the chunk,
            // fetch the next chunk. This really happens.
            let last_byte = self.data_buf[self.data_buf.len() - 1];
            if (last_byte & 0b1000_0000) != 0 {
                //let c_s = CString::new("SPLIT MULTI-BYTE CHAR\n").unwrap();
                //unsafe { libc::write(2, c_s.as_ptr().cast(), c_s.count_bytes()) };
                continue;
            }
            break;
        }
        Some(Ok(unsafe { str::from_utf8_unchecked(&self.data_buf) }))
    }
}
