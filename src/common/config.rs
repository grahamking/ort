//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::str::FromStr;

extern crate alloc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{ErrorKind, OrtResult, cli::Env, common::utils, ort_error};
use crate::{Priority, ReasoningEffort};

/// Needed for the "-c" continue option to work so default enable.
/// Disable it for privacy / diskless.
const DEFAULT_SAVE_TO_FILE: bool = true;

/// Quiet disables showing the stats. I love the stats!
const DEFAULT_QUIET: bool = false;

/// Don't show reasoning by default, because if there are words I have to
/// read them, and I just want the answer.
const DEFAULT_SHOW_REASONING: bool = false;

/// Allowing the model to search is very very useful, but it makes responses
/// slower, so make it opt-in.
const DEFAULT_INCLUDE_WEB_TOOLS: bool = false;

/*
pub fn load_config(env: &Env, filename: &'static str) -> OrtResult<ConfigFile> {
    match read_config_file(env, filename)? {
        Some(cfg_str) => {
            ConfigFile::from_json(&cfg_str).map_err(|_| ort_error(ErrorKind::ConfigParseFailed, ""))
        }
        None => Ok(ConfigFile::default()),
    }
}
*/

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
#[derive(Clone, Default)]
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

    /// Default model. Usually passed on the cmd line as '-m <model_id>'
    pub model: Option<String>,

    /// System prompt if not given at the cmd line
    pub system_prompt: Option<String>,

    /// Quiet means don't show stats at the end. Helpful for scripts / pipelines
    pub quiet: bool,

    /// Show reasoning output. -rr on the cmd line.
    pub show_reasoning: bool,

    /// Preferred provider slug.
    pub provider: Option<String>,

    /// How to choose a provider: price, latency, throughput
    pub priority: Option<Priority>,

    /// Include web_search and web_fetch server-side tools
    pub include_web_tools: bool,

    /// How much thinking to do. -r flag.
    pub effort: Option<ReasoningEffort>,
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
        let mut base_url = None;
        let mut save_to_file = DEFAULT_SAVE_TO_FILE;
        let mut dns = Vec::new();
        let mut model = None;
        let mut system_prompt = None;
        let mut quiet = DEFAULT_QUIET;
        let mut show_reasoning = DEFAULT_SHOW_REASONING;
        let mut provider = None;
        let mut priority = None;
        let mut include_web_tools = DEFAULT_INCLUDE_WEB_TOOLS;
        let mut effort = None;

        for line in cfg.lines().filter(|l| !l.trim().is_empty()) {
            let (key, value) = line
                .split_once(":")
                .map(|(k, v)| (k.trim(), v.trim()))
                .unwrap();
            match key {
                "api_key" => api_key = Some(value.to_string()),
                "base_url" => base_url = Some(value.to_string()),
                "save_to_file" => save_to_file = value == "true",
                "dns" => {
                    dns = value.split(",").map(|ip| ip.trim().to_string()).collect();
                }
                "model" => model = Some(value.to_string()),
                "system_prompt" => system_prompt = Some(value.to_string()),
                "quiet" => quiet = value == "true",
                "show_reasoning" => show_reasoning = value == "true",
                "provider" => provider = Some(value.to_string()),
                "priority" => {
                    let p = Priority::from_str(value).map_err(|_| {
                        ort_error(
                            ErrorKind::ConfigParseFailed,
                            "Invalid priority field. Must be price, latency or throughput",
                        )
                    })?;
                    priority = Some(p);
                }
                "effort" => {
                    let r = ReasoningEffort::from_str(value).map_err(|_| {
                        ort_error(
                            ErrorKind::ConfigParseFailed,
                            "Invalid effort field. Must be low, medium, high, etc.",
                        )
                    })?;
                    effort = Some(r);
                }
                "include_web_tools" => include_web_tools = value == "true",
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
        let Some(base_url) = base_url else {
            return Err(ort_error(ErrorKind::MissingBaseURL, ""));
        };
        Ok(Cfg {
            base_url,
            api_key,
            save_to_file,
            dns,
            model,
            system_prompt,
            quiet,
            show_reasoning,
            priority,
            provider,
            include_web_tools,
            effort,
        })
    }

    pub fn default() -> Cfg {
        Cfg {
            base_url: "openrouter.ai/api/v1".to_string(),
            save_to_file: DEFAULT_SAVE_TO_FILE,
            dns: Vec::new(),
            quiet: DEFAULT_QUIET,
            show_reasoning: DEFAULT_SHOW_REASONING,
            include_web_tools: DEFAULT_INCLUDE_WEB_TOOLS,
            ..Default::default()
        }
    }

    pub fn get_api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
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

    use crate::ReasoningEffort;

    use super::*;

    #[test]
    fn cfg_file() {
        let s = r#"
api_key: THE-KEY
base_url: openrouter.ai/api/v1
save_to_file: false
dns: 104.18.2.115, 104.18.3.115
model: openai/gpt-oss-20b:free
system_prompt: Make your answer concise but complete. No yapping. Direct professional tone. No emoji.
quiet: false
show_reasoning: true
provider: openai
priority: price
include_web_tools: true
effort: low
"#;
        let cfg = Cfg::from_str(s).unwrap();
        assert_eq!(cfg.base_url, "openrouter.ai/api/v1");
        assert_eq!(cfg.api_key.as_deref(), Some("THE-KEY"));
        assert!(!cfg.save_to_file);

        assert_eq!(cfg.dns.len(), 2);
        for ip in cfg.dns {
            assert!(ip == "104.18.2.115" || ip == "104.18.3.115");
        }

        assert_eq!(cfg.model.as_deref(), Some("openai/gpt-oss-20b:free"));
        assert_eq!(
            cfg.system_prompt.as_deref(),
            Some(
                "Make your answer concise but complete. No yapping. Direct professional tone. No emoji."
            )
        );
        assert!(!cfg.quiet);
        assert!(cfg.show_reasoning);
        assert_eq!(cfg.provider.as_deref(), Some("openai"));
        assert_eq!(cfg.priority, Some(Priority::Price));
        assert!(cfg.include_web_tools);
        assert_eq!(cfg.effort, Some(ReasoningEffort::Low));
    }
}
