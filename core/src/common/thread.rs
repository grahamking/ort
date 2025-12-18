//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

use core::ffi::{c_int, c_void};
use core::ptr;

use crate::libc;

// Linux default (libpthread) is 8 MiB. We don't need that much.
const STACK_SIZE: usize = 2 << 20;

/// Start a thread.
/// Returns 0 on success, 1 if allocating stack space failed, 2 if clone failed.
/// # Safety
/// Does not currently make a guard page on the stack, so don't overlow.
pub unsafe fn spawn(cb: extern "C" fn(*mut c_void) -> c_int, arg: *mut c_void) -> c_int {
    let stack_base = unsafe {
        libc::mmap(
            ptr::null_mut(),
            STACK_SIZE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_STACK,
            -1,
            0,
        )
    };
    if stack_base.is_null() {
        return 1;
    }

    // TODO: We should add a guard page in case stack overflow

    let tid = unsafe {
        libc::clone(
            cb,
            stack_base.add(STACK_SIZE - 1),
            libc::CLONE_VM | libc::CLONE_FS | libc::CLONE_FILES | libc::SIGCHLD,
            arg,
        )
    };
    if tid <= 0 {
        return 2;
    }

    0
}
