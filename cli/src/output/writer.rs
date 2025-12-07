//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use crate::{OrtResult, ort_from_err};

use crate::Flushable;
use std::fmt;

/// Adapter that lets us use any `std::io::Write` as a UTF-8-only `fmt::Write`.
pub struct IoFmtWriter<W: std::io::Write> {
    inner: W,
}

impl<W: std::io::Write> IoFmtWriter<W> {
    pub fn new(inner: W) -> Self {
        IoFmtWriter { inner }
    }

    pub fn into_inner(self) -> W {
        self.inner
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl<W: std::io::Write> fmt::Write for IoFmtWriter<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        std::io::Write::write_all(&mut self.inner, s.as_bytes()).map_err(|_| fmt::Error)
    }

    fn write_char(&mut self, c: char) -> fmt::Result {
        let mut buf = [0u8; 4];
        let slice = c.encode_utf8(&mut buf);
        std::io::Write::write_all(&mut self.inner, slice.as_bytes()).map_err(|_| fmt::Error)
    }
}

impl<W: std::io::Write> Flushable for IoFmtWriter<W> {
    fn flush(&mut self) -> OrtResult<()> {
        std::io::Write::flush(&mut self.inner).map_err(ort_from_err)
    }
}
