//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!
//! This main.rs contains two main:
//! - A no_std release build
//! - A regular debug build

#![cfg_attr(not(debug_assertions), no_std)]
#![cfg_attr(not(debug_assertions), no_main)]

use ort_openrouter_cli::{StdoutWriter, cli, libc};

#[cfg(debug_assertions)]
use std::ffi::CString;

#[cfg(not(debug_assertions))]
extern crate alloc;
#[cfg(not(debug_assertions))]
use alloc::{
    ffi::CString,
    string::{String, ToString},
    vec::Vec,
};

#[cfg(not(debug_assertions))]
use core::ffi::{CStr, c_char, c_int};

#[cfg(not(debug_assertions))]
use ort_openrouter_cli::LibcAlloc;

#[cfg(not(debug_assertions))]
#[global_allocator]
static GLOBAL: LibcAlloc = LibcAlloc;

/// The release mode main
///
/// # Safety
/// It's all good
#[cfg(not(debug_assertions))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(argc: c_int, argv: *const *const c_char) -> c_int {
    // Collect cmd line arguments
    let mut args = Vec::with_capacity(argc as usize);
    for idx in 0..argc {
        let cstr = unsafe { CStr::from_ptr(*argv.add(idx as usize)) };
        args.push(String::from_utf8_lossy(cstr.to_bytes()).into_owned());
    }

    if is_version_flag(&args) {
        return 0;
    }

    // Check stdout for redirection
    let is_terminal = unsafe { libc::isatty(1) == 1 };

    match cli::main(args, is_terminal, StdoutWriter {}) {
        Ok(exit_code) => exit_code as c_int,
        Err(err) => {
            let err_msg = CString::new(err.as_string()).unwrap();
            unsafe { libc::printf(c"ERROR: %s".as_ptr(), err_msg.as_ptr()) };
            1
        }
    }
}

/// Debug mode main
/// Try to keep this as similar to the release main as possible
#[cfg(debug_assertions)]
fn main() -> std::process::ExitCode {
    // Collect cmd line arguments
    let args: Vec<String> = std::env::args().collect();

    if is_version_flag(&args) {
        return 0.into();
    }

    // Check stdout for redirection
    let is_terminal = unsafe { libc::isatty(1) == 1 };

    match cli::main(args, is_terminal, StdoutWriter {}) {
        Ok(exit_code) => (exit_code as u8).into(),
        Err(err) => {
            let err_msg = CString::new(err.as_string()).unwrap();
            unsafe { libc::printf(c"ERROR: %s".as_ptr(), err_msg.as_ptr()) };
            1.into()
        }
    }
}

fn is_version_flag(args: &[String]) -> bool {
    if args.iter().any(|arg| arg == "--version") {
        let v = CString::new(
            env!("CARGO_BIN_NAME").to_string() + " " + env!("CARGO_PKG_VERSION") + "\n",
        )
        .unwrap();
        let _ = unsafe { libc::write(1, v.as_ptr().cast(), v.count_bytes()) };
        true
    } else {
        false
    }
}
