//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::fmt;
use std::borrow::Cow;
use std::env;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, OnceLock};

use ort::PromptOpts;

mod action_history;
mod action_list;
mod action_prompt;
mod multi_channel;

// How the Ctrl-C handler asks the prompt thread to stop.
// This gymnastics is necessary because a POSIX signal handler is very limited
// in terms of code it can safely run.

// Keep the Arc/Atomic alive, not otherwise used
static IS_RUNNING: OnceLock<Arc<AtomicBool>> = OnceLock::new();
// Signal handlers can't use Arc directly so they use this
static IS_RUNNING_PTR: AtomicPtr<AtomicBool> = AtomicPtr::new(std::ptr::null_mut());

#[derive(Debug)]
enum Cmd {
    List(ListOpts),
    Prompt(ort::PromptOpts),
    ContinueConversation(ort::PromptOpts),
}

#[derive(Debug)]
struct ListOpts {
    is_json: bool,
}

fn print_usage_and_exit() -> ! {
    eprintln!(
        "Usage: ort [-m <model>] [-s \"<system prompt>\"] [-p <price|throughput|latency>] [-pr provider-slug] [-r] [-rr] [-q] [-nc] <prompt>\n\
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

/// Ctrl-C handler
unsafe extern "C" fn sigint_handler(_sig: libc::c_int) {
    unsafe {
        let p = IS_RUNNING_PTR.load(Ordering::Acquire);
        if !p.is_null() {
            (*p).store(false, Ordering::SeqCst);
        }
    }
}

// Attach a Ctrl-C handler for clean shutdown, particularly switching back
// on the ANSI cursor.
// Returns a boolean that will toggle to false when we need to stop.
fn install_ctrl_c_handler() -> Arc<AtomicBool> {
    let is_running = Arc::new(AtomicBool::new(true));
    IS_RUNNING_PTR.store(
        Arc::as_ptr(&is_running) as *mut AtomicBool,
        Ordering::Release,
    );
    let _ = IS_RUNNING.set(is_running.clone()); // keep it alive
    unsafe {
        libc::signal(libc::SIGINT, sigint_handler as libc::sighandler_t);
    };
    is_running
}

fn main() -> ExitCode {
    // Load ~/.config/ort.json
    let cfg = match ort::config::load() {
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

    let is_running = install_ctrl_c_handler();

    let cmd_result = match cmd {
        Cmd::Prompt(mut cli_opts) => {
            if cli_opts.merge_config {
                cli_opts.merge(cfg.prompt_opts.unwrap_or_default());
            } else {
                cli_opts.merge(PromptOpts::default());
            }
            let mut messages = if let Some(sys) = cli_opts.system.take() {
                vec![ort::Message::system(sys)]
            } else {
                vec![]
            };
            messages.push(ort::Message::user(cli_opts.prompt.take().unwrap()));
            action_prompt::run(
                &api_key,
                is_running.clone(),
                cfg.settings.unwrap_or_default(),
                cli_opts,
                messages,
            )
        }
        Cmd::ContinueConversation(cli_opts) => action_history::run_continue(
            &api_key,
            is_running.clone(),
            cfg.settings.unwrap_or_default(),
            cli_opts,
        ),
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
