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

#[derive(Default, serde::Deserialize)]
#[allow(unused)]
pub struct ConfigFile {
    pub keys: Vec<ApiKey>,
    pub prompt_opts: Option<ort::PromptOpts>,
}

impl ConfigFile {
    pub fn get_openrouter_key(&self) -> Option<String> {
        self.keys
            .iter()
            .find_map(|ak| (ak.name == OPENROUTER_KEY).then(|| ak.value.clone()))
    }
}

#[derive(serde::Deserialize)]
pub struct ApiKey {
    name: String,
    value: String,
}

pub fn load() -> anyhow::Result<ConfigFile> {
    let config_dir = match env::var("XDG_CONFIG_HOME") {
        Ok(c) => PathBuf::from(c),
        _ => {
            let Some(home_dir) = std::env::home_dir() else {
                anyhow::bail!("Could not get home dir.");
            };
            home_dir.join(".config")
        }
    };
    let config_file = config_dir.join(CONFIG_FILE);
    match fs::read_to_string(&config_file) {
        Ok(cfg_str) => serde_json::from_str(&cfg_str).context("Failed to parse config"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ConfigFile::default()),
        Err(e) => Err(e).context(config_file.display().to_string()),
    }
}
