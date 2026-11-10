//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::ops::AddAssign;
use core::time::Duration;

extern crate alloc;
use alloc::string::String;
use alloc::string::ToString;

use crate::common::utils;
use crate::time;

#[derive(Default, Clone, Debug)]
pub struct Stats {
    pub num_tokens: usize,
    pub metadata: Metadata,
    pub usage: Option<Usage>,
    pub timing: Option<Timing>,
}

#[derive(Default, Clone, Debug)]
pub struct Metadata {
    pub used_model: String,
    pub provider: String,
}

#[derive(Default, Clone, Debug)]
pub struct Usage {
    pub cost_in_cents: Option<f64>, // Divide by 100 for $
    pub tokens: Option<u32>,
    pub web_search_requests: Option<u32>,
    pub tool_calls: Option<u32>,
}

#[derive(Default, Clone, Debug)]
pub struct Timing {
    pub start: time::Ticks,
    pub elapsed_time: Duration,
    pub time_to_first_token: Option<Duration>,
    pub inter_token_latency_ms: u128,
    pub token_stream_start: Option<time::Ticks>,
}

impl AddAssign for Stats {
    fn add_assign(&mut self, other: Self) {
        if let Some(other_usage) = other.usage {
            let mut usage = self.usage.take().unwrap_or_default();
            if let Some(cost) = other_usage.cost_in_cents {
                *usage.cost_in_cents.get_or_insert(0.0) += cost;
            }
            if let Some(tokens) = other_usage.tokens {
                *usage.tokens.get_or_insert(0) += tokens;
            }
            if let Some(web_searches) = other_usage.web_search_requests {
                *usage.web_search_requests.get_or_insert(0) += web_searches;
            }
            if let Some(tool_calls) = other_usage.tool_calls {
                *usage.tool_calls.get_or_insert(0) += tool_calls;
            }
            self.usage = Some(usage);
        }

        if let Some(other_timing) = other.timing {
            let mut t = self.timing.take().unwrap_or_default();
            // We don't add this, only set once
            if t.time_to_first_token.is_none() {
                t.time_to_first_token = other_timing.time_to_first_token;
            }
            t.elapsed_time += other_timing.elapsed_time;
            self.timing = Some(t);
        }

        if self.metadata.provider.is_empty() {
            self.metadata.provider = other.metadata.provider;
        }
        if self.metadata.used_model.is_empty() {
            self.metadata.used_model = other.metadata.used_model;
        }
    }
}

impl Stats {
    pub fn provider(&self) -> &str {
        &self.metadata.provider
    }

    pub fn as_string(&self) -> String {
        // "{used_model} at {provider}. {cost_in_cents:.4} cents. {elapsed_time} ({time_to_first_token} TTFT, {inter_token_latency_ms}ms ITL)",
        let mut s = String::with_capacity(256);

        s.push_str(&self.metadata.used_model);
        if !self.metadata.provider.is_empty() {
            s.push_str(" at ");
            s.push_str(&self.metadata.provider);
        }
        s.push_str(". ");

        if let Some(usage) = &self.usage {
            if let Some(cost_in_cents) = usage.cost_in_cents {
                if cost_in_cents >= 100.0 {
                    s.push('$');
                    s.push_str(&utils::float_to_string(cost_in_cents / 100.0, 2));
                    s.push_str(". ");
                } else {
                    s.push_str(&utils::float_to_string(cost_in_cents, 4));
                    s.push_str(" cents. ");
                }
            }
            if let Some(tokens) = usage.tokens {
                s.push_str(&utils::num_to_human_string(tokens as usize));
                s.push_str(" tokens. ");
            }
            if let Some(web_search_requests) = usage.web_search_requests {
                s.push_str(&utils::num_to_string(web_search_requests));
                s.push_str(" web search. ");
            }
            if let Some(tool_calls) = usage.tool_calls {
                s.push_str(&utils::num_to_string(tool_calls));
                s.push_str(" tool calls. ");
            }
        }

        if let Some(t) = &self.timing {
            if t.elapsed_time != Duration::ZERO {
                s.push_str(&format_duration(t.elapsed_time));
            }
            // We will either both timings or none, only need to check one
            if t.time_to_first_token.is_some() {
                s.push_str(" (");
                s.push_str(&format_duration(t.time_to_first_token.unwrap_or_default()));
                s.push_str(" TTFT, ");
                s.push_str(&utils::num_to_string(t.inter_token_latency_ms as usize));
                s.push_str("ms ITL)");
            }
        }
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
