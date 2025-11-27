//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!
//! Input/request path, from the point of the view of the user, so
//! cli argument parsing, gathing the input, preparing, serializing
//! and sending the request.

pub mod args;
pub mod cli;
pub mod list;
pub mod prompt;
pub mod to_json;
