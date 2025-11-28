//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::ffi::CStr;

use std::fs;
use std::path::PathBuf;

use crate::{ConfigFile, OrtError, OrtResult, get_env, ort_error, ort_from_err};

const CONFIG_FILE: &str = "ort.json";

pub fn load() -> OrtResult<ConfigFile> {
    let config_dir = xdg_dir(c"XDG_CONFIG_HOME", ".config")?;
    let config_file = config_dir.join(CONFIG_FILE);
    match fs::read_to_string(&config_file) {
        Ok(cfg_str) => ConfigFile::from_json(&cfg_str)
            .map_err(|err| ort_error(format!("Failed to parse config: {err}"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ConfigFile::default()),
        Err(e) => {
            let mut err: OrtError = ort_from_err(e);
            err.context(config_file.display().to_string());
            Err(err)
        }
    }
}

pub fn cache_dir() -> OrtResult<PathBuf> {
    let cache_root = xdg_dir(c"XDG_CACHE_HOME", ".cache")?;
    let d = cache_root.join("ort");
    if !d.exists() {
        fs::create_dir_all(&d).map_err(ort_from_err)?;
    }
    Ok(d)
}

fn xdg_dir(var_name: &CStr, default: &'static str) -> OrtResult<PathBuf> {
    let v = get_env(var_name);
    if !v.is_empty() {
        return Ok(PathBuf::from(v));
    }

    let v = get_env(c"HOME");
    if !v.is_empty() {
        let home_dir = PathBuf::from(v);
        Ok(home_dir.join(default))
    } else {
        Err(ort_error("Could not get home dir. Is $HOME set?"))
    }
}
