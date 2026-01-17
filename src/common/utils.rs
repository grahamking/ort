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

/// Converts the number to a string, putting it plus a carriage return into `buf`.
/// `buf` must be big enough to hold the largest possible number of digits in
/// your number + 2 ('\n' and '\0').
/// Returns the length of the converted string, including null bute.
pub fn to_ascii(mut num: usize, buf: &mut [u8]) -> usize {
    if num == 0 {
        buf[0] = b'0';
        buf[1] = 0;
        return 2;
    }

    let mut div = 1usize;
    while num / div >= 10 {
        div *= 10;
    }

    let mut i = 0usize;
    while div != 0 {
        buf[i] = b'0' + (num / div) as u8;
        num %= div;
        div /= 10;
        i += 1;
    }
    buf[i] = b'\n';
    i += 1;
    buf[i] = 0;
    i + 1
}

pub fn num_to_string(mut num: usize) -> String {
    if num == 0 {
        return "0".to_string();
    }

    let mut buf: [u8; 20] = [0; 20];
    let mut div = 1usize;
    while num / div >= 10 {
        div *= 10;
    }

    let mut i = 0usize;
    while div != 0 {
        buf[i] = b'0' + (num / div) as u8;
        num %= div;
        div /= 10;
        i += 1;
    }

    unsafe { String::from_utf8_unchecked(buf[..i].into()) }
}

/* Not currently used
pub fn print_string(prefix: &CStr, s: &str) {
    let msg = CString::new(s.to_string()).unwrap();
    unsafe { libc::printf(c"%s%s\n".as_ptr(), prefix.as_ptr(), msg.as_ptr()) };
}
*/

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

// The filename of the last invocation of `ort`, taking into account tmux pane ID.
pub fn last_filename() -> String {
    let id = tmux_pane_id();
    // 4 because we never expect more than two chars, but to_ascii adds CR and nul byte.
    let mut buf: [u8; 4] = [0, 0, 0, 0];
    let buf_len = to_ascii(id, &mut buf[..]);

    let mut out = String::with_capacity(16);
    out.push_str("last-");
    // safety: to_ascii only returns chars '0'-'9'.
    // buf_len-2 to trim the carriage return and null byte
    out.push_str(unsafe { str::from_utf8_unchecked(&buf[..buf_len - 2]) });
    out.push_str(".json");

    out
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
pub fn filename_read_to_string(filename: &str) -> Result<String, &'static str> {
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
