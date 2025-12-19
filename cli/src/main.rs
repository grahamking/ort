//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use ort_openrouter_core::{OrtResult, Write, cli, ort_from_err};
use std::{io::IsTerminal, process::ExitCode};

fn main() -> ExitCode {
    let stdout = std::io::stdout();
    //let stdout_writer = stdout.lock();
    let args: Vec<String> = std::env::args().collect();
    match cli::main(args, stdout.is_terminal(), WriteConvertor(stdout)) {
        Ok(exit_code) => ExitCode::from(exit_code as u8),
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

struct WriteConvertor<T: std::io::Write>(T);

impl<T: std::io::Write> Write for WriteConvertor<T> {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        self.0.write(buf).map_err(ort_from_err)
    }

    fn flush(&mut self) -> OrtResult<()> {
        let _ = self.0.flush();
        Ok(())
    }
}
