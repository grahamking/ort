//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;
use alloc::string::String;

#[allow(unused)]
pub struct ReadTool {
    /// Path to the file to read (relative or absolute)
    pub path: String,
    /// Line number to start reading from
    pub offset: Option<u32>,
    /// Maximum number of lines to read
    pub limit: Option<u32>,
}
