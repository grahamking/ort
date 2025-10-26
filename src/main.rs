//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::io::IsTerminal;

fn main() -> std::process::ExitCode {
    let stdout = std::io::stdout();
    //let stdout_writer = stdout.lock();
    let args: Vec<String> = std::env::args().collect();
    ort::cli::main(args, stdout.is_terminal(), stdout)
}
