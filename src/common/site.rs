//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

use core::ffi::CStr;

pub const OPENROUTER: Site = Site {
    api_key_env: c"OPENROUTER_API_KEY",
    config_filename: "ort.json",
    host: "openrouter.ai",
    chat_completions_url: "/api/v1/chat/completions",
};

pub const NVIDIA: Site = Site {
    api_key_env: c"NVIDIA_API_KEY",
    config_filename: "nrt.json",
    host: "integrate.api.nvidia.com",
    chat_completions_url: "/v1/chat/completions",
};

pub struct Site {
    pub api_key_env: &'static CStr,
    pub config_filename: &'static str,
    pub host: &'static str,
    pub chat_completions_url: &'static str,
}
