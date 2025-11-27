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

pub fn tmux_pane_id() -> usize {
    std::env::var("TMUX_PANE")
        .ok()
        .and_then(|mut v| {
            // removing leading '%'. Values are e.g. '%4'
            let _ = v.drain(0..1);
            v.parse::<usize>().ok()
        })
        .unwrap_or(0)
}
