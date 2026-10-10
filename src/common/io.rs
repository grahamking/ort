//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

use core::ffi::c_void;

use crate::{ErrorKind, OrtResult, net::AsFd, ort_err, ort_error, syscall};

pub(crate) const MAX_LEN_UTF8: usize = 4;

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> OrtResult<usize>;

    fn read_exact(&mut self, mut buf: &mut [u8]) -> OrtResult<()> {
        while !buf.is_empty() {
            let n = self.read(buf)?;
            if n == 0 {
                break;
            }
            buf = &mut buf[n..];
        }

        if !buf.is_empty() {
            Err(ort_error(ErrorKind::UnexpectedEof, ""))
        } else {
            Ok(())
        }
    }
}

pub trait ReadLine {
    /// Reads all bytes up to and including a newline (0x0A) and appends
    /// them to `buf`.
    ///
    /// Existing content of `buf` is preserved.
    /// Returns the number of bytes appended.
    ///
    /// On EOF with no new data, returns `Ok(0)`.
    /// Assumes the stream is valid UTF-8.
    fn read_line(&mut self, buf: &mut String) -> OrtResult<usize>;
}

pub trait Write: AsFd {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize>;
    fn flush(&mut self) -> OrtResult<()>;

    fn write_all(&mut self, mut buf: &[u8]) -> OrtResult<()> {
        while !buf.is_empty() {
            match self.write(buf) {
                Ok(0) => {
                    return Err(ort_error(ErrorKind::UnexpectedEof, "EOF"));
                }
                Ok(n) => buf = &buf[n..],
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }

    fn write_str(&mut self, s: &str) -> OrtResult<usize> {
        self.write(s.as_bytes())
    }

    fn write_char(&mut self, c: char) -> OrtResult<usize> {
        self.write_str(c.encode_utf8(&mut [0; MAX_LEN_UTF8]))
    }

    fn writev(&mut self, vs: &[&[u8]]) -> OrtResult<usize> {
        let fd = self.as_fd();
        let mut bytes_written = 0;
        if fd != 42 {
            // Read fd's get accelerated `writev`
            let mut iovecs = Vec::with_capacity(vs.len());
            for v in vs {
                iovecs.push(crate::syscall::iovec {
                    iov_base: v.as_ptr() as *const c_void,
                    iov_len: v.len(),
                });
            }

            let res = syscall::writev(self.as_fd(), iovecs.as_ptr(), iovecs.len() as i32);
            if res < 0 {
                return Err(ort_err(ErrorKind::Writev, "Failed writev".into()));
            }
            bytes_written = res as usize;
        } else {
            for v in vs {
                bytes_written += self.write(v)?;
            }
        }
        Ok(bytes_written)
    }

    /* Not used yet
    fn write_byte(&mut self, b: u8) -> OrtResult<()> {
        // TODO Override this in File, and other places where we can be more efficient
        self.write(&vec![b])?;
        Ok(())
    }
    */
}

impl Write for String {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        unsafe {
            self.as_mut_vec().extend_from_slice(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> OrtResult<()> {
        Ok(())
    }
}

impl Write for Vec<u8> {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        self.extend(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> OrtResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_writev() {
        extern crate alloc;
        use alloc::string::ToString;

        let s1 = "Hello ".to_string();
        let s2 = "world!".to_string();
        let vs = &[s1.as_bytes(), s2.as_bytes()];

        let mut iovecs = Vec::with_capacity(vs.len());
        for v in vs {
            iovecs.push(crate::syscall::iovec {
                iov_base: v.as_ptr() as *const c_void,
                iov_len: v.len(),
            });
        }

        let bytes_written = syscall::writev(1, iovecs.as_ptr(), iovecs.len() as i32);

        assert_eq!(bytes_written, 12);
    }
}
