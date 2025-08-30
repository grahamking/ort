//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use anyhow::Context as _;
use std::fs;
use std::{env, path::PathBuf};

const CONFIG_FILE: &str = "ort.json";
const OPENROUTER_KEY: &str = "openrouter";

const DEFAULT_SAVE_TO_FILE: bool = true;
const DEFAULT_VERIFY_CERTS: bool = false;

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
    pub fn _verify_certs(&self) -> bool {
        self.settings
            .as_ref()
            .map(|s| s.verify_certs)
            .unwrap_or(DEFAULT_VERIFY_CERTS)
    }
}

#[derive(Debug, PartialEq)]
pub struct Settings {
    /// Yes to persist to a file in ~/.cache/ort to allow `-c` flag (continue)
    pub save_to_file: bool,
    /// Yes to verify TLS certificates. Many people choose yes.
    pub verify_certs: bool,
    /// IP addresses of openrouter.ai. Saves time resolving them.
    pub dns: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            save_to_file: DEFAULT_SAVE_TO_FILE,
            verify_certs: DEFAULT_VERIFY_CERTS,
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

pub fn load() -> anyhow::Result<ConfigFile> {
    let config_dir = xdg_dir("XDG_CONFIG_HOME", ".config")?;
    let config_file = config_dir.join(CONFIG_FILE);
    match fs::read_to_string(&config_file) {
        Ok(cfg_str) => ConfigFile::from_json(&cfg_str)
            .map_err(|err| anyhow::anyhow!("Failed to parse config: {err}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ConfigFile::default()),
        Err(e) => Err(e).context(config_file.display().to_string()),
    }
}

pub fn cache_dir() -> anyhow::Result<PathBuf> {
    let cache_root = xdg_dir("XDG_CACHE_HOME", ".cache")?;
    let d = cache_root.join("ort");
    if !d.exists() {
        fs::create_dir_all(&d)?;
    }
    Ok(d)
}

fn xdg_dir(var_name: &'static str, default: &'static str) -> anyhow::Result<PathBuf> {
    match env::var(var_name) {
        Ok(c) => Ok(PathBuf::from(c)),
        _ => {
            let Some(home_dir) = std::env::home_dir() else {
                anyhow::bail!("Could not get home dir.");
            };
            Ok(home_dir.join(default))
        }
    }
}
