//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

use core::ops::Sub;
use core::time::Duration;

use crate::{ErrorKind, OrtResult, ort_error};

#[derive(Copy, Clone, PartialEq, PartialOrd)]
pub struct Instant {
    secs: u64,
    nanos: u64,
}

impl Instant {
    pub fn new(secs: u64, nanos: u64) -> Self {
        Instant { secs, nanos }
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

#[derive(Copy, Clone)]
pub struct Ticks(u64);

impl Ticks {
    pub fn now() -> Self {
        read_tsc()
    }
}

fn read_tsc() -> Ticks {
    let low: u32;
    let high: u32;

    unsafe {
        core::arch::asm!(
            "rdtsc",
            lateout("eax") low,
            lateout("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }

    Ticks((u64::from(high) << 32) | u64::from(low))
}

#[derive(Copy, Clone)]
pub struct TscCalibration {
    numerator: u32,
    denominator: u32,
    crystal_hz: u32,
}

impl TscCalibration {
    fn tsc_hz(self) -> u64 {
        u64::from(self.crystal_hz) * u64::from(self.numerator) / u64::from(self.denominator)
    }
}

pub fn tsc_calibration() -> OrtResult<TscCalibration> {
    use core::arch::x86_64::__cpuid_count;

    let max_basic_leaf = __cpuid_count(0, 0).eax;
    if max_basic_leaf < 0x15 {
        return Err(ort_error(ErrorKind::TscCpuidLeafUnavailable, ""));
    }

    let leaf = __cpuid_count(0x15, 0);
    if leaf.eax == 0 || leaf.ebx == 0 {
        return Err(ort_error(ErrorKind::TscInvalidCalibration, ""));
    }
    if leaf.ecx == 0 {
        return Err(ort_error(ErrorKind::TscMissingCrystalClock, ""));
    }

    Ok(TscCalibration {
        denominator: leaf.eax,
        numerator: leaf.ebx,
        crystal_hz: leaf.ecx,
    })
}

pub fn elapsed_duration(start: Ticks, end: Ticks, calibration: TscCalibration) -> Duration {
    let ticks = end.0.saturating_sub(start.0) as u128;
    let nanos = ticks * 1_000_000_000u128 / u128::from(calibration.tsc_hz());
    let secs = (nanos / 1_000_000_000u128) as u64;
    let subsec_nanos = (nanos % 1_000_000_000u128) as u32;
    Duration::new(secs, subsec_nanos)
}
