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

extern "C" fn handle_sigint(_: libc::c_int) {
    CANCELLED.store(true, Ordering::SeqCst);
}

unsafe fn install_sigint_handler() {
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_flags = 0;
        sa.sa_sigaction = handle_sigint as usize; // treated as sa_handler when SA_SIGINFO not set
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
    }
}
