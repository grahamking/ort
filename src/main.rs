//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::fmt;
use std::borrow::Cow;
use std::env;
use std::process::ExitCode;

mod action_history;
mod action_list;
mod action_prompt;
mod config;
mod multi_channel;
mod writer;

#[derive(Debug)]
enum Cmd {
    List(ListOpts),
    Prompt(ort::PromptOpts),
    ContinueConversation(ort::CommonPromptOpts),
}

#[derive(Debug)]
struct ListOpts {
    is_json: bool,
}

fn print_usage_and_exit() -> ! {
    eprintln!(
        "Usage: ort [-m <model>] [-s \"<system prompt>\"] [-p <price|throughput|latency>] [-pr provider-slug] [-r] [-rr] [-q] <prompt>\n\
Defaults: -m {} ; -s omitted ; -p omitted\n\
Example:\n  ort -p price -m moonshotai/kimi-k2 -s \"Respond like a pirate\" \"Write a limerick about AI\"

See https://github.com/grahamking/ort for full docs.
",
        ort::DEFAULT_MODEL
    );
    std::process::exit(2);
}

#[derive(Debug)]
struct ArgParseError {
    s: Cow<'static, str>,
}

impl ArgParseError {
    pub fn new(s: String) -> Self {
        ArgParseError { s: Cow::Owned(s) }
    }

    pub fn new_str(s: &'static str) -> Self {
        ArgParseError {
            s: Cow::Borrowed(s),
        }
    }
}

impl fmt::Display for ArgParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Argument parsing error: {}", self.s)
    }
}

impl std::error::Error for ArgParseError {}

fn parse_args() -> Result<Cmd, ArgParseError> {
    let args: Vec<String> = env::args().collect();
    // args[0] is program name
    if args.len() == 1 {
        print_usage_and_exit();
    }

    if args[1].as_str() == "list" {
        action_list::parse_args(&args)
    } else {
        action_prompt::parse_args(&args)
    }
}

fn main() -> ExitCode {
    // Load ~/.config/ort.json
    let cfg = match config::load() {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("Failed loading config file: {err:#}");
            std::process::exit(1);
        }
    };

    // Fail fast if key missing
    let api_key = match env::var("OPENROUTER_API_KEY") {
        Ok(v) if !v.is_empty() => v,
        _ => match cfg.get_openrouter_key() {
            Some(k) => k,
            None => {
                eprintln!("OPENROUTER_API_KEY is not set.");
                std::process::exit(1);
            }
        },
    };

    let cmd = match parse_args() {
        Ok(cmd) => cmd,
        Err(err) => {
            eprintln!("{err}");
            print_usage_and_exit();
        }
    };

    let cmd_result = match cmd {
        Cmd::Prompt(mut cli_opts) => {
            let save_to_file = cfg.save_to_file();
            cli_opts.merge(cfg.prompt_opts.unwrap_or_default());
            action_prompt::run(&api_key, save_to_file, cli_opts)
        }
        Cmd::ContinueConversation(opts) => action_history::run_continue(&api_key, opts),
        Cmd::List(args) => action_list::run(&api_key, args),
    };
    match cmd_result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}
