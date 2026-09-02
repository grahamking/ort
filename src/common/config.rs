//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::{fmt, str::FromStr};

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::common::error::ort_err;
use crate::common::file;
use crate::common::io::Write;
use crate::{Context, Message, Priority, ReasoningEffort, syscall};
use crate::{ErrorKind, OrtResult, cli::Env, common::utils, ort_error};

/// To use a different endpoint set `base_url` in `${XDG_CONFIG_HOME}/ort.cfg`
pub const DEFAULT_BASE_URL: &str = "openrouter.ai/api/v1";

/// Quiet disables showing the stats. I love the stats!
const DEFAULT_QUIET: bool = false;

/// Don't show reasoning by default, because if there are words I have to
/// read them, and I just want the answer.
const DEFAULT_SHOW_REASONING: bool = false;

/// Allowing the model to search is very very useful, but it makes responses
/// slower, so make it opt-in.
const DEFAULT_INCLUDE_WEB_TOOLS: bool = false;

/// Prefixing the system prompt or user prompt with this byte means it's
/// a filename, read the contents.
const FILE_INDICATOR: u8 = b'@';

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

/// Read a file from the XDG config dir. `filename` must not include path.
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
        Err(err) => {
            let msg = "Reading config file ".to_string() + config_file + " - " + err;
            Err(ort_err(ErrorKind::ConfigReadFailed, msg.into()))
        }
    }
}

#[derive(Clone, Default)]
pub struct Cfg {
    //
    // These are config file only
    //
    /// Address and path base of the server. "https://" is optional and implied.
    /// Include the "/v1". No trailing slash.
    /// e.g.
    /// - "openrouter.ai/api/v1"
    /// - "https://localhost:8000/v1"
    pub base_url: String,

    pub api_key: Option<String>,

    /// IP addresses of domain in base_url (usually openrouter.ai).
    /// Saves time resolving them.
    pub dns: Vec<String>,

    //
    // These are also on the command line
    //
    /// Default model. Usually passed on the cmd line as '-m <model_id>'
    /// Can be multiple comma separated.
    pub models: Vec<String>,

    /// Prompt if not given at the cmd line.
    /// Normally you would not set this.
    /// For automated processes you may want to have the prompt in the cfg
    /// file and check it in.
    pub prompt: Option<String>,

    // If the prompt is '@<filename>' we save filename in here.
    // Putting the prompt in a file allows us agent mode to watch it with `inotify`.
    pub prompt_filename: Option<String>,

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

    /// Images to attach to the request.
    pub files: Vec<String>,

    /// Do not record prompt on disk. Use if running diskless or for privacy.
    /// Disables the "-c" continue functionality, and the `jsonl` log
    pub is_private: bool,

    /// On disk only in last.cfg. Helps inference servers.
    /// Always populated in memory. New one for each new session.
    pub session_id: String,
}

impl Cfg {
    pub fn load(env: &Env, filename: &str) -> OrtResult<Cfg> {
        match read_config_file(env, filename)? {
            Some(cfg_str) => Self::from_str(&cfg_str),
            None => Ok(Self::default()),
        }
    }

    /// Initial chat completions messages to send.
    /// Includes the system prompt, regular prompt, and any attache files.
    pub fn messages(&mut self) -> OrtResult<Vec<Message>> {
        // A Message is quite small, an enum and two Option<String>.
        // Capacity 3 for:
        // - System message (optional)
        // - User message (required)
        // - and the assistant message that LastWriter appends, to save a realloc.
        let mut messages = Vec::with_capacity(3);
        if let Some(sys) = self.system_prompt.clone() {
            messages.push(crate::Message::system(sys));
        };
        let user_message = if self.files.is_empty() {
            crate::Message::user(self.prompt.clone().unwrap())
        } else {
            crate::Message::with_files(self.prompt.take().unwrap(), &self.files)?
        };
        messages.push(user_message);
        Ok(messages)
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(cfg: &str) -> OrtResult<Cfg> {
        let mut api_key = None;
        let mut base_url = DEFAULT_BASE_URL.to_string();
        let mut dns = Vec::new();
        let mut models = Vec::new();
        let mut prompt = None;
        let mut system_prompt = None;
        let mut quiet = DEFAULT_QUIET;
        let mut show_reasoning = DEFAULT_SHOW_REASONING;
        let mut provider = None;
        let mut priority = None;
        let mut include_web_tools = DEFAULT_INCLUDE_WEB_TOOLS;
        let mut effort = None;
        let mut files = Vec::new();
        let mut is_private = false;
        let mut session_id = None;

        for line in cfg.lines().filter(|l| !l.trim().is_empty()) {
            if line.as_bytes()[0] == b'#' {
                // comment
                continue;
            }
            let (key, value) = line
                .split_once(":")
                .map(|(k, v)| (k.trim(), v.trim()))
                .unwrap();
            match key {
                "api_key" => api_key = Some(value.to_string()),
                "base_url" => base_url = value.to_string(),
                "dns" => {
                    dns = value.split(",").map(|ip| ip.trim().to_string()).collect();
                }
                "model" => {
                    models = value.split(",").map(|m| m.trim().to_string()).collect();
                }
                "files" => {
                    files = value.split(",").map(|f| f.trim().to_string()).collect();
                }
                "prompt" => prompt = Some(value.to_string()),
                "system_prompt" => system_prompt = Some(value.to_string()),
                "quiet" => quiet = value == "true",
                "show_reasoning" => show_reasoning = value == "true",
                "priority" => {
                    let p = Priority::from_str(value).map_err(|_| {
                        ort_error(
                            ErrorKind::ConfigParseFailed,
                            "Invalid priority field. Must be price, latency or throughput",
                        )
                    })?;
                    priority = Some(p);
                }
                "private" => is_private = value == "true",
                "provider" => provider = Some(value.to_string()),
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
                "session_id" => session_id = Some(value.to_string()),
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
            base_url,
            api_key,
            dns,
            models,
            prompt,
            system_prompt,
            quiet,
            show_reasoning,
            priority,
            provider,
            include_web_tools,
            effort,
            files,
            is_private,
            session_id: session_id.unwrap_or_else(utils::generate_session_id),
            // Resolved later
            prompt_filename: None,
        })
    }

    pub fn save(&self, path: [u8; 128], len: usize) -> OrtResult<()> {
        let mut f =
            unsafe { file::File::create(&path[..len + 1]).context("create last file config")? };
        let as_str = self.to_string();
        let _ = f.write_all(as_str.as_bytes());
        Ok(())
    }

    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Cfg {
        Cfg {
            base_url: DEFAULT_BASE_URL.to_string(),
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

    /// Resolve any `@filename.txt` prompts, and replace the `$DATE` system prompts
    /// variables.
    /// After this the Cfg is ready.
    /// Ideally merge override_config_from_cli into this too, but I don't want
    /// a dependency from common::Cfg to args::PromptOps.
    pub fn setup(&mut self, env: &Env) -> OrtResult<()> {
        self.load_prompts()?;
        self.fill_system_prompt_variables(env)?;
        Ok(())
    }

    /// If the prompt or system prompt start with `@` read them from that file.
    /// A missing prompt file is created empty, so that `art` (agent) can start.
    /// A missing system prompt file is an error.
    fn load_prompts(&mut self) -> OrtResult<()> {
        if let Some(p) = self.prompt.as_ref()
            && p.bytes().next() == Some(FILE_INDICATOR)
        {
            let filename = &p[1..];
            self.prompt_filename = Some(filename.to_string());
            match utils::filename_read_to_string(filename) {
                Ok(p) => self.prompt = Some(p),
                Err(_err) => {
                    // File does not exist, create it empty. art needs this.
                    let c_filename = CString::new(filename).map_err(|_| {
                        ort_err(
                            ErrorKind::ConfigParseFailed,
                            "prompt filename has null byte".into(),
                        )
                    })?;
                    let file = unsafe { file::File::create(c_filename.as_bytes())? };
                    file.close();
                }
            }
        }

        if let Some(system_prompt) = self.system_prompt.as_ref()
            && system_prompt.bytes().next() == Some(FILE_INDICATOR)
        {
            let filename = &system_prompt[1..];
            let sp = utils::filename_read_to_string(filename).map_err(|err| {
                let msg = "Invalid system prompt filename ".to_string() + filename + " - " + err;
                ort_err(ErrorKind::ConfigParseFailed, msg.into())
            })?;
            self.system_prompt = Some(sp);
        }

        Ok(())
    }

    /// System prompt variable substitution.
    /// `$PWD` -> current working directory
    /// `$DATE` -> output of `date` cmd
    fn fill_system_prompt_variables(&mut self, env: &Env) -> OrtResult<()> {
        let Some(mut sp) = self.system_prompt.take() else {
            // No system prompt
            return Ok(());
        };

        // System prompt variable substitution. PWD is current working directory.
        if let Some(pwd) = env.PWD {
            sp = sp.replace("$PWD", pwd);
        }

        // This one is more expensive so only do it if necessary
        if sp.contains("$DATE") {
            // Shelling to `date` is much simpler and shorter than converting kernel clock
            match syscall::system("date") {
                Ok(current_date) => sp = sp.replace("$DATE", &current_date.stdout),
                Err(err) => {
                    let msg = "Failed running `date` to substitute $DATE in system prompt: "
                        .to_string()
                        + &err.as_string();
                    return Err(ort_err(ErrorKind::FailedFillingSystemPrompt, msg.into()));
                }
            };
        }

        self.system_prompt = Some(sp);
        Ok(())
    }
}

fn write_csv(f: &mut fmt::Formatter<'_>, key: &str, values: &[String]) -> fmt::Result {
    if values.is_empty() {
        return Ok(());
    }

    write!(f, "{key}: ")?;
    for (idx, value) in values.iter().enumerate() {
        if idx > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{value}")?;
    }
    writeln!(f)
}

impl fmt::Display for Cfg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Don't reveal the API key
        //if let Some(api_key) = self.api_key.as_ref() {
        //    writeln!(f, "api_key: {api_key}")?;
        //}

        if !self.base_url.is_empty() && self.base_url != DEFAULT_BASE_URL {
            writeln!(f, "base_url: {}", self.base_url)?;
        }
        write_csv(f, "dns", &self.dns)?;
        write_csv(f, "model", &self.models)?;
        if let Some(prompt_filename) = self.prompt_filename.as_ref() {
            writeln!(f, "prompt: @{}", prompt_filename)?;
        } else if let Some(prompt) = self.prompt.as_ref() {
            writeln!(f, "prompt: {prompt}")?;
        }
        if let Some(system_prompt) = self.system_prompt.as_ref() {
            writeln!(f, "system_prompt: {system_prompt}")?;
        }
        writeln!(f, "quiet: {}", self.quiet)?;
        writeln!(f, "show_reasoning: {}", self.show_reasoning)?;
        if let Some(provider) = self.provider.as_ref() {
            writeln!(f, "provider: {provider}")?;
        }
        if let Some(priority) = self.priority {
            writeln!(f, "priority: {}", priority.as_str())?;
        }
        writeln!(f, "include_web_tools: {}", self.include_web_tools)?;
        if let Some(effort) = self.effort {
            writeln!(f, "effort: {}", effort.as_str())?;
        }
        write_csv(f, "files", &self.files)?;
        writeln!(f, "private: {}", self.is_private)?;
        writeln!(f, "session_id: {}", self.session_id)?;
        Ok(())
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
        target[..dir_len].copy_from_slice(xdg_var_value.as_bytes());
        return Ok(dir_len);
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

    use alloc::vec;

    use crate::ReasoningEffort;

    use super::*;

    #[test]
    fn cfg_file() {
        let s = r#"
api_key: THE-KEY
base_url: openrouter.ai/api/v1
dns: 104.18.2.115, 104.18.3.115
model: openai/gpt-oss-20b:free
system_prompt: Make your answer concise but complete. No yapping. Direct professional tone. No emoji.
quiet: false
show_reasoning: true
provider: openai
priority: price
include_web_tools: true
effort: low
private: false
"#;
        let cfg = Cfg::from_str(s).unwrap();
        assert_eq!(cfg.base_url, "openrouter.ai/api/v1");
        assert_eq!(cfg.api_key.as_deref(), Some("THE-KEY"));
        assert!(!cfg.is_private);

        assert_eq!(cfg.dns.len(), 2);
        for ip in cfg.dns {
            assert!(ip == "104.18.2.115" || ip == "104.18.3.115");
        }

        assert_eq!(cfg.models[0], "openai/gpt-oss-20b:free");
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

    #[test]
    fn cfg_file_to_string() {
        let cfg = Cfg {
            api_key: Some("THE-KEY".to_string()),
            base_url: "https://localhost:8000/v1".to_string(),
            dns: vec!["127.0.0.1".to_string(), "127.0.0.2".to_string()],
            models: vec!["openai/gpt-oss-20b:free".to_string()],
            prompt: Some("prompt text".to_string()),
            system_prompt: Some("system text".to_string()),
            quiet: false,
            show_reasoning: true,
            provider: Some("openai".to_string()),
            priority: Some(Priority::Price),
            include_web_tools: true,
            effort: Some(ReasoningEffort::Low),
            files: vec!["image.png".to_string(), "other.jpg".to_string()],
            is_private: false,
            session_id: "test".to_string(),
            ..Cfg::default()
        };

        let cfg_str = cfg.to_string();
        assert_eq!(
            cfg_str,
            "base_url: https://localhost:8000/v1\n\
dns: 127.0.0.1, 127.0.0.2\n\
model: openai/gpt-oss-20b:free\n\
prompt: prompt text\n\
system_prompt: system text\n\
quiet: false\n\
show_reasoning: true\n\
provider: openai\n\
priority: price\n\
include_web_tools: true\n\
effort: low\n\
files: image.png, other.jpg\n\
private: false\n\
session_id: test\n"
        );

        let parsed = Cfg::from_str(&cfg_str).unwrap();
        assert_eq!(parsed.base_url, cfg.base_url);
        assert_eq!(parsed.dns, cfg.dns);
        assert_eq!(parsed.models, cfg.models);
        assert_eq!(parsed.prompt, cfg.prompt);
        assert_eq!(parsed.system_prompt, cfg.system_prompt);
        assert_eq!(parsed.quiet, cfg.quiet);
        assert_eq!(parsed.show_reasoning, cfg.show_reasoning);
        assert_eq!(parsed.provider, cfg.provider);
        assert_eq!(parsed.priority, cfg.priority);
        assert_eq!(parsed.include_web_tools, cfg.include_web_tools);
        assert_eq!(parsed.effort, cfg.effort);
        assert_eq!(parsed.files, cfg.files);
        assert_eq!(parsed.is_private, cfg.is_private);
    }
}
