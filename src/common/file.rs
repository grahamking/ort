//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

use core::ffi::{CStr, c_char, c_int, c_void};
use core::mem::MaybeUninit;

extern crate alloc;
use alloc::string::ToString;

use crate::common::time;
use crate::{OrtResult, Read, Write, libc, ort_err};

pub struct File {
    fd: c_int,
}

impl File {
    /// # Safety
    /// Calls libc::open64 with the given pointer. Is actually safe.
    pub unsafe fn create(path: *const c_char) -> OrtResult<Self> {
        let flags = libc::O_CLOEXEC | libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC;
        let fd = unsafe { libc::open64(path, flags, 0o660 as c_int) };
        if fd == -1 {
            return ort_err("open64 failed");
        }
        Ok(File { fd })
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> OrtResult<usize> {
        let bytes_read = unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut c_void, buf.len()) };
        if bytes_read < 0 {
            ort_err("syscall read error")
        } else {
            Ok(bytes_read as usize)
        }
    }
}

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        let bytes_written =
            unsafe { libc::write(self.fd, buf.as_ptr() as *const c_void, buf.len()) };
        if bytes_written < 0 {
            ort_err("syscall write error")
        } else {
            Ok(bytes_written as usize)
        }
    }

    fn flush(&mut self) -> OrtResult<()> {
        // The stdlib version is a no-op on Unix. It does not fsync.
        Ok(())
    }
}

impl core::fmt::Write for File {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let _ = self.write(s.as_bytes()).unwrap();
        Ok(())
    }
}

pub fn last_modified(path: &CStr) -> OrtResult<time::Instant> {
    let mut st = MaybeUninit::<libc::stat>::uninit();
    unsafe {
        if libc::stat(path.as_ptr(), st.as_mut_ptr()) != 0 {
            return ort_err("stat failed: ".to_string() + &path.to_string_lossy());
        }
    }
    let st = unsafe { st.assume_init() };
    Ok(time::Instant::new(
        st.st_mtime as u64,
        st.st_mtime_nsec as u64,
    ))
}
