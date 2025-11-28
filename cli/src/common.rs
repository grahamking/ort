//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!
//! Core pieces used by both input/request and output/response paths.
//! Also general utlities even if only used by input or output.

use crate::{OrtError, ort_error};

pub mod cancel_token;
pub mod config;
pub mod error;
pub mod multi_channel;
pub mod utils;

impl From<std::io::Error> for OrtError {
    fn from(err: std::io::Error) -> OrtError {
        ort_error(err.to_string())
    }
}
