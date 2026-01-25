//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

extern crate alloc;
use alloc::vec::Vec;

use crate::{ErrorKind, OrtResult, ort_error};

const MAX_LEN_UTF8: usize = 4;

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

pub trait Write {
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

    /* Not used yet
    fn write_byte(&mut self, b: u8) -> OrtResult<()> {
        // TODO Override this in File, and other places where we can be more efficient
        self.write(&vec![b])?;
        Ok(())
    }
    */
}

impl Write for &mut Vec<u8> {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        self.extend(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> OrtResult<()> {
        Ok(())
    }
}
