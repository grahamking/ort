//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

use core::ffi::{CStr, c_char, c_int, c_void};
use core::mem::MaybeUninit;

extern crate alloc;

use crate::common::time;
use crate::{ErrorKind, OrtResult, Read, Write, ort_error, syscall};

pub struct File {
    fd: c_int,
}

impl File {
    /// # Safety
    /// Calls libc::open with the given pointer. Is actually safe.
    /// Path must end with a null byte.
    pub unsafe fn create(path: &[u8]) -> OrtResult<Self> {
        let flags = syscall::O_CLOEXEC | syscall::O_WRONLY | syscall::O_CREAT | syscall::O_TRUNC;
        let fd = syscall::open(path.as_ptr() as *const c_char, flags, 0o660 as c_int)
            .map_err(|e| ort_error(ErrorKind::FileCreateFailed, e))?;
        if fd == -1 {
            return Err(ort_error(ErrorKind::FileCreateFailed, "open64 failed"));
        }
        Ok(File { fd })
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> OrtResult<usize> {
        let bytes_read = syscall::read(self.fd, buf.as_mut_ptr() as *mut c_void, buf.len());
        if bytes_read < 0 {
            Err(ort_error(ErrorKind::FileReadFailed, "syscall read error"))
        } else {
            Ok(bytes_read as usize)
        }
    }
}

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        let bytes_written = syscall::write(self.fd, buf.as_ptr() as *const c_void, buf.len());
        if bytes_written < 0 {
            Err(ort_error(ErrorKind::FileWriteFailed, "syscall write error"))
        } else {
            Ok(bytes_written as usize)
        }
    }

    fn flush(&mut self) -> OrtResult<()> {
        // The stdlib version is a no-op on Unix. It does not fsync.
        Ok(())
    }
}

pub fn last_modified(path: &CStr) -> OrtResult<time::Instant> {
    let mut st = MaybeUninit::<syscall::Stat>::uninit();
    if syscall::stat(path.as_ptr(), &mut st).is_err() {
        // In debug build print the path.
        #[cfg(debug_assertions)]
        syscall::write(2, path.as_ptr().cast(), path.count_bytes());

        return Err(ort_error(ErrorKind::FileStatFailed, ""));
    }
    let st = unsafe { st.assume_init() };
    Ok(time::Instant::new(
        st.st_mtime as u64,
        st.st_mtime_nsec as u64,
    ))
}
