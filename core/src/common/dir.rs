//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

use core::ffi::CStr;

extern crate alloc;
use alloc::string::{String, ToString};

use crate::{OrtResult, libc, ort_err};

/// Iterator over the regular files in this directory.
pub struct DirFiles {
    dir: *mut libc::DIR,
}

impl DirFiles {
    pub fn new(p: &CStr) -> OrtResult<Self> {
        let dir: *mut libc::DIR = unsafe { libc::opendir(p.as_ptr()) };
        if dir.is_null() {
            return ort_err("opendir returned null");
        }
        Ok(DirFiles { dir })
    }
}

impl Iterator for DirFiles {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            loop {
                let entry = libc::readdir(self.dir);
                if entry.is_null() {
                    // This is how readdir tells us we hit the end
                    return None;
                }

                let name_ptr = (*entry).d_name.as_ptr();
                if name_ptr.is_null() {
                    // File with no name??
                    continue;
                }

                let d_type = (*entry).d_type;
                if d_type != libc::DT_REG {
                    // Not a regular file
                    // Technically readdir can return DT_UNKNOWN and we
                    // need to `stat` it. We're modern Linux only though.
                    continue;
                }

                let s = CStr::from_ptr(name_ptr).to_string_lossy().to_string();
                return Some(s);
            }
        }
    }
}

impl Drop for DirFiles {
    fn drop(&mut self) {
        unsafe { libc::closedir(self.dir) };
    }
}
