//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use core::ffi::c_void;

use alloc::string::String;
use core::time::Duration;

use crate::common::error::{ort_err, ort_error};
use crate::common::time::{Ticks, TscCalibration, elapsed_duration};
use crate::syscall;
use crate::{ErrorKind, OrtResult, Response, ThinkEvent, Write, common::stats};

const SPINNER_UPDATE_MS: Duration = Duration::from_millis(40);

pub struct ConsoleWriter<'a, W: Write + Send> {
    pub writer: &'a mut W, // Must handle ANSI control chars
    pub show_reasoning: bool,
    pub is_quiet: bool,
    pub is_running: bool,
    pub is_first_content: bool,
    pub has_web_search: bool,
    pub spindx: usize,
    pub stats_out: Option<stats::Stats>,
    tsc_calibration: Option<TscCalibration>,
    last_spinner_update: Ticks,
}

impl<'a, W: Write + Send> ConsoleWriter<'a, W> {
    pub fn new(
        writer: &'a mut W,
        show_reasoning: bool,
        is_quiet: bool,
        tsc_calibration: Option<TscCalibration>,
    ) -> ConsoleWriter<'a, W> {
        ConsoleWriter {
            writer,
            show_reasoning,
            is_quiet,
            is_running: false,
            is_first_content: true,
            has_web_search: false,
            spindx: 0,
            stats_out: None,
            tsc_calibration,
            last_spinner_update: Ticks::now(),
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
                            if self.has_web_search {
                                // Blank line after web search, switch back to grey
                                let _ = self.writer.write_char('\n');
                                let _ = self.writer.write(super::MSG_THINK_START);
                                self.has_web_search = false;
                            }
                            let _ = self.writer.write_all(s.as_bytes());
                            let _ = self.writer.flush();
                        }
                        ThinkEvent::Details(_) => {
                            // OpenRouter puts anything interesting from here into
                            // ThinkEvent::Content
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
                            let now = Ticks::now();
                            let should_update = self.tsc_calibration.is_none_or(|tc| {
                                elapsed_duration(self.last_spinner_update, now, tc)
                                    >= SPINNER_UPDATE_MS
                            });
                            if should_update {
                                let _ = self
                                    .writer
                                    .write(super::SPINNER[self.spindx % super::SPINNER.len()]);
                                let _ = self.writer.flush();
                                self.spindx += 1;
                                self.last_spinner_update = now;
                            }
                        }
                        ThinkEvent::Details(_) => {}
                        ThinkEvent::Stop => {}
                    }
                }
            }
            Response::Content(content) => {
                if self.has_web_search {
                    // Blank line after web search
                    let _ = self.writer.write_char('\n');
                    self.has_web_search = false;
                }
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
            Response::Annotation(annotation) => {
                // These are url_citation from remote web_search tool
                if !self.has_web_search {
                    let _ = self.writer.write(b"\n\n");
                }
                let _ = self.writer.write(super::MSG_WEB_FETCH);
                let _ = self.writer.write(annotation.citation_url().as_bytes());
                let _ = self.writer.write_char('\n');
                self.has_web_search = true;
            }
            Response::Stats(stats) => {
                self.stats_out = Some(stats);
            }
            Response::Prompt(_prompt) => {
                // Prompt not displayed in chat mode
            }
            Response::Missing => {
                let _ = self.writer.write_char(super::MISSING_CHAR);
            }
            Response::Warn(warning) => {
                let _ = self.writer.write(super::WARN_START);
                let _ = self.writer.write(warning.trim().as_bytes());
                let _ = self.writer.write(super::RESET);
                let _ = self.writer.flush();
            }
            Response::Error(err_string) => {
                let _ = self.writer.write(super::CURSOR_ON);
                let _ = self.writer.flush();
                if err_string.contains(super::ERR_RATE_LIMITED) {
                    return Err(ort_error(ErrorKind::RateLimited, ""));
                }
                return Err(ort_err(ErrorKind::ResponseStreamError, err_string.into()));
            }
        }

        Ok(())
    }
}

pub struct FileWriter<'a, W: Write + Send> {
    pub writer: &'a mut W,
    pub show_reasoning: bool,
    pub has_web_search: bool,
    pub is_quiet: bool,
    pub stats_out: Option<stats::Stats>,
}

impl<'a, W: Write + Send> FileWriter<'a, W> {
    pub fn new(writer: &'a mut W, show_reasoning: bool, is_quiet: bool) -> FileWriter<'a, W> {
        FileWriter {
            writer,
            show_reasoning,
            is_quiet,
            has_web_search: false,
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
                            if self.has_web_search {
                                // Blank line after web search
                                let _ = self.writer.write_char('\n');
                                self.has_web_search = false;
                            }
                            let _ = self.writer.write_all(s.as_bytes());
                        }
                        ThinkEvent::Details(_) => {}
                        ThinkEvent::Stop => {
                            let _ = self.writer.write("</think>\n\n".as_bytes());
                        }
                    }
                }
            }
            Response::Content(content) => {
                if self.has_web_search {
                    // Blank line after web search
                    let _ = self.writer.write_char('\n');
                    self.has_web_search = false;
                }
                let _ = self.writer.write_all(content.as_bytes());
            }
            Response::ToolCalls(_) | Response::ToolDisplay(_) => {
                // TODO
            }
            Response::Annotation(annotation) => {
                // These are url_citation from remote web_search tool
                if !self.has_web_search && self.show_reasoning {
                    let _ = self.writer.write(b"\n\n");
                }
                let _ = self.writer.write(b"Web fetch: ");
                let _ = self.writer.write(annotation.citation_url().as_bytes());
                let _ = self.writer.write_char('\n');
                self.has_web_search = true;
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
            Response::Warn(warning) => {
                let _ = self.writer.write(warning.trim().as_bytes());
                let _ = self.writer.write_char('\n');
            }
            Response::Missing => {
                let _ = self.writer.write_char(super::MISSING_CHAR);
            }
            Response::Error(err_string) => {
                if err_string.contains(super::ERR_RATE_LIMITED) {
                    return Err(ort_error(ErrorKind::RateLimited, ""));
                }
                return Err(ort_err(ErrorKind::ResponseStreamError, err_string.into()));
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
            Response::Annotation(_) => {
                // TODO
            }
            Response::Stats(stats) => {
                self.got_stats = Some(stats);
            }
            Response::Prompt(_) => {}
            Response::Missing => {
                self.contents.push(super::MISSING_CHAR);
            }
            Response::Warn(_warning) => {
                // TODO
            }
            Response::Error(err) => {
                return Err(ort_err(ErrorKind::ResponseStreamError, err.into()));
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
