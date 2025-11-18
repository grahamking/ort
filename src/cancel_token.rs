//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::sync::{
    Once,
    atomic::{AtomicBool, Ordering},
};

static CANCELLED: AtomicBool = AtomicBool::new(false);
static INIT: Once = Once::new();

// A way to stop a running thread
// Loosely inspired by tokio's CancellationToken
#[derive(Clone, Copy)]
pub struct CancelToken(&'static AtomicBool);

impl CancelToken {
    pub fn init() -> Self {
        INIT.call_once(|| unsafe { install_sigint_handler() });
        CancelToken(&CANCELLED)
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

extern "C" fn handle_sigint(_: i32) {
    CANCELLED.store(true, Ordering::SeqCst);
}

unsafe fn install_sigint_handler() {
    const SIGINT: i32 = 2;
    unsafe {
        let mut sa: sigaction = std::mem::zeroed();
        sa.sa_flags = 0;
        sa.sa_sigaction = handle_sigint as usize; // treated as sa_handler when SA_SIGINFO not set
        sigemptyset(&mut sa.sa_mask);
        sigaction(SIGINT, &sa, std::ptr::null_mut());
    }
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct sigset_t {
    __val: [u64; 16],
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct sigaction {
    pub sa_sigaction: usize,
    pub sa_mask: sigset_t,
    pub sa_flags: i32,
    pub sa_restorer: Option<extern "C" fn()>,
}

unsafe extern "C" {
    pub fn sigemptyset(set: *mut sigset_t) -> i32;
    pub fn sigaction(signum: i32, act: *const sigaction, oldact: *mut sigaction) -> i32;
}
