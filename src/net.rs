//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

pub mod chunked;
pub mod http;
pub mod socket;
pub mod tls;

/// The official one is in std
pub trait AsFd {
    fn as_fd(&self) -> i32;
}
