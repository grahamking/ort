//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::common::json_parser::{JsonField, autoparser};
use crate::{ErrorKind, OrtResult, PromptOpts, cli::Env, common::utils, ort_error};

const DEFAULT_SAVE_TO_FILE: bool = true;

pub fn load_config(env: &Env, filename: &'static str) -> OrtResult<ConfigFile> {
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
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [
            JsonField::new_raw("settings"),
            JsonField::new_vec_raw("keys"),
            JsonField::new_raw("prompt_opts"),
        ];
        autoparser(json, &mut fields)?;

        let settings = fields[0]
            .get_raw()
            .as_deref()
            .map(Settings::from_json)
            .transpose()?;

        let mut keys = vec![];
        if let Some(keys_str) = fields[1].get_vec_raw() {
            for k in keys_str {
                keys.push(ApiKey::from_json(&k)?);
            }
        }

        let prompt_opts = fields[2]
            .get_raw()
            .as_deref()
            .map(PromptOpts::from_json)
            .transpose()?;

        Ok(ConfigFile {
            settings,
            keys,
            prompt_opts,
        })
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

impl Settings {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [
            JsonField::new_bool("save_to_file"),
            JsonField::new_vec_string("dns"),
        ];
        autoparser(json, &mut fields)?;

        let default = Settings::default();
        Ok(Settings {
            save_to_file: fields[0].get_bool().unwrap_or(default.save_to_file),
            dns: fields[1].get_vec_string().unwrap_or_default(),
        })
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
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [
            JsonField::new_simple_string("name"),
            JsonField::new_string("value"),
        ];
        autoparser(json, &mut fields)?;
        Ok(ApiKey::new(
            fields[0].get_string().expect("Missing ApiKey name"),
            fields[1].get_string().expect("Missing ApiKey value"),
        ))
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
    use alloc::string::ToString;

    use super::*;

    #[test]
    fn api_key() {
        let s = r#"{"name":"openrouter","value":"sk-or-v1-a123b456c789d012a345b8032470394876576573242374098174093274abcdef"}"#;
        let got = ApiKey::from_json(s).unwrap();
        let expect = ApiKey::new(
            "openrouter".to_string(),
            "sk-or-v1-a123b456c789d012a345b8032470394876576573242374098174093274abcdef".to_string(),
        );
        assert_eq!(got, expect);
    }

    #[test]
    fn settings() {
        let s = r#"{
    "save_to_file": true,
    "dns": ["104.18.2.115", "104.18.3.115"]
}"#;
        let settings = Settings::from_json(s).unwrap();
        assert!(settings.save_to_file);
        assert_eq!(settings.dns, ["104.18.2.115", "104.18.3.115"]);
    }

    #[test]
    fn config_file() {
        let s = r#"
{
    "keys": [{"name": "openrouter", "value": "sk-or-v1-abcd1234"}],
    "settings": {
        "save_to_file": true,
        "dns": ["104.18.2.115", "104.18.3.115"]
    },
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
        assert_eq!(cfg.keys.len(), 1);
        assert!(cfg.settings.is_some());
        assert!(cfg.prompt_opts.is_some());
    }
}
