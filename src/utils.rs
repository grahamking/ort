//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

pub fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or('-')
            } else {
                '-'
            }
        })
        .collect()
}
