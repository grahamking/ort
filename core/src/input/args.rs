//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!
//! All the command line argument parsing

use core::fmt;

extern crate alloc;
use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::OrtError;
use crate::Priority;
use crate::PromptOpts;
use crate::ReasoningConfig;
use crate::ReasoningEffort;
use crate::ort_error;
use crate::slug;

#[derive(Debug)]
pub struct ListOpts {
    pub is_json: bool,
}

#[derive(Debug)]
pub enum Cmd {
    List(ListOpts),
    Prompt(crate::PromptOpts),
    ContinueConversation(crate::PromptOpts),
}

pub fn parse_prompt_args(args: &[String], stdin: Option<String>) -> Result<Cmd, ArgParseError> {
    // Only the prompt is required. Everything else can come from config file
    // or default.
    let mut prompt_parts: Vec<String> = Vec::new();

    let mut models: Vec<String> = vec![];
    let mut system: Option<String> = None;
    let mut priority: Option<Priority> = None;
    let mut quiet: Option<bool> = None;
    let mut reasoning: Option<ReasoningConfig> = None;
    let mut show_reasoning: Option<bool> = None;
    let mut provider: Option<String> = None;
    let mut continue_conversation = false;
    let mut merge_config = true;

    let mut i = 1usize;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => {
                return Err(ArgParseError::show_help());
            }
            "-m" => {
                i += 1;
                if i >= args.len() {
                    return Err(ArgParseError::new_str("Missing value for -m"));
                }
                models.push(args[i].clone());
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
                    "price" | "throughput" | "latency" => priority = Some(val.parse().unwrap()),
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
                let r_cfg = match args[i].as_str() {
                    "off" => ReasoningConfig {
                        enabled: false,
                        ..Default::default()
                    },
                    "none" => ReasoningConfig {
                        enabled: true,
                        effort: Some(ReasoningEffort::None),
                        ..Default::default()
                    },
                    "low" => ReasoningConfig {
                        enabled: true,
                        effort: Some(ReasoningEffort::Low),
                        ..Default::default()
                    },
                    "medium" | "med" => ReasoningConfig {
                        enabled: true,
                        effort: Some(ReasoningEffort::Medium),
                        ..Default::default()
                    },
                    "high" => ReasoningConfig {
                        enabled: true,
                        effort: Some(ReasoningEffort::High),
                        ..Default::default()
                    },
                    n_str => match n_str.parse::<u32>() {
                        Ok(n) => ReasoningConfig {
                            enabled: true,
                            tokens: Some(n),
                            ..Default::default()
                        },
                        Err(_) => {
                            return Err(ArgParseError::new_str(
                                "Invalid -r value. Must be off|low|medium|high|<num-tokens>",
                            ));
                        }
                    },
                };
                reasoning = Some(r_cfg);
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
                provider = Some(slug(args[i].as_ref()));
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
    let prompt_opts = PromptOpts {
        prompt: Some(prompt),
        models,
        provider,
        system,
        priority,
        reasoning,
        show_reasoning,
        quiet,
        merge_config,
    };
    if !continue_conversation {
        Ok(Cmd::Prompt(prompt_opts))
    } else {
        Ok(Cmd::ContinueConversation(prompt_opts))
    }
}

pub fn parse_list_args(args: &[String]) -> Result<Cmd, ArgParseError> {
    let mut is_json = false;

    let mut i = 2;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-json" => is_json = true,
            x => {
                return Err(ArgParseError::new(
                    "Invalid list argument: ".to_string() + x,
                ));
            }
        }
        i += 1;
    }

    Ok(Cmd::List(ListOpts { is_json }))
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
        ort_error(err.to_string())
    }
}

impl fmt::Display for ArgParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Argument parsing error: {}", self.s)
    }
}

impl core::error::Error for ArgParseError {}
