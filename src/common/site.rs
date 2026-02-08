//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

use core::ffi::CStr;

pub const OPENROUTER: &Site = &Site {
    name: "OpenRouter",
    api_key_env: c"OPENROUTER_API_KEY",
    config_filename: "ort.json",
    host: "openrouter.ai",
    chat_completions_url: "/api/v1/chat/completions",
    list_url: "/api/v1/models",
};

pub const NVIDIA: &Site = &Site {
    name: "NVIDIA",
    api_key_env: c"NVIDIA_API_KEY",
    config_filename: "nrt.json",
    host: "integrate.api.nvidia.com",
    chat_completions_url: "/v1/chat/completions",
    list_url: "/v1/models",
};

pub struct Site {
    pub name: &'static str,
    pub api_key_env: &'static CStr,
    pub config_filename: &'static str,
    pub host: &'static str,
    pub chat_completions_url: &'static str,
    pub list_url: &'static str,
}
