//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!
//! Core pieces used by both input/request and output/response paths.
//! Also general utlities even if only used by input or output.

pub mod alloc;
pub mod buf_read;
pub mod cancel_token;
pub mod config;
pub mod data;
pub mod dir;
pub mod error;
pub mod file;
pub mod io;
pub mod queue;
pub mod resolver;
pub mod site;
pub mod stats;
pub mod thread;
pub mod time;
pub mod utils;
