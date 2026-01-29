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
pub(crate) fn to_ascii(mut num: usize, buf: &mut [u8]) -> usize {
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

pub(crate) fn num_to_string(mut num: usize) -> String {
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

/// Convert a float to it's string representation with given number of
/// significant_digits after the decimal.
pub(crate) fn float_to_string(mut f: f64, significant_digits: usize) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f < 0.0 { "-inf".into() } else { "inf".into() };
    }

    let mut result = String::new();

    if f < 0.0 {
        result.push('-');
        f = -f;
    }

    // Handle integer part
    let mut integer_part = f as u64;
    let mut fraction_part = f - (integer_part as f64);

    // Naive integer to string conversion
    if integer_part == 0 {
        result.push('0');
    } else {
        let mut buffer = [0u8; 20]; // Max u64 is ~1.8e19
        let mut idx = 0;
        while integer_part > 0 {
            buffer[idx] = (integer_part % 10) as u8 + b'0';
            integer_part /= 10;
            idx += 1;
        }
        while idx > 0 {
            idx -= 1;
            result.push(buffer[idx] as char);
        }
    }

    if significant_digits > 0 {
        result.push('.');

        for _ in 0..significant_digits {
            fraction_part *= 10.0;
            let digit = fraction_part as u8; // Truncate
            result.push((digit + b'0') as char);
            fraction_part -= digit as f64;
        }
    }

    result
}

#[allow(unused)]
pub(crate) fn print_string(prefix: &CStr, s: &str) {
    let msg = CString::new(s.to_string()).unwrap();
    unsafe { libc::printf(c"%s%s\n".as_ptr(), prefix.as_ptr(), msg.as_ptr()) };
}

pub(crate) fn slug(s: &str) -> String {
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
pub(crate) fn last_filename() -> String {
    let id = tmux_pane_id();
    // 5 because we never expect more than three chars, but to_ascii adds CR and nul byte.
    let mut buf: [u8; 5] = [0; 5];
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
pub(crate) fn get_env(cs: &CStr) -> String {
    let value_ptr = unsafe { libc::getenv(cs.as_ptr()) };
    if value_ptr.is_null() {
        return String::new();
    }
    let c_str = unsafe { CStr::from_ptr(value_ptr) };
    c_str.to_string_lossy().into_owned()
}

/// Create this directory if necessary. Does not create ancestors.
pub(crate) fn ensure_dir_exists(dir: &str) {
    let cs = CString::new(dir).unwrap();
    if !path_exists(cs.as_ref()) {
        unsafe { libc::mkdir(cs.as_ptr(), 0o755) };
    }
}

/// Does this file path exists, and is accessible by the user?
pub(crate) fn path_exists(path: &CStr) -> bool {
    unsafe { libc::access(path.as_ptr(), libc::F_OK) == 0 }
}

/// Read a file into memory
pub(crate) fn filename_read_to_string(filename: &str) -> Result<String, &'static str> {
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

#[cfg(test)]
mod tests {
    use super::float_to_string;

    #[test]
    fn nan_and_infinity() {
        assert_eq!(float_to_string(f64::NAN, 3), "NaN");
        assert_eq!(float_to_string(f64::INFINITY, 3), "inf");
        assert_eq!(float_to_string(f64::NEG_INFINITY, 3), "-inf");
    }

    #[test]
    fn sign_and_integer_part() {
        assert_eq!(float_to_string(-2.5, 1), "-2.5");
        assert_eq!(float_to_string(0.0, 0), "0");
        assert_eq!(float_to_string(-0.0, 2), "0.00"); // f < 0.0 is false for -0.0
        assert_eq!(float_to_string(12345.0, 0), "12345");
    }

    #[test]
    fn fractional_digits_truncate_not_round() {
        // 1.875 is exactly representable; with 2 digits -> "1.87" (truncation)
        assert_eq!(float_to_string(1.875, 2), "1.87");
    }

    #[test]
    fn fractional_leading_zeros() {
        // 1/64 = 0.015625 is exactly representable
        assert_eq!(float_to_string(0.015625, 3), "0.015");
    }

    #[test]
    fn no_decimal_point_when_zero_digits() {
        assert_eq!(float_to_string(3.75, 0), "3");
    }
}
