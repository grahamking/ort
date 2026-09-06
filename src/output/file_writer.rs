//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025,2026 Graham King

extern crate alloc;

use crate::Section;
use crate::common::error::{ort_err, ort_error};
use crate::{ErrorKind, OrtResult, Response, ThinkEvent, Write, common::stats};

pub struct FileWriter<'a, W: Write + Send> {
    writer: &'a mut W,
    show_reasoning: bool,
    is_quiet: bool,
    stats_out: Option<stats::Stats>,
    section: Section,
}

impl<'a, W: Write + Send> FileWriter<'a, W> {
    pub fn new(writer: &'a mut W, show_reasoning: bool, is_quiet: bool) -> FileWriter<'a, W> {
        FileWriter {
            writer,
            show_reasoning,
            is_quiet,
            stats_out: None,
            section: Section::None,
        }
    }

    fn section(&mut self, to_section: Section) {
        if self.section != to_section {
            self.new_section(to_section);
        } else {
            self.same_section();
        }
    }

    fn new_section(&mut self, to_section: Section) {
        // From
        if self.section == Section::Think {
            let _ = self.writer.write("</think>".as_bytes());
        }

        // Blank line between each section
        if self.section != Section::None {
            let _ = self.writer.write(b"\n\n");
        }

        // To
        match to_section {
            Section::Think => {
                let _ = self.writer.write("<think>".as_bytes());
            }
            Section::Content => {
                let _ = self.writer.write(super::CONTENT_START);
            }
            _ => {}
        }

        // Update
        self.section = to_section;
    }

    fn same_section(&mut self) {
        match self.section {
            Section::WebSearch | Section::Tool => {
                // These must go one per line
                let _ = self.writer.write_char('\n');
            }
            _ => {}
        }
    }
}

impl<'a, W: Write + Send> super::OutputWriter for FileWriter<'a, W> {
    fn write(&mut self, data: Response) -> OrtResult<()> {
        match data {
            Response::Connecting | Response::Start => {}
            Response::Think(think) => {
                if self.show_reasoning {
                    self.section(Section::Think);
                    match think {
                        ThinkEvent::Start => {}
                        ThinkEvent::Content(s) => {
                            let _ = self.writer.write_all(s.as_bytes());
                        }
                        ThinkEvent::Details(_) => {}
                        ThinkEvent::Stop => {}
                    }
                }
            }
            Response::Content(content) => {
                self.section(Section::Content);
                let _ = self.writer.write_all(content.as_bytes());
            }
            Response::ToolCalls(_) | Response::ToolDisplay(_) => {
                // TODO
            }
            Response::Annotation(annotation) => {
                self.section(Section::WebSearch);
                let _ = self.writer.write(annotation.citation_url().as_bytes());
            }
            Response::Stats(stats) => {
                self.section(Section::Stats);
                self.stats_out = Some(stats);
            }
            Response::Prompt(prompt) => {
                self.section(Section::Prompt);
                let _ = self.writer.write("> ".as_bytes());
                let _ = self.writer.write(prompt.as_bytes());
                let _ = self.writer.flush();
            }
            Response::Warn(warning) => {
                self.section(Section::Warn);
                let _ = self.writer.write(warning.trim().as_bytes());
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
}
