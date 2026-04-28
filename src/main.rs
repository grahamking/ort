//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025-2026 Graham King

use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;

use ort_openrouter_cli::{StdoutWriter, cli};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if is_version_flag(&args) {
        return ExitCode::SUCCESS;
    }

    let env = collect_env();
    let is_terminal = io::stdout().is_terminal();

    match cli::main(&args, env, is_terminal, StdoutWriter {}) {
        Ok(exit_code) => (exit_code as u8).into(),
        Err(err) => {
            let _ = writeln!(io::stderr(), "ERROR: {}", err.as_string());
            1.into()
        }
    }
}

fn is_version_flag(args: &[String]) -> bool {
    if args.iter().any(|arg| arg == "--version") {
        println!("{} {}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION"));
        true
    } else {
        false
    }
}

fn collect_env() -> cli::Env {
    macro_rules! env_str {
        ($name:literal) => {
            std::env::var($name).ok().map(|v| {
                let s: &'static str = v.leak();
                s
            })
        };
    }

    cli::Env {
        HOME: env_str!("HOME"),
        TMUX_PANE: env_str!("TMUX_PANE"),
        XDG_CONFIG_HOME: env_str!("XDG_CONFIG_HOME"),
        XDG_CACHE_HOME: env_str!("XDG_CACHE_HOME"),
        OPENROUTER_API_KEY: env_str!("OPENROUTER_API_KEY"),
        NVIDIA_API_KEY: env_str!("NVIDIA_API_KEY"),
    }
}
