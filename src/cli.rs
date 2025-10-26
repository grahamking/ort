//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::fmt;
use std::borrow::Cow;
use std::env;
use std::io;
use std::process::ExitCode;

use crate::OrtResult;
use crate::PromptOpts;
use crate::action_history;
use crate::action_list;
use crate::action_prompt;
use crate::ort_err;

#[derive(Debug)]
pub enum Cmd {
    List(ListOpts),
    Prompt(crate::PromptOpts),
    ContinueConversation(crate::PromptOpts),
}

#[derive(Debug)]
pub struct ListOpts {
    pub is_json: bool,
}

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

#[derive(Debug)]
pub struct ArgParseError {
    s: Cow<'static, str>,
    pub(crate) is_help: bool,
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
}

impl fmt::Display for ArgParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Argument parsing error: {}", self.s)
    }
}

impl std::error::Error for ArgParseError {}

fn parse_args(args: Vec<String>) -> Result<Cmd, ArgParseError> {
    // args[0] is program name
    if args.len() == 1 {
        return Err(ArgParseError::show_help());
    }

    if args[1].as_str() == "list" {
        action_list::parse_args(&args)
    } else {
        action_prompt::parse_args(&args)
    }
}

pub fn main(args: Vec<String>, is_terminal: bool, w: impl io::Write + Send) -> OrtResult<ExitCode> {
    // Load ~/.config/ort.json
    let cfg = crate::config::load()?;

    // Fail fast if key missing
    let api_key = match env::var("OPENROUTER_API_KEY") {
        Ok(v) if !v.is_empty() => v,
        _ => match cfg.get_openrouter_key() {
            Some(k) => k,
            None => {
                return ort_err("OPENROUTER_API_KEY is not set.");
            }
        },
    };

    let cmd = match parse_args(args) {
        Ok(cmd) => cmd,
        Err(err) if err.is_help => {
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
            action_prompt::run(
                &api_key,
                cancel_token,
                cfg.settings.unwrap_or_default(),
                cli_opts,
                messages,
                !is_terminal,
                w,
            )
        }
        Cmd::ContinueConversation(cli_opts) => action_history::run_continue(
            &api_key,
            cancel_token,
            cfg.settings.unwrap_or_default(),
            cli_opts,
            !is_terminal,
            w,
        ),
        Cmd::List(args) => action_list::run(
            &api_key,
            cancel_token,
            cfg.settings.unwrap_or_default(),
            args,
            w,
        ),
    };
    cmd_result.map(|_| ExitCode::SUCCESS)
}
