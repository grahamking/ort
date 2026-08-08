//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025-2026 Graham King

use core::ffi::{c_int, c_void};

extern crate alloc;
use alloc::string::{String, ToString};

use crate::OrtResult;
use crate::Write;
use crate::common::buf_read;
use crate::common::config;
use crate::common::config::Cfg;
use crate::input::args;
use crate::input::args::Cmd;
use crate::input::args::PromptOpts;
use crate::input::list;
use crate::input::prompt;
use crate::output::last_writer;
use crate::syscall;
use crate::{ErrorKind, ort_error};

const STDIN_FILENO: i32 = 0;
const STDERR_FILENO: i32 = 0;

// Keep default mode in sync with common/data.rs DEFAULT_MODEL
const USAGE: &str = "Usage: ort [--cfg ort.cfg] [-m <model>] [-s \"<system prompt>\"] [-p <price|throughput|latency>] [-pr provider-slug] [-r] [-rr] [-q] [-nc] [-ws] <prompt>\n\
Defaults: -m nvidia/nemotron-3-super-120b-a12b:free -s omitted ; -p omitted\n\
Example:\n  ort -p price -m openai/gpt-oss-20b -r low -rr -s \"Respond like a pirate\" \"Write a limerick about AI\"

See https://github.com/grahamking/ort for full docs.
";

pub fn print_usage() {
    syscall::write(STDERR_FILENO, USAGE.as_ptr() as *const c_void, USAGE.len());
}

/// The environment variables we use
#[allow(nonstandard_style)]
#[derive(Default, Clone)]
pub struct Env {
    pub HOME: Option<&'static str>,
    pub PWD: Option<&'static str>,
    pub TMUX_PANE: Option<&'static str>,
    pub XDG_CONFIG_HOME: Option<&'static str>,
    pub XDG_CACHE_HOME: Option<&'static str>,
    pub OPENROUTER_API_KEY: Option<&'static str>,
    pub NVIDIA_API_KEY: Option<&'static str>,
}

fn parse_args(args: &[String]) -> Result<args::Cmd, args::ArgParseError> {
    // args[0] is program name
    if args.len() == 1 {
        return Err(args::ArgParseError::show_help());
    }

    if args[1].as_str() == "list" {
        args::parse_list_args(args)
    } else {
        let is_pipe_input = !syscall::isatty(STDIN_FILENO);
        let stdin = if is_pipe_input {
            let mut buffer = String::with_capacity(8 * 1024);
            buf_read::fd_read_to_string(STDIN_FILENO, &mut buffer);
            Some(buffer)
        } else {
            None
        };
        args::parse_prompt_args(args, stdin)
    }
}

pub fn main<W: Write + Send>(
    args: &[String],
    env: Env,
    is_terminal: bool,
    w: &mut W,
) -> OrtResult<c_int> {
    let cmd = match parse_args(args) {
        Ok(cmd) => cmd,
        Err(err) if err.is_help() => {
            print_usage();
            return Ok(0);
        }
        Err(err) => {
            print_usage();
            return Err(err.into());
        }
    };
    let config_file = match &cmd {
        Cmd::List(opts) => opts.config_file.as_deref(),
        Cmd::Prompt(opts) | Cmd::ContinueConversation(opts) => opts.config_file.as_deref(),
    };

    let mut cfg = config::Cfg::load(&env, config_file.unwrap_or("ort.cfg"))?;

    // Fail fast if key missing
    let api_key_ref = env.OPENROUTER_API_KEY.unwrap_or_default();
    let mut api_key = api_key_ref.to_string();
    if api_key.is_empty() {
        api_key = match cfg.get_api_key() {
            Some(k) => k.to_string(),
            None => {
                return Err(ort_error(
                    ErrorKind::MissingApiKey,
                    "api_key not in ort.cfg and OPENROUTER_API_KEY is not set.",
                ));
            }
        }
    };

    let cmd_result = match cmd {
        args::Cmd::Prompt(cli_opts) => {
            override_config_from_cli(&mut cfg, cli_opts.clone());
            cfg.setup(&env)?;

            let messages = cfg.messages()?;
            if cli_opts.models.len() == 1 {
                prompt::run(&api_key, &cfg, &env, messages, !is_terminal, w)
            } else {
                prompt::run_multi(&api_key, &cfg, &env, cli_opts, messages, w)
            }
        }
        args::Cmd::ContinueConversation(cli_opts) => {
            let new_prompt = cli_opts.prompt.clone().unwrap();

            // Use the config we used last time
            let mut prev_cfg = last_writer::last_cfg(&env)?;

            // CLI still overrides the last used config
            override_config_from_cli(&mut prev_cfg, cli_opts.clone());
            prev_cfg.setup(&env)?;

            prompt::run_continue(&api_key, &prev_cfg, &env, new_prompt, !is_terminal, w)
        }
        args::Cmd::List(args) => list::run(&api_key, &cfg, args, w),
    };
    cmd_result.map(|_| 0)
}

/// CLI opts always override the config
pub fn override_config_from_cli(cfg: &mut Cfg, cli_opts: PromptOpts) {
    if !cli_opts.models.is_empty() {
        cfg.models = cli_opts.models;
    }
    if let Some(prompt) = cli_opts.prompt {
        cfg.prompt = Some(prompt);
    }
    if let Some(sp) = cli_opts.system {
        cfg.system_prompt = Some(sp);
    }
    if let Some(quiet) = cli_opts.quiet {
        cfg.quiet = quiet;
    }
    if let Some(rr) = cli_opts.show_reasoning {
        cfg.show_reasoning = rr;
    }
    if let Some(pr) = cli_opts.provider {
        cfg.provider = Some(pr);
    }
    if let Some(p) = cli_opts.priority {
        cfg.priority = Some(p);
    }
    if let Some(ws) = cli_opts.include_web_tools {
        cfg.include_web_tools = ws;
    }
    if let Some(r) = cli_opts.effort {
        cfg.effort = Some(r);
    }
    if !cli_opts.files.is_empty() {
        cfg.files = cli_opts.files;
    }
    if let Some(private) = cli_opts.is_private {
        cfg.is_private = private;
    }
}
