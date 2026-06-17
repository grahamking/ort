//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!
//! Output/response path, from the point of view of the user,
//! so deserializing openrouter.ai's response, and writing out
//! to the screen/file/history.

use crate::OrtResult;
use crate::common::data::Response;

pub mod agent;
pub mod last_writer;
pub mod logger;
pub mod writer;

pub const CURSOR_ON: &[u8] = "\x1b[?25h".as_bytes();

//const CURSOR_OFF: &str = "\x1b[?25l";
pub const MSG_CONNECTING: &[u8] = "\x1b[?25lConnecting...\r".as_bytes();

// \r{CLEAR_LINE}\n
pub const MSG_CLEAR_LINE: &[u8] = "\r\x1b[2K\n".as_bytes();

// These are both surrounded by BOLD_START and BOLD_END, but I can't find a way to
// do string concatenation at build time with constants
pub const MSG_PROCESSING: &[u8] = "\x1b[1mProcessing...\x1b[0m\r".as_bytes();
pub const MSG_THINKING: &[u8] = "\x1b[1mThinking...\x1b[0m  ".as_bytes();

pub const MSG_THINK_START: &[u8] = "\x1b[2m".as_bytes();
pub const MSG_THINK_END: &[u8] = "\x1b[0m\n".as_bytes();

// The spinner displays a sequence of these characters: | / - \ , which when
// animated look like they are spinning.
// The array includes the ANSI escape to move back one character after each one
// is printed, so they overwrite each other.
//const BACK_ONE: &[u8] = "\x1b[1D".as_bytes();
pub const SPINNER: [&[u8]; 4] = [
    "|\x1b[1D".as_bytes(),
    "/\x1b[1D".as_bytes(),
    "-\x1b[1D".as_bytes(),
    "\\\x1b[1D".as_bytes(),
];

pub const PROMPT_START: &[u8] = "\n\x1b[3m".as_bytes();
pub const RESET: &[u8] = "\x1b[0m".as_bytes();

pub const TOOL_CALL_START: &[u8] = "\n\x1b[0m\x1b[96m".as_bytes();
pub const TOOL_CALL_END: &[u8] = "\x1b[0m\n".as_bytes();

pub const ERR_RATE_LIMITED: &str = "429 Too Many Requests";

pub trait OutputWriter {
    fn write(&mut self, data: Response) -> OrtResult<()>;
    fn stop(&mut self, include_stats: bool) -> OrtResult<()>;
}
