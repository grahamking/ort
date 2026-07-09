//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!
//! All the command line argument parsing

use core::str::FromStr;

extern crate alloc;
use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::Priority;
use crate::PromptOpts;
use crate::ReasoningEffort;
use crate::cli::Env;
use crate::common::utils;
use crate::{ErrorKind, ort_error};
use crate::{OrtError, syscall};

const MAX_CONCURRENT_MODELS: usize = 10;

/// Prefixing the system prompt or user prompt with this byte means it's a filename, read the
/// contents.
const FILE_INDICATOR: u8 = b'@';

pub struct ListOpts {
    pub config_file: Option<String>,
    pub is_json: bool,
}

pub enum Cmd {
    List(ListOpts),
    Prompt(crate::PromptOpts),
    Agent(crate::PromptOpts),
    ContinueConversation(crate::PromptOpts),
}

pub fn parse_prompt_args(
    args: &[String],
    stdin: Option<String>,
    env: &Env,
) -> Result<Cmd, ArgParseError> {
    // Only the prompt is required. Everything else can come from config file
    // or default.
    let mut prompt_parts: Vec<String> = Vec::new();

    let mut config_file = None;
    let mut models: Vec<String> = vec![];
    let mut system: Option<String> = None;
    let mut priority: Option<Priority> = None;
    let mut quiet: Option<bool> = None;
    let mut effort: Option<ReasoningEffort> = None;
    let mut show_reasoning: Option<bool> = None;
    let mut provider: Option<String> = None;
    let mut continue_conversation = false;
    let mut merge_config = true;
    let mut files: Vec<String> = vec![];
    let mut include_web_tools: Option<bool> = None;

    // If the prompt is '@<filename>' we save filename in here
    // Agent mode needs it
    let mut prompt_filename: Option<String> = None;

    let mut i = 1usize;

    let is_agent = if args[i] == "agent" {
        i += 1;
        true
    } else {
        false
    };

    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => {
                return Err(ArgParseError::show_help());
            }
            "--cfg" => {
                i += 1;
                if i >= args.len() {
                    return Err(ArgParseError::new_str("Missing value for -c"));
                }
                config_file = Some(args[i].clone());
                i += 1;
            }
            "-m" => {
                i += 1;
                if i >= args.len() {
                    return Err(ArgParseError::new_str("Missing value for -m"));
                }
                models.push(args[i].clone());
                if models.len() > MAX_CONCURRENT_MODELS {
                    return Err(ArgParseError::new_str("Too many '-m' flags, max 10"));
                }
                i += 1;
            }
            "-s" => {
                i += 1;
                if i >= args.len() {
                    return Err(ArgParseError::new_str("Missing value for -s"));
                }
                system = Some(args[i].clone());
                i += 1;
            }
            "-p" => {
                i += 1;
                if i >= args.len() {
                    return Err(ArgParseError::new_str("Missing value for -p"));
                }
                let val = args[i].clone();
                match val.as_str() {
                    // Safety: The 'parse' can handle exactly the three strings we match on
                    "price" | "throughput" | "latency" => priority = val.parse().ok(),
                    _ => {
                        return Err(ArgParseError::new_str(
                            "Invalid -p value: must be one of price|throughput|latency",
                        ));
                    }
                }
                i += 1;
            }
            "-q" => {
                quiet = Some(true);
                i += 1;
            }
            "-r" => {
                i += 1;
                let r_cfg = ReasoningEffort::from_str(args[i].as_str()).unwrap();
                effort = Some(r_cfg);
                i += 1;
            }
            "-rr" => {
                show_reasoning = Some(true);
                i += 1;
            }
            "-pr" => {
                i += 1;
                if i >= args.len() {
                    return Err(ArgParseError::new_str("Missing value for -pr"));
                }
                provider = Some(utils::slug(args[i].as_ref()));
                i += 1;
            }
            "-c" => {
                continue_conversation = true;
                i += 1;
            }
            "-nc" => {
                merge_config = false;
                i += 1;
            }
            "-ws" => {
                include_web_tools = Some(true);
                i += 1;
            }
            "-f" => {
                i += 1;
                if i >= args.len() {
                    return Err(ArgParseError::new_str("Missing value for -f"));
                }
                files.push(args[i].clone());
                i += 1;
            }
            s if s.starts_with('-') => {
                return Err(ArgParseError::new("Unknown flag: ".to_string() + s));
            }
            _ => {
                // First positional marks the start of the prompt; join the rest verbatim
                prompt_parts.extend_from_slice(&args[i..]);
                break;
            }
        }
    }

    let mut prompt = "".to_string();
    if !prompt_parts.is_empty() {
        prompt = prompt_parts.join(" ");
    };
    // If a prompt was piped in use it
    if let Some(stdin) = stdin {
        prompt.push_str("\n\n");
        prompt.push_str(&stdin);
    }

    if prompt.is_empty() {
        return Err(ArgParseError::new_str("Missing prompt."));
    };

    // Read system and user prompt from a file
    if prompt.bytes().next() == Some(FILE_INDICATOR) {
        let filename = &prompt[1..];
        prompt_filename = Some(filename.to_string());
        prompt = utils::filename_read_to_string(filename).map_err(ArgParseError::new_str)?;
    }
    if let Some(system_prompt) = system.as_ref()
        && system_prompt.bytes().next() == Some(FILE_INDICATOR)
    {
        let mut sp = utils::filename_read_to_string(&system_prompt[1..])
            .map_err(|err| ArgParseError::new("System prompt file: ".to_string() + err))?;
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
                    return Err(ArgParseError::new(
                        "Failed running `date` to substitute $DATE in system prompt: ".to_string()
                            + &err.as_string(),
                    ));
                }
            };
        }
        system = Some(sp);
    }

    let prompt_opts = PromptOpts {
        config_file,
        prompt: Some(prompt),
        models,
        provider,
        system,
        priority,
        effort,
        show_reasoning,
        quiet,
        merge_config,
        files,
        prompt_filename,
        include_web_tools,
    };
    if continue_conversation {
        Ok(Cmd::ContinueConversation(prompt_opts))
    } else if is_agent {
        Ok(Cmd::Agent(prompt_opts))
    } else {
        Ok(Cmd::Prompt(prompt_opts))
    }
}

pub fn parse_list_args(args: &[String]) -> Result<Cmd, ArgParseError> {
    let mut config_file = None;
    let mut is_json = false;

    let mut i = 2;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--cfg" => {
                i += 1;
                if i >= args.len() {
                    return Err(ArgParseError::new_str("Missing value for -c"));
                }
                config_file = Some(args[i].clone());
            }
            "-json" => is_json = true,
            x => {
                return Err(ArgParseError::new(
                    "Invalid list argument: ".to_string() + x,
                ));
            }
        }
        i += 1;
    }

    Ok(Cmd::List(ListOpts {
        config_file,
        is_json,
    }))
}

#[derive(Debug)]
pub struct ArgParseError {
    s: Cow<'static, str>,
    is_help: bool,
}

impl ArgParseError {
    pub fn new(s: String) -> Self {
        ArgParseError {
            s: Cow::Owned(s),
            is_help: false,
        }
    }

    pub fn new_str(s: &'static str) -> Self {
        ArgParseError {
            s: Cow::Borrowed(s),
            is_help: false,
        }
    }

    pub fn show_help() -> Self {
        ArgParseError {
            s: Cow::Borrowed(""),
            is_help: true,
        }
    }

    pub fn is_help(&self) -> bool {
        self.is_help
    }
}

impl From<ArgParseError> for OrtError {
    fn from(err: ArgParseError) -> OrtError {
        let _ = err;
        match err.s {
            Cow::Borrowed(static_str) => ort_error(ErrorKind::InvalidArguments, static_str),
            Cow::Owned(owned_str) => {
                // TODO: OrtError must be able to hold a String
                crate::utils::print_string(c"ArgParseError: ", &owned_str);
                ort_error(ErrorKind::InvalidArguments, "See above")
            }
        }
    }
}
