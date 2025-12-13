//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

use core::ops::Sub;
use core::time::Duration;

use crate::libc;

#[derive(Copy, Clone)]
pub struct Instant {
    secs: u64,
    nanos: u64,
}

impl Instant {
    pub fn now() -> Self {
        let mut ts: libc::timespec = unsafe { core::mem::zeroed() };
        let out =
            unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts as *mut libc::timespec) };
        if out != 0 {
            panic!("clock_gettime failed: {out}");
        }
        Instant {
            secs: ts.tv_sec as u64,
            nanos: ts.tv_nsec as u64,
        }
    }
}

impl Sub for Instant {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        let total_nanos =
            self.secs as f64 * 1e9 + self.nanos as f64 - rhs.secs as f64 * 1e9 + rhs.nanos as f64;
        Duration::from_nanos(total_nanos as u64)
    }
}
