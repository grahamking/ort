//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::ffi::{c_int, c_void};

extern crate alloc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::OrtResult;
use crate::PromptOpts;
use crate::Write;
use crate::fd_read_to_string;
use crate::get_env;
use crate::libc;
use crate::list;
use crate::load_config;
use crate::ort_err;
use crate::prompt;
use crate::{ArgParseError, Cmd, parse_list_args, parse_prompt_args};

const STDIN_FILENO: i32 = 0;
const STDERR_FILENO: i32 = 0;

pub fn print_usage() {
    let usage = "Usage: ort [-m <model>] [-s \"<system prompt>\"] [-p <price|throughput|latency>] [-pr provider-slug] [-r] [-rr] [-q] [-nc] <prompt>\n\
Defaults: -m ".to_string() + crate::DEFAULT_MODEL +" ; -s omitted ; -p omitted\n\
Example:\n  ort -p price -m moonshotai/kimi-k2 -s \"Respond like a pirate\" \"Write a limerick about AI\"

See https://github.com/grahamking/ort for full docs.
";
    unsafe { libc::write(STDERR_FILENO, usage.as_ptr() as *const c_void, usage.len()) };
}

fn parse_args(args: Vec<String>) -> Result<Cmd, ArgParseError> {
    // args[0] is program name
    if args.len() == 1 {
        return Err(ArgParseError::show_help());
    }

    if args[1].as_str() == "list" {
        parse_list_args(&args)
    } else {
        let is_pipe_input = unsafe { libc::isatty(STDIN_FILENO) == 0 };
        let stdin = if is_pipe_input {
            let mut buffer = String::new();
            fd_read_to_string(STDIN_FILENO, &mut buffer);
            Some(buffer)
        } else {
            None
        };
        parse_prompt_args(&args, stdin)
    }
}

pub fn main(args: Vec<String>, is_terminal: bool, w: impl Write + Send) -> OrtResult<c_int> {
    // Load ~/.config/ort.json
    let cfg = load_config()?;

    // Fail fast if key missing
    let mut api_key = get_env(c"OPENROUTER_API_KEY");
    if api_key.is_empty() {
        api_key = match cfg.get_openrouter_key() {
            Some(k) => k,
            None => {
                return ort_err("OPENROUTER_API_KEY is not set.");
            }
        }
    };

    let cmd = match parse_args(args) {
        Ok(cmd) => cmd,
        Err(err) if err.is_help() => {
            print_usage();
            return Ok(2);
        }
        Err(err) => {
            print_usage();
            return Err(err.into());
        }
    };

    let cancel_token = crate::CancelToken::init();

    let cmd_result = match cmd {
        Cmd::Prompt(mut cli_opts) => {
            if cli_opts.merge_config {
                cli_opts.merge(cfg.prompt_opts.unwrap_or_default());
            } else {
                cli_opts.merge(PromptOpts::default());
            }
            let mut messages = if let Some(sys) = cli_opts.system.take() {
                vec![crate::Message::system(sys)]
            } else {
                vec![]
            };
            messages.push(crate::Message::user(cli_opts.prompt.take().unwrap()));
            if cli_opts.models.len() == 1 {
                prompt::run(
                    &api_key,
                    cancel_token,
                    cfg.settings.unwrap_or_default(),
                    cli_opts,
                    messages,
                    !is_terminal,
                    w,
                )
            } else {
                prompt::run_multi(
                    &api_key,
                    cancel_token,
                    cfg.settings.unwrap_or_default(),
                    cli_opts,
                    messages,
                    w,
                )
            }
        }
        Cmd::ContinueConversation(cli_opts) => prompt::run_continue(
            &api_key,
            cancel_token,
            cfg.settings.unwrap_or_default(),
            cli_opts,
            !is_terminal,
            w,
        ),
        Cmd::List(args) => list::run(
            &api_key,
            cancel_token,
            cfg.settings.unwrap_or_default(),
            args,
            w,
        ),
    };
    cmd_result.map(|_| 0)
}
