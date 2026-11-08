//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025, 2026 Graham King

#![cfg_attr(not(feature = "responses"), allow(unused))]

mod request;
pub(crate) use request::PROMPT_URL;
pub use request::build_body;
