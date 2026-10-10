//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

pub mod chunked;
pub mod http;
pub mod socket;
pub mod tls;

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

/// The official one is in std
pub trait AsFd {
    fn as_fd(&self) -> i32;
}

impl AsFd for Vec<u8> {
    fn as_fd(&self) -> i32 {
        42
    }
}

impl AsFd for String {
    fn as_fd(&self) -> i32 {
        42
    }
}
