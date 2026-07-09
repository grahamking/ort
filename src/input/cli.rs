//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025-2026 Graham King

use core::ffi::{c_int, c_void};

extern crate alloc;
use alloc::string::{String, ToString};

use crate::OrtResult;
use crate::PromptOpts;
use crate::Write;
use crate::common::config;
use crate::common::{buf_read, site};
use crate::input::agent;
use crate::input::args;
use crate::input::list;
use crate::input::prompt;
use crate::syscall;
use crate::{ErrorKind, ort_error};

const STDIN_FILENO: i32 = 0;
const STDERR_FILENO: i32 = 0;

// Keep default mode in sync with common/data.rs DEFAULT_MODEL
const USAGE: &str = "Usage: ort [-m <model>] [-s \"<system prompt>\"] [-p <price|throughput|latency>] [-pr provider-slug] [-r] [-rr] [-q] [-nc] [-ws] <prompt>\n\
Defaults: -m nvidia/nemotron-3-super-120b-a12b:free -s omitted ; -p omitted\n\
Example:\n  ort -p price -m openai/gpt-oss-20b -r low -rr -s \"Respond like a pirate\" \"Write a limerick about AI\"

See https://github.com/grahamking/ort for full docs.
";

pub fn print_usage() {
    syscall::write(STDERR_FILENO, USAGE.as_ptr() as *const c_void, USAGE.len());
}

/// The environment variables we use
#[allow(nonstandard_style)]
#[derive(Default)]
pub struct Env {
    pub HOME: Option<&'static str>,
    pub PWD: Option<&'static str>,
    pub TMUX_PANE: Option<&'static str>,
    pub XDG_CONFIG_HOME: Option<&'static str>,
    pub XDG_CACHE_HOME: Option<&'static str>,
    pub OPENROUTER_API_KEY: Option<&'static str>,
    pub NVIDIA_API_KEY: Option<&'static str>,
}

fn parse_args(args: &[String], env: &Env) -> Result<args::Cmd, args::ArgParseError> {
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
        args::parse_prompt_args(args, stdin, env)
    }
}

pub fn main<W: Write + Send>(
    args: &[String],
    env: Env,
    is_terminal: bool,
    w: &mut W,
) -> OrtResult<c_int> {
    let site = match args[0].split('/').next_back().unwrap() {
        "nrt" => site::NVIDIA,
        "mrt" => site::MOCK,
        _ => site::OPENROUTER,
    };

    // Load ~/.config/ort/cfg
    // TODO: Flag to pass on cmd line: '-c ort-dev.cfg'
    let cfg = config::Cfg::load(&env, "ort.cfg")?;

    // Load ~/.config/ort.json or nrt.json
    let old_cfg = config::load_config(&env, site.config_filename)?;

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

    let cmd = match parse_args(args, &env) {
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

    let cmd_result = match cmd {
        args::Cmd::Prompt(mut cli_opts) => {
            if cli_opts.merge_config {
                cli_opts.merge(old_cfg.prompt_opts.unwrap_or_default());
            } else {
                cli_opts.merge(PromptOpts::default());
            }
            let messages = cli_opts.messages()?;
            if cli_opts.models.len() == 1 {
                prompt::run(
                    &api_key,
                    &cfg,
                    &old_cfg.settings.unwrap_or_default(),
                    &env,
                    cli_opts,
                    messages,
                    alloc::vec![],
                    !is_terminal,
                    w,
                )
            } else {
                prompt::run_multi(
                    &api_key,
                    &cfg,
                    &old_cfg.settings.unwrap_or_default(),
                    cli_opts,
                    messages,
                    w,
                )
            }
        }
        args::Cmd::Agent(mut cli_opts) => {
            if cli_opts.merge_config {
                cli_opts.merge(old_cfg.prompt_opts.unwrap_or_default());
            } else {
                cli_opts.merge(PromptOpts::default());
            }
            // Agent mode always includes server-side web tools
            cli_opts.include_web_tools = Some(true);
            let messages = cli_opts.messages()?;
            agent::run(
                &api_key,
                &cfg,
                &old_cfg.settings.unwrap_or_default(),
                &env,
                cli_opts,
                messages,
                w,
            )
        }
        args::Cmd::ContinueConversation(cli_opts) => prompt::run_continue(
            &api_key,
            &cfg,
            &old_cfg.settings.unwrap_or_default(),
            &env,
            cli_opts,
            !is_terminal,
            w,
        ),
        args::Cmd::List(args) => list::run(
            &api_key,
            &cfg,
            old_cfg.settings.unwrap_or_default(),
            args,
            w,
        ),
    };
    cmd_result.map(|_| 0)
}
