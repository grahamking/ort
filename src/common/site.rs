//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

#[rustfmt::skip]
pub const OPENROUTER: &Site = &Site {
    name: "OpenRouter",
    config_filename: "ort.json",
    dns_label: &[
        10, b'o', b'p', b'e', b'n', b'r', b'o', b'u', b't', b'e', b'r',
        2, b'a', b'i',
        0,
    ],
    port: 443,
};

#[rustfmt::skip]
pub const NVIDIA: &Site = &Site {
    name: "NVIDIA",
    config_filename: "nrt.json",
    dns_label: &[
        9, b'i', b'n', b't', b'e', b'g', b'r', b'a', b't', b'e',
        3, b'a', b'p', b'i',
        6, b'n', b'v', b'i', b'd', b'i', b'a',
        3, b'c', b'o', b'm',
        0,
    ],
    port: 443,
};

pub const MOCK: &Site = &Site {
    name: "MOCK",
    config_filename: "mrt.json",
    dns_label: &[9, b'l', b'o', b'c', b'a', b'l', b'h', b'o', b's', b't', 0],
    port: 8000,
};

pub struct Site {
    pub name: &'static str,
    pub config_filename: &'static str,
    pub dns_label: &'static [u8],
    pub port: u16,
}
