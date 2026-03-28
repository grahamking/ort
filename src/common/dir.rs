//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

use core::{ffi::CStr, mem::offset_of};

extern crate alloc;
use alloc::string::{String, ToString};

use crate::{ErrorKind, OrtResult, ort_error, syscall};

/// Iterator over the regular files in this directory.
pub struct DirFiles {
    fd: i32,
    buf: [u8; 4096],
    pos: usize,
    len: usize,
}

impl DirFiles {
    pub fn new(p: &CStr) -> OrtResult<Self> {
        let fd = syscall::open(p.as_ptr(), syscall::O_RDONLY | syscall::O_DIRECTORY, 0)
            .map_err(|_| ort_error(ErrorKind::DirOpenFailed, "open returned error"))?;
        Ok(DirFiles {
            fd,
            buf: [0; 4096],
            pos: 0,
            len: 0,
        })
    }
}

impl Iterator for DirFiles {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.pos == self.len {
                let bytes_read =
                    syscall::getdents64(self.fd, self.buf.as_mut_ptr().cast(), self.buf.len());
                if bytes_read <= 0 {
                    return None;
                }
                self.pos = 0;
                self.len = bytes_read as usize;
            }

            let entry_ptr = unsafe { self.buf.as_ptr().add(self.pos) };
            let entry = unsafe { &*(entry_ptr as *const syscall::linux_dirent64) };
            let reclen = entry.d_reclen as usize;
            if reclen == 0 || self.pos + reclen > self.len {
                return None;
            }
            self.pos += reclen;

            if entry.d_type != syscall::DT_REG {
                // Not a regular file. We intentionally skip DT_UNKNOWN to avoid stat syscalls.
                continue;
            }

            let name_ptr = unsafe {
                entry_ptr
                    .add(offset_of!(syscall::linux_dirent64, d_name))
                    .cast()
            };
            let s = unsafe { CStr::from_ptr(name_ptr) }
                .to_string_lossy()
                .to_string();
            return Some(s);
        }
    }
}

impl Drop for DirFiles {
    fn drop(&mut self) {
        let _ = syscall::close(self.fd);
    }
}
