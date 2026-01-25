//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::cmp::min;
use core::ffi::c_int;

extern crate alloc;
use alloc::string::String;
use core::cmp;

use crate::{ErrorKind, OrtResult, Read, ort_error, ort_from_err};

const BUF_SIZE: usize = 8 * 1024;

pub struct OrtBufReader<R: Read> {
    inner: R,
    buf: [u8; BUF_SIZE],
    pos: usize, // index of next unread byte in `buf`
    cap: usize, // number of bytes currently in `buf`
}

impl<R: Read> Read for OrtBufReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> OrtResult<usize> {
        self.inner.read(buf)
    }
}

impl<R: Read> OrtBufReader<R> {
    /// Create a new buffered reader with a fixed internal buffer.
    #[inline]
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buf: [0; BUF_SIZE],
            pos: 0,
            cap: 0,
        }
    }

    #[inline(always)]
    fn buffer_consumed(&self) -> bool {
        self.pos >= self.cap
    }

    /// Refill the internal buffer from the underlying reader.
    ///
    /// After this call:
    ///   * `pos` is 0
    ///   * `cap` is the number of bytes read
    #[inline]
    fn fill_buf(&mut self) -> OrtResult<()> {
        self.pos = 0;
        let n = self.inner.read(&mut self.buf)?;
        self.cap = n;
        Ok(())
    }

    /// Reads all bytes up to and including a newline (0x0A) and appends
    /// them to `buf`.
    ///
    /// Existing content of `buf` is preserved.
    /// Returns the number of bytes appended.
    ///
    /// On EOF with no new data, returns `Ok(0)`.
    /// Assumes the stream is valid UTF-8.
    pub fn read_line(&mut self, buf: &mut String) -> OrtResult<usize> {
        let mut total = 0;

        loop {
            if self.buffer_consumed() {
                self.fill_buf()?;
                if self.cap == 0 {
                    // EOF and no more buffered data
                    return Ok(total);
                }
            }

            // Search for newline in the current buffered data
            let available = &self.buf[self.pos..self.cap];
            let mut newline_rel = None;

            for (i, &b) in available.iter().enumerate() {
                if b == b'\n' {
                    newline_rel = Some(i);
                    break;
                }
            }

            let end = match newline_rel {
                Some(i) => self.pos + i + 1, // include newline
                None => self.cap,
            };

            let chunk = &self.buf[self.pos..end];

            // Interpret as UTF-8 and append to the caller's String
            let s = core::str::from_utf8(chunk)
                .map_err(|e| ort_from_err(ErrorKind::FormatError, "utf8 decode", e))?;
            buf.push_str(s);

            total += chunk.len();
            self.pos = end;

            if newline_rel.is_some() {
                // We have consumed up to and including the newline
                return Ok(total);
            }

            // Otherwise loop and refill
        }
    }

    /// Reads exactly `buf.len()` bytes into `buf`.
    ///
    /// Returns an error if EOF is reached before the buffer is full.
    pub fn read_exact(&mut self, buf: &mut [u8]) -> OrtResult<()> {
        let mut offset = 0;
        let len = buf.len();

        while offset < len {
            // First use any bytes remaining in the internal buffer
            if !self.buffer_consumed() {
                let n = cmp::min(len - offset, self.cap - self.pos);
                buf[offset..offset + n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
                self.pos += n;
                offset += n;
                continue;
            }

            // Internal buffer is empty here

            // For large remaining reads, bypass the internal buffer
            if len - offset >= BUF_SIZE {
                let n = self.inner.read(&mut buf[offset..])?;
                if n == 0 {
                    return Err(ort_error(
                        ErrorKind::UnexpectedEof,
                        "unexpected EOF in read_exact",
                    ));
                }
                offset += n;
            } else {
                // For small remaining reads, refill internal buffer and copy
                self.fill_buf()?;
                if self.cap == 0 {
                    return Err(ort_error(
                        ErrorKind::UnexpectedEof,
                        "unexpected EOF in read_exact",
                    ));
                }
            }
        }

        Ok(())
    }
}

pub fn fd_read_to_string(fd: c_int, buffer: &mut String) {
    const READ_CHUNK: usize = 64 * 1024;

    // Write bytes directly into String's backing Vec<u8> for speed.
    let v = unsafe { buffer.as_mut_vec() };

    loop {
        let len = v.len();
        if v.capacity() == len {
            v.reserve(READ_CHUNK);
        }

        let avail = v.capacity() - len;
        let to_read = min(avail, READ_CHUNK);

        let n = unsafe {
            crate::libc::read(
                fd,
                v.as_mut_ptr().add(len) as *mut _,
                to_read as crate::libc::size_t,
            )
        };
        if n == 0 {
            break; // EOF
        }
        if n < 0 {
            break;
        }
        unsafe {
            v.set_len(len + n as usize);
        }
    }

    // Maintain String's UTF-8 invariant. On invalid UTF-8, clear to a valid value.
    if core::str::from_utf8(v.as_slice()).is_err() {
        v.clear();
    }
}
