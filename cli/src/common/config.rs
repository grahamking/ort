//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::fs;

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::String;

use crate::{ConfigFile, OrtError, OrtResult, ort_error, ort_from_err, path_exists, xdg_dir};

const CONFIG_FILE: &str = "ort.json";

pub fn load() -> OrtResult<ConfigFile> {
    let mut config_dir = xdg_dir(c"XDG_CONFIG_HOME", ".config")?;
    config_dir.push('/');
    config_dir.push_str(CONFIG_FILE);
    let config_file = config_dir;
    match fs::read_to_string(&config_file) {
        Ok(cfg_str) => ConfigFile::from_json(&cfg_str)
            .map_err(|err| ort_error(format!("Failed to parse config: {err}"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ConfigFile::default()),
        Err(e) => {
            let mut err: OrtError = ort_from_err(e);
            err.context(config_file);
            Err(err)
        }
    }
}

pub fn cache_dir() -> OrtResult<String> {
    let mut cache_dir = xdg_dir(c"XDG_CACHE_HOME", ".cache")?;
    cache_dir.push('/');
    cache_dir.push_str("ort");
    let cs = CString::new(cache_dir.clone()).expect("Null bytes found in cache path, unlikely");
    if !path_exists(cs.as_ref()) {
        fs::create_dir_all(&cache_dir).map_err(ort_from_err)?;
    }
    Ok(cache_dir)
}
