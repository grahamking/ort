//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::String;

use core::ffi::{c_char, c_str::CStr};

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
    unsafe extern "C" {
        fn getenv(name: *const c_char) -> *const c_char;
    }
    let value_ptr = unsafe { getenv(cs.as_ptr()) };
    if value_ptr.is_null() {
        return String::new();
    }
    let c_str = unsafe { CStr::from_ptr(value_ptr) };
    c_str.to_string_lossy().into_owned()
}

/// Create this directory if necessary. Does not create ancestors.
pub fn ensure_dir_exists(dir: &str) {
    unsafe extern "C" {
        fn mkdir(path: *const c_char, mode: u32);
    }
    let cs = CString::new(dir).unwrap();
    if !path_exists(cs.as_ref()) {
        unsafe { mkdir(cs.as_ptr(), 0o755) };
    }
}

/// Does this file path exists, and is accessible by the user?
pub fn path_exists(path: &CStr) -> bool {
    unsafe extern "C" {
        fn access(path: *const c_char, mode: u32) -> u32;
    }
    const F_OK: u32 = 0;
    unsafe { access(path.as_ptr(), F_OK) == 0 }
}
