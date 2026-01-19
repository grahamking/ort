//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::time::Duration;

extern crate alloc;
use alloc::string::String;
use alloc::string::ToString;

use crate::utils;

#[derive(Default, Clone)]
pub struct Stats {
    pub used_model: String,
    pub provider: String,
    pub cost_in_cents: f64, // Divide by 100 for $
    pub elapsed_time: Duration,
    pub time_to_first_token: Option<Duration>,
    pub inter_token_latency_ms: u128,
}

impl Stats {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub(crate) fn as_string(&self) -> String {
        // "{used_model} at {provider}. {cost_in_cents:.4} cents. {elapsed_time} ({time_to_first_token} TTFT, {inter_token_latency_ms}ms ITL)",
        let mut s = String::with_capacity(256);
        s.push_str(&self.used_model);
        s.push_str(" at ");
        s.push_str(&self.provider);
        s.push_str(". ");
        s.push_str(&utils::float_to_string(self.cost_in_cents, 4));
        s.push_str(" cents. ");
        s.push_str(&format_duration(self.elapsed_time));
        s.push_str(" (");
        s.push_str(&format_duration(
            self.time_to_first_token.unwrap_or_default(),
        ));
        s.push_str(" TTFT, ");
        s.push_str(&utils::num_to_string(self.inter_token_latency_ms as usize));
        s.push_str("ms ITL)");
        s
    }
}

// Format the Duration as minutes, seconds and milliseconds.
// examples: 3m12s, 5s, 400ms, 12m, 4s
fn format_duration(d: Duration) -> String {
    let total_millis = d.as_millis();
    let minutes = total_millis / 60_000;
    let seconds = (total_millis % 60_000) / 1_000;
    let milliseconds = total_millis % 1_000;

    let mut result = String::new();

    if minutes > 0 {
        result.push_str(&utils::num_to_string(minutes as usize));
        result.push('m');
    }

    if seconds > 0 {
        if seconds <= 2 {
            result.push_str(&utils::num_to_string(seconds as usize));
            result.push('.');
            result.push_str(&utils::num_to_string(
                (milliseconds as f64 / 100.0) as usize,
            ));
            result.push('s');
        } else {
            result.push_str(&utils::num_to_string(seconds as usize));
            result.push('s');
        }
    }

    if milliseconds > 0 && minutes == 0 && seconds == 0 {
        result.push_str(&utils::num_to_string(milliseconds as usize));
        result.push_str("ms");
    }

    // Handle the case where duration is 0
    if result.is_empty() {
        result = "0ms".to_string();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::format_duration;
    use core::time::Duration;

    #[test]
    fn format_duration_zero() {
        assert_eq!(format_duration(Duration::from_millis(0)), "0ms");
    }

    #[test]
    fn format_duration_milliseconds_only() {
        assert_eq!(format_duration(Duration::from_millis(400)), "400ms");
    }

    #[test]
    fn format_duration_seconds_only() {
        assert_eq!(format_duration(Duration::from_secs(5)), "5s");
    }

    #[test]
    fn format_duration_seconds_with_tenths() {
        assert_eq!(format_duration(Duration::from_millis(1250)), "1.2s");
        assert_eq!(format_duration(Duration::from_millis(2345)), "2.3s");
    }

    #[test]
    fn format_duration_minutes_only() {
        assert_eq!(format_duration(Duration::from_secs(12 * 60)), "12m");
    }

    #[test]
    fn format_duration_minutes_and_seconds() {
        let d = Duration::from_secs(3 * 60 + 12);
        assert_eq!(format_duration(d), "3m12s");
    }
}
