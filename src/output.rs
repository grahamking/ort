//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!
//! Output/response path, from the point of view of the user,
//! so deserializing openrouter.ai's response, and writing out
//! to the screen/file/history.

extern crate alloc;
use core::ffi::c_void;

use crate::common::error::ort_error;
use crate::syscall;
use crate::{ErrorKind, OrtResult, Write};

use crate::common::data::Response;

pub mod collected_writer;
pub mod console_writer;
pub mod file_writer;
pub mod last_writer;
pub mod logger;

// No \n in these constants!
// That all goes in`section`

const CURSOR_ON: &[u8] = "\x1b[?25h".as_bytes();
//const CURSOR_OFF: &str = "\x1b[?25l";
const MSG_CONNECTING: &[u8] = "\x1b[?25lConnecting...\r".as_bytes();

//const MSG_CLEAR_LINE: &[u8] = "\r\x1b[2K\n".as_bytes();
const RESET: &[u8] = "\x1b[0m".as_bytes();

// These are surrounded by BOLD_START and BOLD_END, but I can't find a way to
// do string concatenation at build time with constants
const MSG_PROCESSING: &[u8] = "\x1b[1mProcessing...\x1b[0m\r".as_bytes();
const MSG_THINKING: &[u8] = "\x1b[1mThinking...\x1b[0m ".as_bytes();
const MSG_WEB_FETCH: &[u8] = "\x1b[0m\x1b[2mWeb search: \x1b[0m".as_bytes();

const THINK_START: &[u8] = "\x1b[0m\x1b[2m".as_bytes();
const CONTENT_START: &[u8] = "\x1b[0m".as_bytes();

const WARN_START: &[u8] = "\x1b[38;5;208m".as_bytes();

// The spinner displays a sequence of these characters: | / - \ , which when
// animated look like they are spinning.
// The array includes the ANSI escape to move back one character after each one
// is printed, so they overwrite each other.
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

pub struct StdoutWriter {}

impl Write for StdoutWriter {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        let bytes_written = syscall::write(1, buf.as_ptr() as *const c_void, buf.len());
        if bytes_written >= 0 {
            Ok(bytes_written as usize)
        } else {
            Err(ort_error(ErrorKind::StdoutWriteFailed, ""))
        }
    }

    fn flush(&mut self) -> OrtResult<()> {
        Ok(())
    }
}

/// Section is in charge of formatting output by adding new lines as appropriate.
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum Section {
    None,
    Prompt,
    Think,
    WebSearch,
    Tool,
    Content,
    Stats,
    Warn,
}

impl Section {
    /// Move to a new section, e.g. Think is done now show Content
    pub fn change<T: Write>(
        from_section: Section,
        to_section: Section,
        think_start: &[u8],
        writer: &mut T,
    ) {
        // Blank line between each section
        if from_section != Section::None {
            let _ = writer.write(b"\n\n");
        }

        // To
        match to_section {
            Section::Think => {
                let _ = writer.write(think_start);
            }
            Section::Content => {
                let _ = writer.write(CONTENT_START);
            }
            _ => {}
        }
    }

    /// More content for the same section. Usually do nothing.
    pub fn same<T: Write>(section: Section, writer: &mut T) {
        match section {
            Section::WebSearch | Section::Tool => {
                // These must go one per line
                let _ = writer.write_char('\n');
            }
            _ => {}
        }
    }
}
