//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    ort::cli::main(args)
}
