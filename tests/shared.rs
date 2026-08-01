//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

use ort_openrouter_cli::cli::Env;

pub const HELLO: [&str; 4] = ["assist", "help", "ello", "greet"];

pub fn env() -> Env {
    macro_rules! env_str {
        ($name:literal) => {
            std::env::var($name).ok().map(|v| {
                let s: &'static str = v.leak();
                s
            })
        };
    }
    Env {
        HOME: env_str!("HOME"),
        PWD: env_str!("PWD"),
        TMUX_PANE: env_str!("TMUX_PANE"),
        XDG_CONFIG_HOME: env_str!("XDG_CONFIG_HOME"),
        XDG_CACHE_HOME: env_str!("XDG_CACHE_HOME"),
        OPENROUTER_API_KEY: env_str!("OPENROUTER_API_KEY"),
        NVIDIA_API_KEY: env_str!("NVIDIA_API_KEY"),
    }
}
