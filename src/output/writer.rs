//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use core::ffi::c_void;

use alloc::ffi::CString;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::utils::zclean;
use crate::{
    Context, ErrorKind, LastData, Message, OrtResult, PromptOpts, Response, ThinkEvent, Write,
    common::config, common::file, common::queue, common::stats, common::utils,
};
use crate::{libc, ort_error};

const CURSOR_ON: &[u8] = "\x1b[?25h".as_bytes();

//const CURSOR_OFF: &str = "\x1b[?25l";
const MSG_CONNECTING: &[u8] = "\x1b[?25lConnecting...\r".as_bytes();

// \r{CLEAR_LINE}\n
const MSG_CLEAR_LINE: &[u8] = "\r\x1b[2K\n".as_bytes();

// These are all surrounded by BOLD_START and BOLD_END, but I can't find a way to
// do string concatenation at build time with constants
//const BOLD_START: &str = "\x1b[1m";
//const BOLD_END: &str = "\x1b[0m";
const MSG_PROCESSING: &[u8] = "\x1b[1mProcessing...\x1b[0m\r".as_bytes();
const MSG_THINK_TAG_END: &[u8] = "\x1b[1m</think>\x1b[0m\n".as_bytes();
const MSG_THINKING: &[u8] = "\x1b[1mThinking...\x1b[0m  ".as_bytes();
const MSG_THINK_TAG_START: &[u8] = "\x1b[1m<think>\x1b[0m".as_bytes();

// The spinner displays a sequence of these characters: | / - \ , which when
// animated look like they are spinning.
// The array includes the ANSI escape to move back one character after each one
// is printed, so they overwrite each other.
//const BACK_ONE: &[u8] = "\x1b[1D".as_bytes();
const SPINNER: [&[u8]; 4] = [
    "|\x1b[1D".as_bytes(),
    "/\x1b[1D".as_bytes(),
    "-\x1b[1D".as_bytes(),
    "\\\x1b[1D".as_bytes(),
];

const ERR_RATE_LIMITED: &str = "429 Too Many Requests";

pub struct ConsoleWriter<W: Write + Send> {
    pub writer: W, // Must handle ANSI control chars
    pub show_reasoning: bool,
}

impl<W: Write + Send> ConsoleWriter<W> {
    pub fn into_inner(self) -> W {
        self.writer
    }
    pub fn run<const N: usize>(
        &mut self,
        mut rx: queue::Consumer<Response, N>,
    ) -> OrtResult<stats::Stats> {
        let _ = self.writer.write(MSG_CONNECTING);
        let _ = self.writer.flush();

        let mut is_first_content = true;
        let mut spindx = 0;
        let mut stats_out = None;
        while let Some(data) = rx.get_next() {
            match data {
                Response::Start => {
                    let _ = self.writer.write(MSG_PROCESSING);
                    let _ = self.writer.flush();
                }
                Response::Think(think) => {
                    if self.show_reasoning {
                        match think {
                            ThinkEvent::Start => {
                                let _ = self.writer.write(MSG_THINK_TAG_START);
                            }
                            ThinkEvent::Content(s) => {
                                let _ = self.writer.write_all(s.as_bytes());
                                let _ = self.writer.flush();
                            }
                            ThinkEvent::Stop => {
                                let _ = self.writer.write(MSG_THINK_TAG_END);
                            }
                        }
                    } else {
                        match think {
                            ThinkEvent::Start => {
                                let _ = self.writer.write(MSG_THINKING);
                                let _ = self.writer.flush();
                            }
                            ThinkEvent::Content(_) => {
                                let _ = self.writer.write(SPINNER[spindx % SPINNER.len()]);
                                let _ = self.writer.flush();
                                spindx += 1;
                            }
                            ThinkEvent::Stop => {}
                        }
                    }
                }
                Response::Content(content) => {
                    if is_first_content {
                        // Erase the Processing or Thinking line
                        let _ = self.writer.write(MSG_CLEAR_LINE);
                        is_first_content = false;
                    }
                    let _ = self.writer.write_all(content.as_bytes());
                    let _ = self.writer.flush();
                }
                Response::Stats(stats) => {
                    stats_out = Some(stats);
                }
                Response::Error(mut err_string) => {
                    let _ = self.writer.write(CURSOR_ON);
                    let _ = self.writer.flush();
                    if err_string.contains(ERR_RATE_LIMITED) {
                        return Err(ort_error(ErrorKind::RateLimited, ""));
                    }
                    let c_s =
                        CString::new("\nERROR: ".to_string() + zclean(&mut err_string)).unwrap();
                    unsafe {
                        libc::write(2, c_s.as_ptr().cast(), c_s.count_bytes());
                    }
                    return Err(ort_error(
                        ErrorKind::ResponseStreamError,
                        "OpenRouter returned an error",
                    ));
                }
                Response::None => {
                    panic!("Response::None means we read the wrong Queue position");
                }
            }
        }

        let _ = self.writer.write(CURSOR_ON);
        let _ = self.writer.flush();

        let Some(stats) = stats_out else {
            return Err(ort_error(ErrorKind::MissingUsageStats, ""));
        };
        Ok(stats)
    }
}

pub struct FileWriter<W: Write + Send> {
    pub writer: W,
    pub show_reasoning: bool,
}

impl<W: Write + Send> FileWriter<W> {
    pub fn into_inner(self) -> W {
        self.writer
    }
    pub fn run<const N: usize>(
        &mut self,
        mut rx: queue::Consumer<Response, N>,
    ) -> OrtResult<stats::Stats> {
        let mut stats_out = None;
        while let Some(data) = rx.get_next() {
            match data {
                Response::Start => {}
                Response::Think(think) => {
                    if self.show_reasoning {
                        match think {
                            ThinkEvent::Start => {
                                let _ = self.writer.write("<think>".as_bytes());
                            }
                            ThinkEvent::Content(s) => {
                                let _ = self.writer.write_all(s.as_bytes());
                            }
                            ThinkEvent::Stop => {
                                let _ = self.writer.write("</think>\n\n".as_bytes());
                            }
                        }
                    }
                }
                Response::Content(content) => {
                    let _ = self.writer.write_all(content.as_bytes());
                }
                Response::Stats(stats) => {
                    stats_out = Some(stats);
                }
                Response::Error(mut err_string) => {
                    if err_string.contains(ERR_RATE_LIMITED) {
                        return Err(ort_error(ErrorKind::RateLimited, ""));
                    }
                    let c_s =
                        CString::new("\nERROR: ".to_string() + zclean(&mut err_string)).unwrap();
                    unsafe {
                        libc::write(2, c_s.as_ptr().cast(), c_s.count_bytes());
                    }
                    return Err(ort_error(
                        ErrorKind::ResponseStreamError,
                        "OpenRouter returned an error",
                    ));
                }
                Response::None => {
                    return Err(ort_error(
                        ErrorKind::QueueDesync,
                        "Response::None means we read the wrong Queue position",
                    ));
                }
            }
        }

        let Some(stats) = stats_out else {
            return Err(ort_error(ErrorKind::MissingUsageStats, ""));
        };
        Ok(stats)
    }
}

pub struct CollectedWriter {}

impl CollectedWriter {
    pub fn run<const N: usize>(
        &mut self,
        mut rx: queue::Consumer<Response, N>,
    ) -> OrtResult<String> {
        let mut got_stats = None;
        let mut contents = Vec::with_capacity(1024);
        while let Some(data) = rx.get_next() {
            match data {
                Response::Start => {}
                Response::Think(_) => {}
                Response::Content(content) => {
                    contents.push(content);
                }
                Response::Stats(stats) => {
                    got_stats = Some(stats);
                }
                Response::Error(_err) => {
                    // Original message: CollectedWriter + err detail
                    return Err(ort_error(
                        ErrorKind::ResponseStreamError,
                        "CollectedWriter response error",
                    ));
                }
                Response::None => {
                    return Err(ort_error(
                        ErrorKind::QueueDesync,
                        "Response::None means we read the wrong Queue position",
                    ));
                }
            }
        }

        let out =
            "--- ".to_string() + &got_stats.unwrap().as_string() + " ---\n" + &contents.join("");
        Ok(out)
    }
}

pub struct LastWriter {
    w: file::File,
    data: LastData,
}

impl LastWriter {
    pub fn new(opts: PromptOpts, messages: Vec<Message>) -> OrtResult<Self> {
        let last_filename = utils::last_filename();
        let mut last_path = config::cache_dir()?;
        last_path.push('/');
        last_path.push_str(&last_filename);
        let c_path = CString::new(last_path)
            .map_err(|_| ort_error(ErrorKind::FileCreateFailed, "Null byte in last path"))?;
        let last_file = unsafe { file::File::create(c_path.as_ptr()).context("create last file")? };
        let data = LastData { opts, messages };
        Ok(LastWriter { data, w: last_file })
    }

    pub fn run<const N: usize>(
        &mut self,
        mut rx: queue::Consumer<Response, N>,
    ) -> OrtResult<stats::Stats> {
        let mut contents = Vec::with_capacity(1024);
        while let Some(data) = rx.get_next() {
            match data {
                Response::Start => {}
                Response::Think(_) => {}
                Response::Content(content) => {
                    contents.push(content);
                }
                Response::Stats(stats) => {
                    self.data.opts.provider = Some(utils::slug(stats.provider()));
                }
                Response::Error(_err) => {
                    return Err(ort_error(
                        ErrorKind::LastWriterError,
                        "LastWriter run error",
                    ));
                }
                Response::None => {
                    return Err(ort_error(
                        ErrorKind::QueueDesync,
                        "Response::None means we read the wrong Queue position",
                    ));
                }
            }
        }

        let message = Message::assistant(contents.join(""));
        self.data.messages.push(message);

        self.data.to_json_writer(&mut self.w)?;
        let _ = (&mut self.w).flush();

        Ok(stats::Stats::default()) // Stats is not used
    }
}

pub struct StdoutWriter {}

impl Write for StdoutWriter {
    fn write(&mut self, buf: &[u8]) -> OrtResult<usize> {
        let bytes_written = unsafe { libc::write(1, buf.as_ptr() as *const c_void, buf.len()) };
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
