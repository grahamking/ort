//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

#![no_std]
#![no_main]
#![allow(internal_features)]
#![feature(lang_items)]
//#![feature(alloc_error_handler)]

use core::alloc::Layout;
use core::ffi::{CStr, c_char, c_int, c_void};

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use ort_openrouter_core::{OrtResult, Write, cli, libc, ort_error};

//
// Allocator
//

struct LibcAlloc;

unsafe impl core::alloc::GlobalAlloc for LibcAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { libc::malloc(layout.size().max(layout.align())) as *mut u8 }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe { libc::calloc(1, layout.size().max(layout.align())) as *mut u8 }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        unsafe { libc::free(ptr as *mut c_void) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, _layout: Layout, new_size: usize) -> *mut u8 {
        unsafe { libc::realloc(ptr as *mut c_void, new_size) as *mut u8 }
    }
}

#[global_allocator]
static GLOBAL: LibcAlloc = LibcAlloc;

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}

/*
#[alloc_error_handler]
fn oom(_: Layout) -> ! {
    unsafe { libc::abort() }
}
*/

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe { libc::abort() }
}

/// # Safety
/// It's all good
#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(argc: c_int, argv: *const *const c_char) -> c_int {
    // Collect all environment variables into Vec<String>
    /*
    let mut env_vars: Vec<String> = Vec::new();
    unsafe {
        let mut p = libc::environ as *const *const c_char;
        while !(*p).is_null() {
            let cstr = CStr::from_ptr(*p);
            env_vars.push(String::from_utf8_lossy(cstr.to_bytes()).into_owned());
            p = p.add(1);
        }
    }
    */

    // Collect cmd line arguments
    let mut args = Vec::with_capacity((argc - 1) as usize);
    for idx in 1..argc {
        let cstr = unsafe { CStr::from_ptr(*argv.add(idx as usize)) };
        args.push(String::from_utf8_lossy(cstr.to_bytes()).into_owned());
        //p = p.add(1);
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

struct StdoutWriter {}

impl Write for StdoutWriter {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        let bytes_written = unsafe { libc::write(1, buf.as_ptr() as *const c_void, buf.len()) };
        if bytes_written >= 0 {
            Ok(bytes_written as usize)
        } else {
            Err(ort_error("Failed writing to stdout"))
        }
    }

    fn flush(&mut self) -> OrtResult<()> {
        Ok(())
    }
}
