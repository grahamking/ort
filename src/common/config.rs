//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::common::json_parser::{JsonField, autoparser};
use crate::{ErrorKind, OrtResult, PromptOpts, cli::Env, common::utils, ort_error};

const DEFAULT_SAVE_TO_FILE: bool = true;

pub fn load_config(env: &Env, filename: &'static str) -> OrtResult<ConfigFile> {
    match read_config_file(env, filename)? {
        Some(cfg_str) => {
            ConfigFile::from_json(&cfg_str).map_err(|_| ort_error(ErrorKind::ConfigParseFailed, ""))
        }
        None => Ok(ConfigFile::default()),
    }
}

/// Read a file from the XDG config dir
pub fn read_config_file(env: &Env, filename: &str) -> OrtResult<Option<String>> {
    let mut config_file = [0u8; 64];

    // Write the config directory into `config_file`
    let mut end = xdg_dir(
        env.XDG_CONFIG_HOME.unwrap_or_default(),
        env.HOME.unwrap_or_default(),
        ".config",
        &mut config_file,
    )?;
    config_file[end] = b'/';
    end += 1;
    let start = end;
    end += filename.len();
    config_file[start..end].copy_from_slice(filename.as_bytes());

    let config_file = unsafe { str::from_utf8_unchecked(&config_file[..end]) };
    match utils::filename_read_to_string(config_file) {
        Ok(cfg_str) => Ok(Some(cfg_str)),
        Err("NOT FOUND") => Ok(None),
        Err(_e) => Err(ort_error(ErrorKind::ConfigReadFailed, "")),
    }
}

// Will replace ConfigFile
#[derive(Clone)]
pub struct Cfg {
    /// Address and path base of the server. "https://" is optional and implied.
    /// Include the "/v1". No trailing slash.
    /// e.g.
    /// - "openrouter.ai/api/v1"
    /// - "https://localhost:8000/v1"
    pub base_url: String,

    pub api_key: Option<String>,

    /// Yes to persist to a file in ~/.cache/ort to allow `-c` flag (continue)
    pub save_to_file: bool,

    /// IP addresses of domain in base_url (usually openrouter.ai).
    /// Saves time resolving them.
    pub dns: Vec<String>,
}

impl Cfg {
    pub fn load(env: &Env, filename: &str) -> OrtResult<Cfg> {
        match read_config_file(env, filename)? {
            Some(cfg_str) => Self::from_str(&cfg_str),
            None => Ok(Self::default()),
        }
    }

    pub fn from_str(cfg: &str) -> OrtResult<Cfg> {
        let mut api_key = None;
        let mut base_url = "";
        let mut save_to_file = DEFAULT_SAVE_TO_FILE;
        let mut dns = Vec::new();

        for line in cfg.lines().filter(|l| !l.trim().is_empty()) {
            let (key, value) = line
                .split_once(":")
                .map(|(k, v)| (k.trim(), v.trim()))
                .unwrap();
            match key {
                "api_key" => api_key = Some(value),
                "base_url" => base_url = value,
                "save_to_file" => save_to_file = value == "true",
                "dns" => {
                    dns = value.split(",").map(|ip| ip.trim().to_string()).collect();
                }
                _ => {
                    /*
                    return Err(ort_error(
                        ErrorKind::ConfigReadFailed,
                        "Invalid key in cfg file",
                    ));
                    */
                    // Temp while I port
                    continue;
                }
            }
        }
        Ok(Cfg {
            base_url: base_url.to_string(),
            api_key: api_key.map(|k| k.to_string()),
            save_to_file,
            dns,
        })
    }

    pub fn default() -> Cfg {
        Cfg {
            api_key: None,
            base_url: "openrouter.ai/api/v1".to_string(),
            save_to_file: DEFAULT_SAVE_TO_FILE,
            dns: Vec::new(),
        }
    }

    pub fn get_api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }
}

#[derive(Default)]
pub struct ConfigFile {
    pub prompt_opts: Option<PromptOpts>,
}

impl ConfigFile {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [JsonField::new_raw("prompt_opts")];
        autoparser(json, &mut fields)?;

        let prompt_opts = fields[0]
            .get_raw()
            .as_deref()
            .map(PromptOpts::from_json)
            .transpose()?;

        Ok(ConfigFile { prompt_opts })
    }
}

pub fn cache_dir(env: &Env, cache_dir: &mut [u8]) -> OrtResult<usize> {
    let mut end = xdg_dir(
        env.XDG_CACHE_HOME.unwrap_or_default(),
        env.HOME.unwrap_or_default(),
        ".cache",
        cache_dir,
    )?;
    cache_dir[end] = b'/';
    end += 1;
    let start = end;
    end += 3;
    cache_dir[start..end].copy_from_slice("ort".as_bytes());

    let cache_string = String::from_utf8_lossy(&cache_dir[..end]).into_owned();
    utils::ensure_dir_exists(&cache_string);
    Ok(end)
}

/// A standard XDG directory based on environment variable, or default.
/// Writes the result into `target` and returns the length of the written string.
pub fn xdg_dir(
    xdg_var_value: &str,
    home_dir: &str,
    default: &'static str,
    target: &mut [u8],
) -> OrtResult<usize> {
    // TODO: Pass Option instead of checking for empty
    if !xdg_var_value.is_empty() {
        // If it's in the env var, we assume the dir exists
        let dir_len = xdg_var_value.len();
        target[..dir_len + 1].copy_from_slice(xdg_var_value.as_bytes());
        return Ok(dir_len + 1);
    }

    if !home_dir.is_empty() {
        let mut start = 0;
        let mut end = home_dir.len();
        target[start..end].copy_from_slice(home_dir.as_bytes());
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

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;

    #[test]
    fn json_config_file() {
        let s = r#"
{
    "prompt_opts": {
        "model": "google/gemma-3n-e4b-it:free",
        "system": "Make your answer concise but complete. No yapping. Direct professional tone. No emoji.",
        "quiet": false,
        "show_reasoning": false,
        "reasoning": {
            "enabled": false
        }
    }
}
"#;
        let cfg = ConfigFile::from_json(s).unwrap();
        assert!(cfg.prompt_opts.is_some());
    }

    #[test]
    fn cfg_file() {
        let s = r#"
api_key: THE-KEY
base_url: openrouter.ai/api/v1
save_to_file: false
dns: 104.18.2.115, 104.18.3.115
"#;
        let cfg = Cfg::from_str(s).unwrap();
        assert_eq!(cfg.base_url, "openrouter.ai/api/v1");
        assert_eq!(cfg.api_key.as_deref(), Some("THE-KEY"));
        assert!(!cfg.save_to_file);
        assert_eq!(cfg.dns.len(), 2);
        for ip in cfg.dns {
            assert!(ip == "104.18.2.115" || ip == "104.18.3.115");
        }
    }
}
