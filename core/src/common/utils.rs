//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use core::ffi::{c_str::CStr, c_void};

use crate::libc;

pub fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or('-')
            } else {
                '-'
            }
        })
        .collect()
}

pub fn tmux_pane_id() -> usize {
    let mut v = get_env(c"TMUX_PANE");
    if v.is_empty() {
        return 0;
    }
    // removing leading '%'. Values are e.g. '%4'
    let _ = v.drain(0..1);
    v.parse::<usize>().ok().unwrap_or(0)
}

/// Read the value of an environment variable
// Can't use std::env, we're no_std
pub fn get_env(cs: &CStr) -> String {
    let value_ptr = unsafe { libc::getenv(cs.as_ptr()) };
    if value_ptr.is_null() {
        return String::new();
    }
    let c_str = unsafe { CStr::from_ptr(value_ptr) };
    c_str.to_string_lossy().into_owned()
}

/// Create this directory if necessary. Does not create ancestors.
pub fn ensure_dir_exists(dir: &str) {
    let cs = CString::new(dir).unwrap();
    if !path_exists(cs.as_ref()) {
        unsafe { libc::mkdir(cs.as_ptr(), 0o755) };
    }
}

/// Does this file path exists, and is accessible by the user?
pub fn path_exists(path: &CStr) -> bool {
    unsafe { libc::access(path.as_ptr(), libc::F_OK) == 0 }
}

/// Read a file into memory
pub fn read_to_string(filename: &str) -> Result<String, &'static str> {
    let cs = CString::new(filename).unwrap();
    let fd = unsafe { libc::open(cs.as_ptr(), libc::O_RDONLY) };
    if fd < 0 {
        return Err("NOT FOUND");
    }

    let mut content = Vec::new();
    let mut buffer = [0u8; 4096];

    loop {
        let bytes_read =
            unsafe { libc::read(fd, buffer.as_mut_ptr() as *mut c_void, buffer.len()) };

        if bytes_read < 0 {
            let _ = unsafe { libc::close(fd) };
            return Err("READ ERROR");
        }
        if bytes_read == 0 {
            break;
        }
        let bytes_read = bytes_read as usize; // we checked, it's positive
        content.extend_from_slice(&buffer[..bytes_read]);
    }

    let out = String::from_utf8_lossy(&content);
    Ok(out.into_owned().to_string())
}
