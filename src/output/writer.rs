//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use core::ffi::c_void;

use alloc::ffi::CString;
use alloc::string::{String, ToString};

use crate::utils::zclean;
use crate::{ErrorKind, OrtResult, Response, ThinkEvent, Write, common::stats, common::utils};
use crate::{ort_error, syscall};

pub struct ConsoleWriter<'a, W: Write + Send> {
    pub writer: &'a mut W, // Must handle ANSI control chars
    pub show_reasoning: bool,
    pub is_quiet: bool,
    pub is_running: bool,
    pub is_first_content: bool,
    pub spindx: usize,
    pub stats_out: Option<stats::Stats>,
}

impl<'a, W: Write + Send> ConsoleWriter<'a, W> {
    pub fn new(writer: &'a mut W, show_reasoning: bool, is_quiet: bool) -> ConsoleWriter<'a, W> {
        ConsoleWriter {
            writer,
            show_reasoning,
            is_quiet,
            is_running: false,
            is_first_content: true,
            spindx: 0,
            stats_out: None,
        }
    }
}

impl<'a, W: Write + Send> super::OutputWriter for ConsoleWriter<'a, W> {
    fn stop(&mut self, include_stats: bool) -> OrtResult<()> {
        let _ = self.writer.write(super::CURSOR_ON);
        let _ = self.writer.write(b"\n");
        let _ = self.writer.flush();
        if !include_stats || self.is_quiet {
            return Ok(());
        }

        let Some(stats) = self.stats_out.take() else {
            return Err(ort_error(ErrorKind::MissingUsageStats, ""));
        };
        let _ = self.writer.write("\nStats: ".as_bytes());
        let _ = self.writer.write(stats.as_string().as_bytes());
        let _ = self.writer.write_char('\n');

        Ok(())
    }

    fn write(&mut self, data: Response) -> OrtResult<()> {
        if !self.is_running {
            let _ = self.writer.write(super::MSG_CONNECTING);
            let _ = self.writer.flush();
            self.is_running = true;
        }

        match data {
            Response::Start => {
                let _ = self.writer.write(super::MSG_PROCESSING);
                let _ = self.writer.flush();
            }
            Response::Think(think) => {
                if !self.is_first_content {
                    // If content has started, don't show thinking.
                    // Sometimes Gemini Pro sends it out of order.
                    return Ok(());
                }
                if self.show_reasoning {
                    match think {
                        ThinkEvent::Start => {
                            let _ = self.writer.write(super::MSG_THINK_START);
                        }
                        ThinkEvent::Content(s) => {
                            let _ = self.writer.write_all(s.as_bytes());
                            let _ = self.writer.flush();
                        }
                        ThinkEvent::Stop => {
                            let _ = self.writer.write(super::MSG_THINK_END);
                        }
                    }
                } else {
                    match think {
                        ThinkEvent::Start => {
                            let _ = self.writer.write(super::MSG_THINKING);
                            let _ = self.writer.flush();
                        }
                        ThinkEvent::Content(_) => {
                            let _ = self
                                .writer
                                .write(super::SPINNER[self.spindx % super::SPINNER.len()]);
                            let _ = self.writer.flush();
                            self.spindx += 1;
                        }
                        ThinkEvent::Stop => {}
                    }
                }
            }
            Response::Content(content) => {
                if self.is_first_content {
                    // Erase the Processing or Thinking line
                    let _ = self.writer.write(super::MSG_CLEAR_LINE);
                    self.is_first_content = false;
                }
                let _ = self.writer.write_all(content.as_bytes());
                let _ = self.writer.flush();
            }
            Response::ToolCalls(_) | Response::ToolDisplay(_) => {
                // No tool calls in chat mode
            }
            Response::Stats(stats) => {
                self.stats_out = Some(stats);
            }
            Response::Prompt(_prompt) => {
                // Prompt not displayed in chat mode
            }
            Response::Error(err_string) => {
                let _ = self.writer.write(super::CURSOR_ON);
                let _ = self.writer.flush();
                if err_string.contains(super::ERR_RATE_LIMITED) {
                    return Err(ort_error(ErrorKind::RateLimited, ""));
                }
                utils::print_string(c"\nERROR: ", &err_string);
                return Err(ort_error(
                    ErrorKind::ResponseStreamError,
                    "Remote returned an error",
                ));
            }
            Response::None => {
                // TODO: Can this still happen?
                panic!("Response::None means we read the wrong Queue position");
            }
        }

        Ok(())
    }
}

pub struct FileWriter<'a, W: Write + Send> {
    pub writer: &'a mut W,
    pub show_reasoning: bool,
    pub is_quiet: bool,
    pub stats_out: Option<stats::Stats>,
}

impl<'a, W: Write + Send> FileWriter<'a, W> {
    pub fn new(writer: &'a mut W, show_reasoning: bool, is_quiet: bool) -> FileWriter<'a, W> {
        FileWriter {
            writer,
            show_reasoning,
            is_quiet,
            stats_out: None,
        }
    }
}

impl<'a, W: Write + Send> super::OutputWriter for FileWriter<'a, W> {
    fn write(&mut self, data: Response) -> OrtResult<()> {
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
            Response::ToolCalls(_) | Response::ToolDisplay(_) => {
                // TODO
            }
            Response::Stats(stats) => {
                self.stats_out = Some(stats);
            }
            Response::Prompt(prompt) => {
                let _ = self.writer.write("> ".as_bytes());
                let _ = self.writer.write(prompt.as_bytes());
                let _ = self.writer.write(b"\n");
                let _ = self.writer.flush();
            }
            Response::Error(mut err_string) => {
                if err_string.contains(super::ERR_RATE_LIMITED) {
                    return Err(ort_error(ErrorKind::RateLimited, ""));
                }
                let c_s = CString::new("\nERROR: ".to_string() + zclean(&mut err_string)).unwrap();
                syscall::write(2, c_s.as_ptr().cast(), c_s.count_bytes());
                return Err(ort_error(
                    ErrorKind::ResponseStreamError,
                    "OpenRouter returned an error",
                ));
            }
            Response::None => {
                // TODO: Can this still happen?
                panic!("Response::None means we read the wrong Queue position");
            }
        }
        Ok(())
    }

    fn stop(&mut self, include_stats: bool) -> OrtResult<()> {
        let _ = self.writer.write(b"\n");
        if !include_stats || self.is_quiet {
            return Ok(());
        }

        let Some(stats) = self.stats_out.take() else {
            return Err(ort_error(ErrorKind::MissingUsageStats, ""));
        };
        let _ = self.writer.write("\nStats: ".as_bytes());
        let _ = self.writer.write(stats.as_string().as_bytes());
        let _ = self.writer.write_char('\n');
        Ok(())
    }
}

pub struct CollectedWriter {
    contents: String,
    got_stats: Option<stats::Stats>,
    pub output: Option<String>,
}

impl CollectedWriter {
    pub fn new() -> Self {
        Self {
            got_stats: None,
            contents: String::with_capacity(4096),
            output: None,
        }
    }
}

impl super::OutputWriter for CollectedWriter {
    fn write(&mut self, data: Response) -> OrtResult<()> {
        match data {
            Response::Start => {}
            Response::Think(_) => {}
            Response::Content(content) => {
                self.contents.push_str(&content);
            }
            Response::ToolCalls(_) | Response::ToolDisplay(_) => {
                // No ToolCalls when using CollectedWriter
            }
            Response::Stats(stats) => {
                self.got_stats = Some(stats);
            }
            Response::Prompt(_) => {}
            Response::Error(_err) => {
                // Original message: CollectedWriter + err detail
                return Err(ort_error(
                    ErrorKind::ResponseStreamError,
                    "CollectedWriter response error",
                ));
            }
            Response::None => {
                // TODO: Can this still happen?
                panic!("Response::None means we read the wrong Queue position");
            }
        }
        Ok(())
    }

    fn stop(&mut self, _include_stats: bool) -> OrtResult<()> {
        let stat_string = self.got_stats.take().unwrap().as_string();
        let mut out = String::with_capacity(stat_string.len() + self.contents.len() + 9);
        out.push_str("--- ");
        out.push_str(&stat_string);
        out.push_str(" ---\n");
        out.push_str(&self.contents);

        self.output = Some(out);
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
