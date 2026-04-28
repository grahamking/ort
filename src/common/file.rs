//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::ffi::CStr;
use std::io::{Read as _, Write as _};

use crate::common::time;
use crate::{ErrorKind, OrtResult, Read, Write, ort_error};

pub struct File {
    inner: std::fs::File,
}

impl File {
    /// # Safety
    /// `path` must contain a trailing NUL byte and no interior NUL bytes before it.
    pub unsafe fn create(path: &[u8]) -> OrtResult<Self> {
        let path = CStr::from_bytes_with_nul(path)
            .map_err(|_| ort_error(ErrorKind::FileCreateFailed, "invalid path"))?
            .to_string_lossy();
        let inner = std::fs::File::create(path.as_ref())
            .map_err(|_| ort_error(ErrorKind::FileCreateFailed, "create failed"))?;
        Ok(File { inner })
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> OrtResult<usize> {
        self.inner
            .read(buf)
            .map_err(|_| ort_error(ErrorKind::FileReadFailed, "read failed"))
    }
}

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        self.inner
            .write(buf)
            .map_err(|_| ort_error(ErrorKind::FileWriteFailed, "write failed"))
    }

    fn flush(&mut self) -> OrtResult<()> {
        self.inner
            .flush()
            .map_err(|_| ort_error(ErrorKind::FileWriteFailed, "flush failed"))
    }
}

pub fn last_modified(path: &CStr) -> OrtResult<time::Instant> {
    let metadata = std::fs::metadata(path.to_string_lossy().as_ref())
        .map_err(|_| ort_error(ErrorKind::FileStatFailed, ""))?;
    let modified = metadata
        .modified()
        .map_err(|_| ort_error(ErrorKind::FileStatFailed, ""))?;
    let duration = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| ort_error(ErrorKind::FileStatFailed, ""))?;
    Ok(time::Instant::new(
        duration.as_secs(),
        duration.subsec_nanos() as u64,
    ))
}
