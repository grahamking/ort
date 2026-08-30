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

pub mod last_writer;
pub mod logger;
pub mod writer;

const CURSOR_ON: &[u8] = "\x1b[?25h".as_bytes();

//const CURSOR_OFF: &str = "\x1b[?25l";
const MSG_CONNECTING: &[u8] = "\x1b[?25lConnecting...\r".as_bytes();

// \r{CLEAR_LINE}\n
const MSG_CLEAR_LINE: &[u8] = "\r\x1b[2K\n".as_bytes();
const RESET: &[u8] = "\x1b[0m".as_bytes();

// These are surrounded by BOLD_START and BOLD_END, but I can't find a way to
// do string concatenation at build time with constants
const MSG_PROCESSING: &[u8] = "\x1b[1mProcessing...\x1b[0m\r".as_bytes();
const MSG_THINKING: &[u8] = "\x1b[1mThinking...\x1b[0m ".as_bytes();
const MSG_WEB_FETCH: &[u8] = "\x1b[0m\x1b[2mWeb search: \x1b[0m".as_bytes();

const MSG_THINK_START: &[u8] = "\x1b[2m".as_bytes();
const MSG_THINK_END: &[u8] = "\x1b[0m\n".as_bytes();

const WARN_START: &[u8] = "\x1b[38;5;208m".as_bytes();

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

pub const ERR_RATE_LIMITED: &str = "429 Too Many Requests";

/// What to display if we couldn't parse something, so we're missing a token.
pub const MISSING_CHAR: char = '□';

pub trait OutputWriter {
    fn write(&mut self, data: Response) -> OrtResult<()>;
    fn stop(&mut self, _include_stats: bool) -> OrtResult<()> {
        Ok(())
    }
}
