//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!
//! Core pieces used by both input/request and output/response paths.
//! Also general utlities even if only used by input or output.

extern crate alloc;
use alloc::string::String;

use crate::common::error::OrtResult;

pub mod config;
pub mod data;
pub mod error;
pub mod stats;

pub trait Flushable {
    fn flush(&mut self) -> OrtResult<()>;
}

impl Flushable for String {
    fn flush(&mut self) -> OrtResult<()> {
        Ok(())
    }
}

impl<T: Flushable + ?Sized> Flushable for &mut T {
    fn flush(&mut self) -> OrtResult<()> {
        (**self).flush()
    }
}
