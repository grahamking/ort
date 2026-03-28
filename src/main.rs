//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025-2026 Graham King
//!
//! This main.rs contains two main:
//! - A no_std release build
//! - A regular debug build

#![cfg_attr(not(debug_assertions), no_std)]
#![cfg_attr(not(debug_assertions), no_main)]
#![cfg_attr(not(debug_assertions), feature(lang_items))]
#![allow(internal_features)]

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
use core::{
    arch::{asm, global_asm},
    ffi::{CStr, c_char, c_int},
};

use ort_openrouter_cli::ArenaAlloc;

#[global_allocator]
static GLOBAL: ArenaAlloc = ArenaAlloc;

#[cfg(not(debug_assertions))]
#[rustfmt::skip]
global_asm!(

    ".global _start",
    "_start:",

    // argc
    "  mov rdi, [rsp]",

    // argv. points to a list of argument pointers.
    "  mov rsi, rsp",
    "  add rsi, 8",

    // envp
    // argc * pointer_size further up the stack gets us to environment variables
    // first env var is at rsp + argc * 8 + 1
    "  mov rdx, rsp",
    // 1. skip argc
    "  add rdx, 8",
    // 2. skip all argvs
    // argc is in rdi
    "  mov rcx, rdi",
    // multiply by 8 (2^3). 8 is pointer size.
    "  shl rcx, 3",
    "  add rdx, rcx",
    // 3. argv list ends with a null pointer, skip that
    "  add rdx, 8",

    // main
    "  call main",

    // Return code from main
    "  mov edi, eax",
    // exit syscall number
    "  mov eax, 60",
    // does not return
    "  syscall",
    // undefined instruction in case we go past the end
    "  ud2",
);

/// The release mode main
///
/// # Safety
/// It's all good
#[cfg(not(debug_assertions))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(
    argc: c_int,
    argv: *const *const c_char,
    mut envp: *const *const c_char,
) -> c_int {
    // Collect cmd line arguments
    let mut args = Vec::with_capacity(argc as usize);
    for idx in 0..argc {
        let cstr = unsafe { CStr::from_ptr(*argv.add(idx as usize)) };
        args.push(String::from_utf8_lossy(cstr.to_bytes()).into_owned());
        //print_string(c"argv: ", &args[idx as usize]);
    }
    if is_version_flag(&args) {
        return 0;
    }

    // Collect env vars we want:
    // HOME, TMUX_PANE, XDG_CONFIG_HOME, XDG_CACHE_HOME,
    // OPENROUTER_API_KEY, NVIDIA_API_KEY
    let mut env: cli::Env = Default::default();
    unsafe {
        while !(*envp).is_null() {
            let env_cstr = CStr::from_ptr(*envp);

            // format is key=value, e.g. "USER=graham"
            let mut parts = env_cstr.to_str().unwrap().split("=");
            let key = parts.next().unwrap();
            let value = parts.next().unwrap();
            match key {
                "HOME" => env.HOME = Some(value),
                "TMUX_PANE" => env.TMUX_PANE = Some(value),
                "XDG_CONFIG_HOME" => env.XDG_CONFIG_HOME = Some(value),
                "XDG_CACHE_HOME" => env.XDG_CACHE_HOME = Some(value),
                "OPENROUTER_API_KEY" => env.OPENROUTER_API_KEY = Some(value),
                "NVIDIA_API_KEY" => env.NVIDIA_API_KEY = Some(value),
                _ => {}
            }
            //let env_val = String::from_utf8_lossy(env_cstr.to_bytes()).into_owned();
            //print_string(c"env_val = ", &env_val);
            envp = envp.add(1);
        }
    }

    // Check stdout for redirection
    let is_terminal = libc::isatty(1);

    match cli::main(&args, env, is_terminal, StdoutWriter {}) {
        Ok(exit_code) => exit_code as c_int,
        Err(err) => {
            let err_msg = CString::new(err.as_string()).unwrap();
            let _ = libc::write(2, c"ERROR: ".as_ptr().cast(), c"ERROR: ".count_bytes());
            let _ = libc::write(2, err_msg.as_ptr().cast(), err_msg.count_bytes());
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

    // The environment variable are already in memory, above stack pointer on start.
    // Release mode build uses that, never copies them.
    // Rust's debug mode copies them onto the heap. Use `leak` to make them static again.
    macro_rules! env_str {
        ($name:literal) => {
            std::env::var($name).ok().map(|v| {
                let s: &'static str = v.leak();
                s
            })
        };
    }

    let env = cli::Env {
        HOME: env_str!("HOME"),
        TMUX_PANE: env_str!("TMUX_PANE"),
        XDG_CONFIG_HOME: env_str!("XDG_CONFIG_HOME"),
        XDG_CACHE_HOME: env_str!("XDG_CACHE_HOME"),
        OPENROUTER_API_KEY: env_str!("OPENROUTER_API_KEY"),
        NVIDIA_API_KEY: env_str!("NVIDIA_API_KEY"),
    };

    // Check stdout for redirection
    let is_terminal = libc::isatty(1);

    match cli::main(&args, env, is_terminal, StdoutWriter {}) {
        Ok(exit_code) => (exit_code as u8).into(),
        Err(err) => {
            let err_msg = CString::new(err.as_string()).unwrap();
            let _ = libc::write(2, c"ERROR: ".as_ptr().cast(), c"ERROR: ".count_bytes());
            let _ = libc::write(2, err_msg.as_ptr().cast(), err_msg.count_bytes());
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
        let _ = libc::write(1, v.as_ptr().cast(), v.count_bytes());
        true
    } else {
        false
    }
}

#[cfg(not(debug_assertions))]
#[panic_handler]
fn my_panic(info: &core::panic::PanicInfo) -> ! {
    libc::write(1, "panic: ".as_ptr().cast(), "panic: ".len());
    if let Some(msg) = info.message().as_str() {
        libc::write(2, msg.as_ptr().cast(), msg.len());
        libc::write(2, ", ".as_ptr().cast(), 2);
    }
    if let Some(location) = info.location() {
        let f = location.file();
        libc::write(2, f.as_ptr().cast(), f.len());
        libc::write(2, ", ".as_ptr().cast(), 2);

        let line = location.line();
        let line_s = ort_openrouter_cli::utils::num_to_string(line);
        libc::write(2, line_s.as_ptr().cast(), line_s.len());
    }
    libc::write(1, "\n".as_ptr().cast(), 1);
    libc::exit(99)
}

#[cfg(not(debug_assertions))]
#[lang = "eh_personality"]
extern "C" fn eh_personality() {}

// The compiler expects memcpy, memset and co to be available.
// Usually libc provides them.

#[cfg(not(debug_assertions))]
#[unsafe(no_mangle)]
extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    unsafe {
        asm!(
            "rep movsb",
            inout("rcx") n => _,
            inout("rdi") dest => _,
            inout("rsi") src => _,
            options(nostack, preserves_flags)
        );
    }
    dest
}

#[cfg(not(debug_assertions))]
#[unsafe(no_mangle)]
extern "C" fn memset(dest: *mut u8, c: i32, count: usize) -> *mut u8 {
    unsafe {
        asm!(
            "rep stosb",
            inout("rcx") count => _,
            inout("rdi") dest => _,
            inout("al") c as u8 => _,
            options(nostack, preserves_flags)
        );
    }
    dest
}

#[cfg(not(debug_assertions))]
#[unsafe(no_mangle)]
extern "C" fn memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if n == 0 || core::ptr::eq(dest.cast_const(), src) {
        return dest;
    }

    let dest_addr = dest as usize;
    let src_addr = src as usize;

    unsafe {
        if dest_addr < src_addr || dest_addr.wrapping_sub(src_addr) >= n {
            asm!(
                "rep movsb",
                inout("rcx") n => _,
                inout("rdi") dest => _,
                inout("rsi") src => _,
                options(nostack, preserves_flags)
            );
        } else {
            asm!(
                "std",
                "rep movsb",
                "cld",
                inout("rcx") n => _,
                inout("rdi") dest.add(n - 1) => _,
                inout("rsi") src.add(n - 1) => _,
                options(nostack)
            );
        }
    }

    dest
}

#[cfg(not(debug_assertions))]
#[unsafe(no_mangle)]
extern "C" fn strlen(s: *const c_char) -> usize {
    let len: usize;

    unsafe {
        asm!(
            "xor eax, eax",
            "mov rcx, -1",
            "repne scasb",
            "not rcx",
            "dec rcx",
            inout("rdi") s => _,
            lateout("rcx") len,
            lateout("rax") _,
            options(nostack)
        );
    }

    len
}

#[cfg(not(debug_assertions))]
#[unsafe(no_mangle)]
extern "C" fn bcmp(s1: *const u8, s2: *const u8, n: usize) -> c_int {
    if n == 0 {
        return 0;
    }

    let result: c_int;

    unsafe {
        asm!(
            "repe cmpsb",
            "setne al",
            "movzx eax, al",
            inout("rcx") n => _,
            inout("rsi") s1 => _,
            inout("rdi") s2 => _,
            lateout("eax") result,
            options(nostack)
        );
    }

    result
}

#[cfg(not(debug_assertions))]
#[unsafe(no_mangle)]
extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> c_int {
    if n == 0 {
        return 0;
    }

    let result: c_int;

    unsafe {
        asm!(
            "repe cmpsb",
            "je 2f",
            "movzx eax, byte ptr [rsi - 1]",
            "movzx edx, byte ptr [rdi - 1]",
            "sub eax, edx",
            "jmp 3f",
            "2:",
            "xor eax, eax",
            "3:",
            inout("rcx") n => _,
            inout("rsi") s1 => _,
            inout("rdi") s2 => _,
            lateout("eax") result,
            lateout("edx") _,
            options(nostack)
        );
    }

    result
}
