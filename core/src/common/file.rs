//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

use core::ffi::{c_char, c_int, c_void};

use crate::{OrtResult, Read, Write, ort_err};

#[allow(non_camel_case_types)]
type size_t = usize;
#[allow(non_camel_case_types)]
type ssize_t = isize;

const O_CLOEXEC: c_int = 0x80000;

//const O_RDONLY: c_int = 0;
const O_WRONLY: c_int = 1;
//const O_RDWR: c_int = 2;
const O_CREAT: c_int = 64;
const O_TRUNC: c_int = 512;

pub struct File {
    fd: c_int,
}

impl File {
    /// # Safety
    /// Calls libc::open64 with the given pointer. Is actually safe.
    pub unsafe fn create(path: *const c_char) -> OrtResult<Self> {
        let flags = O_CLOEXEC | O_WRONLY | O_CREAT | O_TRUNC;
        let fd = unsafe { open64(path, flags, 0o660 as c_int) };
        if fd == -1 {
            return ort_err("open64 failed");
        }
        Ok(File { fd })
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> OrtResult<usize> {
        let bytes_read = unsafe { read(self.fd, buf.as_mut_ptr() as *mut c_void, buf.len()) };
        if bytes_read < 0 {
            ort_err("syscall read error")
        } else {
            Ok(bytes_read as usize)
        }
    }
}

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        let bytes_written = unsafe { write(self.fd, buf.as_ptr() as *const c_void, buf.len()) };
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

unsafe extern "C" {
    fn open64(path: *const c_char, oflag: c_int, ...) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, count: size_t) -> ssize_t;
    fn write(fd: c_int, buf: *const c_void, count: size_t) -> ssize_t;
}
