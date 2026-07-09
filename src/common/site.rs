//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

#[rustfmt::skip]
pub const OPENROUTER: &Site = &Site {
    config_filename: "ort.json",
};

#[rustfmt::skip]
pub const NVIDIA: &Site = &Site {
    config_filename: "nrt.json",
};

pub const MOCK: &Site = &Site {
    config_filename: "mrt.json",
};

pub struct Site {
    pub config_filename: &'static str,
}
