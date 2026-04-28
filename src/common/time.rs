//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::ops::Sub;
use core::time::Duration;

use crate::OrtResult;

#[derive(Copy, Clone, PartialEq, PartialOrd)]
pub struct Instant {
    inner: std::time::SystemTime,
}

impl Instant {
    pub fn new(secs: u64, nanos: u64) -> Self {
        Instant {
            inner: std::time::UNIX_EPOCH + Duration::new(secs, nanos as u32),
        }
    }
}

impl Sub for Instant {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        self.inner
            .duration_since(rhs.inner)
            .unwrap_or_else(|_| Duration::ZERO)
    }
}

#[derive(Copy, Clone)]
pub struct Ticks(std::time::Instant);

impl Ticks {
    pub fn now() -> Self {
        Ticks(std::time::Instant::now())
    }
}

#[derive(Copy, Clone)]
pub struct TscCalibration;

pub fn tsc_calibration() -> OrtResult<TscCalibration> {
    Ok(TscCalibration)
}

pub fn elapsed_duration(start: Ticks, end: Ticks, _calibration: TscCalibration) -> Duration {
    end.0.saturating_duration_since(start.0)
}
