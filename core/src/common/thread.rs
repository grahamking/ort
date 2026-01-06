//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

use core::ffi::c_void;
use core::mem;
use core::ptr;

use crate::{ErrorKind, OrtResult, libc, ort_err};

// Linux default (libpthread) is 8 MiB. We don't need that much.
const STACK_SIZE: usize = 1024 * 1024; // 1 MiB
const GUARD_SIZE: usize = 64 * 1024; // 64 KiB

/// Start a thread.
/// Returns 0 on success, 1 if allocating stack space failed, 2 if clone failed.
/// # Safety
/// TODO
pub unsafe fn spawn(
    thread_func: extern "C" fn(*mut c_void) -> *mut c_void,
    arg: *mut c_void,
) -> OrtResult<libc::pthread_t> {
    //
    // Stack
    // Memory returned by mmap is aligned.
    // We never free it, when the threads are done the whole program is usually done.
    //

    let stack_base = unsafe {
        libc::mmap(
            ptr::null_mut(),
            STACK_SIZE + GUARD_SIZE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_STACK,
            -1,
            0,
        )
    };
    if stack_base.is_null() {
        return ort_err(ErrorKind::ThreadStackAllocFailed, "");
    }
    unsafe { libc::mprotect(stack_base, GUARD_SIZE, libc::PROT_NONE) };

    //
    // pthread
    //

    let mut thread_id: libc::pthread_t = 0;
    let mut attr: libc::pthread_attr_t = unsafe { mem::zeroed() };

    let rc = unsafe {
        libc::pthread_attr_init(&mut attr);
        // Skip the guard page when handing stack to pthreads.
        libc::pthread_attr_setstack(&mut attr, stack_base.add(GUARD_SIZE), STACK_SIZE);
        let rc = libc::pthread_create(&mut thread_id, &attr, thread_func, arg);
        libc::pthread_attr_destroy(&mut attr);
        rc
    };
    if rc != 0 {
        ort_err(ErrorKind::ThreadSpawnFailed, "")
    } else {
        Ok(thread_id)
    }
}
