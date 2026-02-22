//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::CStr;

use crate::{ErrorKind, OrtResult, PromptOpts, common::utils, ort_error};

const DEFAULT_SAVE_TO_FILE: bool = true;

pub fn load_config(filename: &'static str) -> OrtResult<ConfigFile> {
    let mut config_file = [0u8; 64];

    // Write the config directory into `config_file`
    let mut end = xdg_dir(c"XDG_CONFIG_HOME", ".config", &mut config_file)?;
    config_file[end] = b'/';
    end += 1;
    let start = end;
    end += filename.len();
    config_file[start..end].copy_from_slice(filename.as_bytes());

    let config_file = unsafe { str::from_utf8_unchecked(&config_file[..end]) };
    match utils::filename_read_to_string(config_file) {
        Ok(cfg_str) => {
            ConfigFile::from_json(&cfg_str).map_err(|_| ort_error(ErrorKind::ConfigParseFailed, ""))
        }
        Err("NOT FOUND") => Ok(ConfigFile::default()),
        Err(_e) => Err(ort_error(ErrorKind::ConfigReadFailed, "")),
    }
}

#[derive(Default)]
pub struct ConfigFile {
    pub settings: Option<Settings>,
    pub keys: Vec<ApiKey>,
    pub prompt_opts: Option<PromptOpts>,
}

impl ConfigFile {
    /// Get the API key. There should only be one.
    pub fn get_api_key(&self) -> Option<String> {
        self.keys.first().as_ref().map(|k| k.value.clone())
    }
    pub fn _save_to_file(&self) -> bool {
        self.settings
            .as_ref()
            .map(|s| s.save_to_file)
            .unwrap_or(DEFAULT_SAVE_TO_FILE)
    }
}

#[derive(Debug)]
pub struct Settings {
    /// Yes to persist to a file in ~/.cache/ort to allow `-c` flag (continue)
    pub save_to_file: bool,
    /// IP addresses of openrouter.ai or integrate.api.nvidia.com.
    /// Saves time resolving them.
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

// The unit tests in output/from_json.rs need PartialEq
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

pub fn cache_dir() -> OrtResult<String> {
    let mut cache_dir = [0u8; 64];
    let mut end = xdg_dir(c"XDG_CACHE_HOME", ".cache", &mut cache_dir)?;
    cache_dir[end] = b'/';
    end += 1;
    let start = end;
    end += 3;
    cache_dir[start..end].copy_from_slice("ort".as_bytes());

    let cache_string = String::from_utf8_lossy(&cache_dir[..end]).into_owned();
    utils::ensure_dir_exists(&cache_string);
    Ok(cache_string)
}

/// A standard XDG directory based on environment variable, or default.
/// Writes the result into `target` and returns the length of the written string.
pub fn xdg_dir(var_name: &CStr, default: &'static str, target: &mut [u8]) -> OrtResult<usize> {
    let dir = utils::get_env(var_name);
    if !dir.is_empty() {
        // If it's in the env var, we assume the dir exists
        // Safety: to_str() will panic if the env var is not valid UTF-8
        let dir_len = dir.count_bytes();
        target[..dir_len + 1].copy_from_slice(dir.to_bytes_with_nul());
        return Ok(dir_len + 1);
    }

    let home_dir = utils::get_env(c"HOME");
    if !home_dir.is_empty() {
        let mut start = 0;
        let mut end = home_dir.count_bytes();
        target[start..end].copy_from_slice(home_dir.to_bytes());
        target[end] = b'/';
        end += 1;
        start = end;
        end += default.len();
        target[start..end].copy_from_slice(default.as_bytes());
        Ok(end)
    } else {
        Err(ort_error(
            ErrorKind::MissingHomeDir,
            "Could not get home dir. Is $HOME set?",
        ))
    }
}
