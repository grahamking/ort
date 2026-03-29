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
    arch::global_asm,
    ffi::{c_char, c_int},
};

use ort_openrouter_cli::ArenaAlloc;
use ort_openrouter_cli::{StdoutWriter, cli, syscall};

#[cfg(not(debug_assertions))]
mod panic_handler;

#[global_allocator]
static GLOBAL: ArenaAlloc = ArenaAlloc;

#[cfg(not(debug_assertions))]
global_asm!(include_str!("start.s"));
#[cfg(not(debug_assertions))]
global_asm!(include_str!("builtins.s"));

/// The release mode main
/// Note the real entry point is in start.s, which calls this.
///
/// # Safety
/// It's all good
#[cfg(not(debug_assertions))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn main(
    argc: c_int,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> c_int {
    let args = collect_args(argc, argv);
    if is_version_flag(&args) {
        return 0;
    }
    let env = collect_env(envp);

    // Check stdout for redirection
    let is_terminal = syscall::isatty(1);

    match cli::main(&args, env, is_terminal, StdoutWriter {}) {
        Ok(exit_code) => exit_code as c_int,
        Err(err) => {
            let err_msg = CString::new(err.as_string()).unwrap();
            let _ = syscall::write(2, c"ERROR: ".as_ptr().cast(), c"ERROR: ".count_bytes());
            let _ = syscall::write(2, err_msg.as_ptr().cast(), err_msg.count_bytes());
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
    let is_terminal = syscall::isatty(1);

    match cli::main(&args, env, is_terminal, StdoutWriter {}) {
        Ok(exit_code) => (exit_code as u8).into(),
        Err(err) => {
            let err_msg = CString::new(err.as_string()).unwrap();
            let _ = syscall::write(2, c"ERROR: ".as_ptr().cast(), c"ERROR: ".count_bytes());
            let _ = syscall::write(2, err_msg.as_ptr().cast(), err_msg.count_bytes());
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
        let _ = syscall::write(1, v.as_ptr().cast(), v.count_bytes());
        true
    } else {
        false
    }
}

/// Collect cmd line arguments from above stack (release mode)
#[allow(unused)]
fn collect_args(argc: core::ffi::c_int, argv: *const *const core::ffi::c_char) -> Vec<String> {
    let mut args = Vec::with_capacity(argc as usize);
    for idx in 0..argc {
        let cstr = unsafe { core::ffi::CStr::from_ptr(*argv.add(idx as usize)) };
        args.push(String::from_utf8_lossy(cstr.to_bytes()).into_owned());
        //print_string(c"argv: ", &args[idx as usize]);
    }
    args
}

/// Collect env vars we want from above stack (release mode)
///
/// HOME, TMUX_PANE, XDG_CONFIG_HOME, XDG_CACHE_HOME,
/// OPENROUTER_API_KEY, NVIDIA_API_KEY
#[allow(unused)]
fn collect_env(mut envp: *const *const core::ffi::c_char) -> cli::Env {
    use core::ffi::CStr;
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
            //ort_openrouter_cli::utils::print_string(c"env_val = ", &env_val);
            envp = envp.add(1);
        }
    }
    env
}
