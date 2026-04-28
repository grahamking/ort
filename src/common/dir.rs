//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::ffi::CStr;

extern crate alloc;
use alloc::string::String;
use alloc::vec::IntoIter;
use alloc::vec::Vec;

use crate::{ErrorKind, OrtResult, ort_error};

/// Iterator over the regular files in this directory.
pub struct DirFiles {
    inner: IntoIter<String>,
}

impl DirFiles {
    pub fn new(p: &CStr) -> OrtResult<Self> {
        let entries = std::fs::read_dir(p.to_string_lossy().as_ref())
            .map_err(|_| ort_error(ErrorKind::DirOpenFailed, "read_dir returned error"))?;
        let mut files = Vec::new();
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file() {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                files.push(name.to_string());
            }
        }
        Ok(DirFiles {
            inner: files.into_iter(),
        })
    }
}

impl Iterator for DirFiles {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}
