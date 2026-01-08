//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

#![no_std]
#![no_main]

use core::ffi::{CStr, c_char, c_int};

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use ort_openrouter_core::{LibcAlloc, StdoutWriter, cli, libc};

#[global_allocator]
static GLOBAL: LibcAlloc = LibcAlloc;

/// # Safety
/// It's all good
#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(argc: c_int, argv: *const *const c_char) -> c_int {
    // Collect cmd line arguments
    let mut args = Vec::with_capacity(argc as usize);
    for idx in 0..argc {
        let cstr = unsafe { CStr::from_ptr(*argv.add(idx as usize)) };
        args.push(String::from_utf8_lossy(cstr.to_bytes()).into_owned());
    }

    if args.iter().any(|arg| arg == "--version") {
        let v = CString::new(
            env!("CARGO_BIN_NAME").to_string() + " " + env!("CARGO_PKG_VERSION") + "\n",
        )
        .unwrap();
        let _ = unsafe { libc::write(1, v.as_ptr().cast(), v.count_bytes()) };
        return 0;
    }

    // Check stdout for redirection
    let is_terminal = unsafe { libc::isatty(1) == 1 };

    match cli::main(args, is_terminal, StdoutWriter {}) {
        Ok(exit_code) => exit_code as c_int,
        Err(err) => {
            let err_msg = CString::new(err.to_string()).unwrap();
            unsafe { libc::printf(c"ERROR: %s".as_ptr(), err_msg.as_ptr()) };
            1
        }
    }
}
