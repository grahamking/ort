//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025,2026 Graham King

extern crate alloc;

use core::time::Duration;

use crate::Section;
use crate::common::error::{ort_err, ort_error};
use crate::common::time::{Ticks, TscCalibration, elapsed_duration};
use crate::{ErrorKind, OrtResult, Response, ThinkEvent, Write, common::stats};

const SPINNER_UPDATE_MS: Duration = Duration::from_millis(40);

pub struct ConsoleWriter<'a, W: Write + Send> {
    writer: &'a mut W, // Must handle ANSI control chars
    show_reasoning: bool,
    is_quiet: bool,
    spindx: usize,
    stats_out: Option<stats::Stats>,
    tsc_calibration: Option<TscCalibration>,
    last_spinner_update: Ticks,
    section: Section,
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
            spindx: 0,
            stats_out: None,
            tsc_calibration,
            last_spinner_update: Ticks::now(),
            section: Section::None,
        }
    }

    fn section(&mut self, to_section: Section) {
        if self.section != to_section {
            Section::change(self.section, to_section, super::THINK_START, self.writer);
            self.section = to_section;
        } else {
            Section::same(self.section, self.writer);
        }
    }
}

impl<'a, W: Write + Send> super::OutputWriter for ConsoleWriter<'a, W> {
    fn stop(&mut self, include_stats: bool) -> OrtResult<()> {
        let _ = self.writer.write(super::CURSOR_ON);
        if !include_stats || self.is_quiet {
            return Ok(());
        }

        let Some(stats) = self.stats_out.take() else {
            return Err(ort_error(ErrorKind::MissingUsageStats, ""));
        };
        let _ = self.writer.write("Stats: ".as_bytes());
        let _ = self.writer.write(stats.as_string().as_bytes());
        let _ = self.writer.write_char('\n');

        Ok(())
    }

    fn write(&mut self, data: Response) -> OrtResult<()> {
        match data {
            Response::Connecting => {
                let _ = self.writer.write(super::MSG_CONNECTING);
                let _ = self.writer.flush();
            }
            Response::Start => {
                let _ = self.writer.write(super::MSG_PROCESSING);
                let _ = self.writer.flush();
            }
            Response::Think(think) => {
                if self.section == Section::Content {
                    // If content has started, don't show thinking.
                    // Sometimes Gemini Pro sends it out of order.
                    return Ok(());
                }
                if self.show_reasoning {
                    match think {
                        ThinkEvent::Start | ThinkEvent::Stop | ThinkEvent::Details(_) => {}
                        ThinkEvent::Content(s) => {
                            self.section(Section::Think);
                            let _ = self.writer.write_all(s.as_bytes());
                            let _ = self.writer.flush();
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
                self.section(Section::Content);
                let _ = self.writer.write_all(content.as_bytes());
                let _ = self.writer.flush();
            }
            Response::ToolCalls(_) | Response::ToolDisplay(_) => {
                // No tool calls in chat mode
            }
            Response::Annotation(annotation) => {
                // These are url_citation from remote web_search tool
                self.section(Section::WebSearch);
                let _ = self.writer.write(super::MSG_WEB_FETCH);
                let _ = self.writer.write(annotation.citation_url().as_bytes());
            }
            Response::Stats(stats) => {
                self.section(Section::Stats);
                self.stats_out = Some(stats);
            }
            Response::Prompt(_prompt) => {
                // Prompt not displayed in chat mode
            }
            Response::Missing => {
                let _ = self.writer.write_char(super::MISSING_CHAR);
            }
            Response::Warn(warning) => {
                self.section(Section::Warn);
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
