//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

use core::ffi::CStr;

#[rustfmt::skip]
pub const OPENROUTER: &Site = &Site {
    name: "OpenRouter",
    api_key_env: c"OPENROUTER_API_KEY",
    config_filename: "ort.json",
    host: "openrouter.ai",
    dns_label: &[
        10, b'o', b'p', b'e', b'n', b'r', b'o', b'u', b't', b'e', b'r',
        2, b'a', b'i',
        0,
    ],
    port: 443,
    chat_completions_url: "/api/v1/chat/completions",
    list_url: "/api/v1/models",
};

#[rustfmt::skip]
pub const NVIDIA: &Site = &Site {
    name: "NVIDIA",
    api_key_env: c"NVIDIA_API_KEY",
    config_filename: "nrt.json",
    host: "integrate.api.nvidia.com",
    dns_label: &[
        9, b'i', b'n', b't', b'e', b'g', b'r', b'a', b't', b'e',
        3, b'a', b'p', b'i',
        6, b'n', b'v', b'i', b'd', b'i', b'a',
        3, b'c', b'o', b'm',
        0,
    ],
    port: 443,
    chat_completions_url: "/v1/chat/completions",
    list_url: "/v1/models",
};

pub const MOCK: &Site = &Site {
    name: "MOCK",
    api_key_env: c"ORT_MOCK_API_KEY",
    config_filename: "mrt.json",
    host: "localhost",
    dns_label: &[9, b'l', b'o', b'c', b'a', b'l', b'h', b'o', b's', b't', 0],
    port: 8000,
    chat_completions_url: "/v1/chat/completions",
    list_url: "/v1/models",
};

pub struct Site {
    pub name: &'static str,
    pub api_key_env: &'static CStr,
    pub config_filename: &'static str,
    pub host: &'static str,
    pub dns_label: &'static [u8],
    pub port: u16,
    pub chat_completions_url: &'static str,
    pub list_url: &'static str,
}
