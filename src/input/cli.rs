//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025-2026 Graham King

use core::ffi::{c_int, c_void};

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

use crate::OrtResult;
use crate::PromptOpts;
use crate::Write;
use crate::common::config;
use crate::common::utils;
use crate::common::{buf_read, site};
use crate::input::args;
use crate::libc;
use crate::list;
use crate::prompt;
use crate::{ErrorKind, ort_error};

const STDIN_FILENO: i32 = 0;
const STDERR_FILENO: i32 = 0;

// Keep default mode in sync with lib.rs DEFAULT_MODEL
const USAGE: &str = "Usage: ort [-m <model>] [-s \"<system prompt>\"] [-p <price|throughput|latency>] [-pr provider-slug] [-r] [-rr] [-q] [-nc] <prompt>\n\
Defaults: -m google/gemma-3n-e4b-it:free; -s omitted ; -p omitted\n\
Example:\n  ort -p price -m openai/gpt-oss-20b -r low -rr -s \"Respond like a pirate\" \"Write a limerick about AI\"

See https://github.com/grahamking/ort for full docs.
";

pub fn print_usage() {
    unsafe { libc::write(STDERR_FILENO, USAGE.as_ptr() as *const c_void, USAGE.len()) };
}

fn parse_args(args: &[String]) -> Result<args::Cmd, args::ArgParseError> {
    // args[0] is program name
    if args.len() == 1 {
        return Err(args::ArgParseError::show_help());
    }

    if args[1].as_str() == "list" {
        args::parse_list_args(args)
    } else {
        let is_pipe_input = unsafe { libc::isatty(STDIN_FILENO) == 0 };
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

pub fn main(args: &[String], is_terminal: bool, w: impl Write + Send) -> OrtResult<c_int> {
    let site = match args[0].split('/').next_back().unwrap() {
        "nrt" => site::NVIDIA,
        _ => site::OPENROUTER,
    };

    // Load ~/.config/ort.json or nrt.json
    let cfg = config::load_config(site.config_filename)?;

    // Fail fast if key missing
    let mut api_key = utils::get_env(site.api_key_env)
        .to_string_lossy()
        .into_owned();
    if api_key.is_empty() {
        api_key = match cfg.get_api_key() {
            Some(k) => k,
            None => {
                return Err(ort_error(
                    ErrorKind::MissingApiKey,
                    "OPENROUTER_API_KEY or NVIDIA_API_KEY is not set.",
                ));
            }
        }
    };

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

    let cancel_token = crate::CancelToken::init();

    let cmd_result = match cmd {
        args::Cmd::Prompt(mut cli_opts) => {
            if cli_opts.merge_config {
                cli_opts.merge(cfg.prompt_opts.unwrap_or_default());
            } else {
                cli_opts.merge(PromptOpts::default());
            }
            // A Message is quite small, an enum and two Option<String>.
            // Capacity 3 for:
            // - System message (optiona)
            // - User message (required)
            // - and the assistant message that LastWriter appends, to save a realloc.
            let mut messages = Vec::with_capacity(3);
            if let Some(sys) = cli_opts.system.take() {
                messages.push(crate::Message::system(sys));
            };
            let user_message = crate::Message::user(cli_opts.prompt.take().unwrap());
            messages.push(user_message);
            if cli_opts.models.len() == 1 {
                prompt::run(
                    &api_key,
                    cancel_token,
                    cfg.settings.unwrap_or_default(),
                    cli_opts,
                    site,
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
                    site,
                    messages,
                    w,
                )
            }
        }
        args::Cmd::ContinueConversation(cli_opts) => prompt::run_continue(
            &api_key,
            cancel_token,
            cfg.settings.unwrap_or_default(),
            cli_opts,
            site,
            !is_terminal,
            w,
        ),
        args::Cmd::List(args) => list::run(
            &api_key,
            cancel_token,
            cfg.settings.unwrap_or_default(),
            args,
            site,
            w,
        ),
    };
    cmd_result.map(|_| 0)
}
