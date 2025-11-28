//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

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
    // Can't use std::env, we're no_std
    let value_ptr = unsafe { getenv(c"TMUX_PANE".as_ptr()) };
    if value_ptr.is_null() {
        return 0;
    }
    let c_str = unsafe { CStr::from_ptr(value_ptr) };
    let mut v = c_str.to_string_lossy().into_owned();
    // removing leading '%'. Values are e.g. '%4'
    let _ = v.drain(0..1);
    v.parse::<usize>().ok().unwrap_or(0)
}

unsafe extern "C" {
    fn getenv(name: *const c_char) -> *const c_char;
}
