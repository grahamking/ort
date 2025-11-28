//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

use crate::common::data::PromptOpts;

const OPENROUTER_KEY: &str = "openrouter";

const DEFAULT_SAVE_TO_FILE: bool = true;

#[derive(Default)]
pub struct ConfigFile {
    pub settings: Option<Settings>,
    pub keys: Vec<ApiKey>,
    pub prompt_opts: Option<PromptOpts>,
}

impl ConfigFile {
    pub fn get_openrouter_key(&self) -> Option<String> {
        self.keys
            .iter()
            .find_map(|ak| (ak.name == OPENROUTER_KEY).then(|| ak.value.clone()))
    }
    pub fn _save_to_file(&self) -> bool {
        self.settings
            .as_ref()
            .map(|s| s.save_to_file)
            .unwrap_or(DEFAULT_SAVE_TO_FILE)
    }
}

#[derive(Debug, PartialEq)]
pub struct Settings {
    /// Yes to persist to a file in ~/.cache/ort to allow `-c` flag (continue)
    pub save_to_file: bool,
    /// IP addresses of openrouter.ai. Saves time resolving them.
    pub dns: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            save_to_file: DEFAULT_SAVE_TO_FILE,
            dns: Vec::new(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct ApiKey {
    name: String,
    value: String,
}

impl ApiKey {
    pub fn new(name: String, value: String) -> Self {
        ApiKey { name, value }
    }
}
