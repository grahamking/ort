//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::ffi::c_int;
use std::io;
use std::io::Read as _;
use std::process::ExitCode;

use super::{list, prompt};
use crate::OrtResult;
use crate::PromptOpts;
use crate::get_env;
use crate::load_config;
use crate::ort_err;
use crate::{ArgParseError, Cmd, parse_list_args, parse_prompt_args};

const STDIN_FILENO: i32 = 0;

pub fn print_usage() {
    eprintln!(
        "Usage: ort [-m <model>] [-s \"<system prompt>\"] [-p <price|throughput|latency>] [-pr provider-slug] [-r] [-rr] [-q] [-nc] <prompt>\n\
Defaults: -m {} ; -s omitted ; -p omitted\n\
Example:\n  ort -p price -m moonshotai/kimi-k2 -s \"Respond like a pirate\" \"Write a limerick about AI\"

See https://github.com/grahamking/ort for full docs.
",
        crate::DEFAULT_MODEL
    );
}

fn parse_args(args: Vec<String>) -> Result<Cmd, ArgParseError> {
    // args[0] is program name
    if args.len() == 1 {
        return Err(ArgParseError::show_help());
    }

    if args[1].as_str() == "list" {
        parse_list_args(&args)
    } else {
        let is_pipe_input = unsafe { isatty(STDIN_FILENO) == 0 };
        let stdin = if is_pipe_input {
            let mut buffer = String::new();
            io::stdin().read_to_string(&mut buffer).unwrap();
            Some(buffer)
        } else {
            None
        };
        parse_prompt_args(&args, stdin)
    }
}

unsafe extern "C" {
    pub fn isatty(fd: c_int) -> c_int;
}

pub fn main(args: Vec<String>, is_terminal: bool, w: impl io::Write + Send) -> OrtResult<ExitCode> {
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
            return Ok(ExitCode::from(2));
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
            WriteConvertor(w),
        ),
    };
    cmd_result.map(|_| ExitCode::SUCCESS)
}

struct WriteConvertor<T: io::Write>(T);

impl<T: io::Write> crate::Write for WriteConvertor<T> {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        self.0.write(buf).map_err(crate::ort_from_err)
    }

    fn flush(&mut self) -> OrtResult<()> {
        let _ = self.0.flush();
        Ok(())
    }
}
