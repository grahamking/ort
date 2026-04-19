//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!

use core::ops::Sub;
use core::sync::atomic::{AtomicU64, Ordering};
use core::time::Duration;

use crate::{ErrorKind, OrtResult, ort_error, utils};

static TSC_HZ_CACHE: AtomicU64 = AtomicU64::new(0);

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
    tsc_hz: u64,
}

impl TscCalibration {
    fn tsc_hz(self) -> u64 {
        self.tsc_hz
    }
}

fn cpuid_0x15_tsc_hz() -> Result<u64, ErrorKind> {
    use core::arch::x86_64::__cpuid_count;

    let max_basic_leaf = __cpuid_count(0, 0).eax;
    if max_basic_leaf < 0x15 {
        return Err(ErrorKind::TscCpuidLeafUnavailable);
    }

    let leaf = __cpuid_count(0x15, 0);
    if leaf.eax == 0 || leaf.ebx == 0 {
        return Err(ErrorKind::TscInvalidCalibration);
    }
    if leaf.ecx == 0 {
        return Err(ErrorKind::TscMissingCrystalClock);
    }

    Ok(u64::from(leaf.ecx) * u64::from(leaf.ebx) / u64::from(leaf.eax))
}

fn has_invariant_tsc() -> bool {
    use core::arch::x86_64::__cpuid_count;

    let max_extended_leaf = __cpuid_count(0x8000_0000, 0).eax;
    if max_extended_leaf < 0x8000_0007 {
        return false;
    }

    (__cpuid_count(0x8000_0007, 0).edx & (1 << 8)) != 0
}

fn is_amd_processor() -> bool {
    use core::arch::x86_64::__cpuid_count;

    let vendor = __cpuid_count(0, 0);
    vendor.ebx == 0x6874_7541 && vendor.edx == 0x6974_6e65 && vendor.ecx == 0x444d_4163
}

fn read_ascii_u64(path: &[u8]) -> Option<u64> {
    let path = core::str::from_utf8(&path[..path.len() - 1]).ok()?;
    let buf = utils::filename_read_to_bytes(path).ok()?;
    let mut value = 0u64;
    let mut saw_digit = false;
    for &b in &buf {
        match b {
            b'0'..=b'9' => {
                saw_digit = true;
                value = value.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
            }
            b'\n' => break,
            _ => return None,
        }
    }

    saw_digit.then_some(value)
}

fn amd_tsc_hz_from_kernel() -> Option<u64> {
    if !is_amd_processor() {
        return None;
    }

    let mhz = read_ascii_u64(b"/sys/devices/system/cpu/cpu0/acpi_cppc/nominal_freq\0")?;
    mhz.checked_mul(1_000_000)
}

pub fn tsc_calibration() -> OrtResult<TscCalibration> {
    let cached = TSC_HZ_CACHE.load(Ordering::Relaxed);
    if cached != 0 {
        return Ok(TscCalibration { tsc_hz: cached });
    }

    let tsc_hz = match cpuid_0x15_tsc_hz() {
        Ok(hz) => hz,
        Err(err) if has_invariant_tsc() => amd_tsc_hz_from_kernel().ok_or(ort_error(err, ""))?,
        Err(err) => return Err(ort_error(err, "")),
    };

    TSC_HZ_CACHE.store(tsc_hz, Ordering::Relaxed);
    Ok(TscCalibration { tsc_hz })
}

pub fn elapsed_duration(start: Ticks, end: Ticks, calibration: TscCalibration) -> Duration {
    let ticks = end.0.saturating_sub(start.0) as u128;
    let nanos = ticks * 1_000_000_000u128 / u128::from(calibration.tsc_hz());
    let secs = (nanos / 1_000_000_000u128) as u64;
    let subsec_nanos = (nanos % 1_000_000_000u128) as u32;
    Duration::new(secs, subsec_nanos)
}
