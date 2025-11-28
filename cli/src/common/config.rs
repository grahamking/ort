//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::fs;
use std::{env, path::PathBuf};

use crate::{OrtError, OrtResult, ort_error, ort_from_err};

const CONFIG_FILE: &str = "ort.json";
const OPENROUTER_KEY: &str = "openrouter";

const DEFAULT_SAVE_TO_FILE: bool = true;

#[derive(Default)]
pub struct ConfigFile {
    pub settings: Option<Settings>,
    pub keys: Vec<ApiKey>,
    pub prompt_opts: Option<crate::PromptOpts>,
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
            dns: vec![],
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

pub fn load() -> OrtResult<ConfigFile> {
    let config_dir = xdg_dir("XDG_CONFIG_HOME", ".config")?;
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
    let cache_root = xdg_dir("XDG_CACHE_HOME", ".cache")?;
    let d = cache_root.join("ort");
    if !d.exists() {
        fs::create_dir_all(&d).map_err(ort_from_err)?;
    }
    Ok(d)
}

fn xdg_dir(var_name: &'static str, default: &'static str) -> OrtResult<PathBuf> {
    match env::var(var_name) {
        Ok(c) => Ok(PathBuf::from(c)),
        _ => {
            let Some(home_dir) = std::env::home_dir() else {
                return Err(ort_error("Could not get home dir."));
            };
            Ok(home_dir.join(default))
        }
    }
}
