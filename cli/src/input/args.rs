//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!
//! All the command line argument parsing

use std::io;
use std::io::Read as _;

use crate::Priority;
use crate::PromptOpts;
use crate::ReasoningConfig;
use crate::ReasoningEffort;
use crate::cli::ArgParseError;
use crate::cli::Cmd;
use crate::common::utils;

const STDIN_FILENO: i32 = 0;

#[derive(Debug)]
pub struct ListOpts {
    pub is_json: bool,
}

pub fn parse_prompt_args(args: &[String]) -> Result<Cmd, ArgParseError> {
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
            s if s.starts_with('-') => {
                return Err(ArgParseError::new(format!("Unknown flag: {s}")));
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

    let is_pipe_input = unsafe { isatty(STDIN_FILENO) == 0 };
    if is_pipe_input {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer).unwrap();
        prompt.push_str("\n\n");
        prompt.push_str(&buffer);
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
                return Err(ArgParseError::new(format!("Invalid list argument: {x}")));
            }
        }
        i += 1;
    }

    Ok(Cmd::List(ListOpts { is_json }))
}

unsafe extern "C" {
    pub fn isatty(fd: i32) -> i32;
}
