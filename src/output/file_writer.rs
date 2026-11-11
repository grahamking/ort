//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025,2026 Graham King

extern crate alloc;

use crate::Section;
use crate::common::error::{ort_err, ort_error};
use crate::{ErrorKind, OrtResult, StreamEvent, ThinkEvent, Write, common::stats};

pub struct FileWriter<'a, W: Write + Send> {
    writer: &'a mut W,
    show_reasoning: bool,
    is_quiet: bool,
    usage: Option<stats::Usage>,
    metadata: Option<stats::Metadata>,
    stats_out: Option<stats::Stats>,
    section: Section,
}

impl<'a, W: Write + Send> FileWriter<'a, W> {
    pub fn new(writer: &'a mut W, show_reasoning: bool, is_quiet: bool) -> FileWriter<'a, W> {
        FileWriter {
            writer,
            show_reasoning,
            is_quiet,
            usage: None,
            metadata: None,
            stats_out: None,
            section: Section::None,
        }
    }

    fn section(&mut self, to_section: Section) {
        if self.section != to_section {
            if self.section == Section::Think {
                let _ = self.writer.write("</think>".as_bytes());
            }
            Section::change(self.section, to_section, "<think>".as_bytes(), self.writer);
            self.section = to_section;
        } else {
            Section::same(self.section, self.writer);
        }
    }
}

impl<'a, W: Write + Send> super::OutputWriter for FileWriter<'a, W> {
    fn write(&mut self, data: StreamEvent) -> OrtResult<()> {
        if self.is_quiet && !matches!(data, StreamEvent::Content(_) | StreamEvent::Error(_)) {
            return Ok(());
        }
        match data {
            StreamEvent::Connecting | StreamEvent::Start => {}
            StreamEvent::Think(think) => {
                if self.show_reasoning {
                    self.section(Section::Think);
                    match think {
                        ThinkEvent::Content(s) => {
                            let _ = self.writer.write_all(s.as_bytes());
                        }
                        ThinkEvent::Details(_) => {}
                    }
                }
            }
            StreamEvent::Content(content) => {
                // In quiet mode there is only one section
                if !self.is_quiet {
                    self.section(Section::Content);
                }
                let _ = self.writer.write_all(content.as_bytes());
            }
            StreamEvent::ToolCalls(_) | StreamEvent::ToolDisplay(_) => {
                // TODO
            }
            StreamEvent::Annotation(annotation) => {
                self.section(Section::WebSearch);
                let _ = self.writer.write(annotation.citation_url().as_bytes());
            }
            StreamEvent::Usage(usage) => {
                self.usage = Some(usage);
            }
            StreamEvent::Metadata(metadata) => {
                self.metadata = Some(metadata);
            }
            StreamEvent::Stats(stats) => {
                self.section(Section::Stats);
                self.stats_out = Some(stats);
            }
            StreamEvent::Prompt(prompt) => {
                self.section(Section::Prompt);
                let _ = self.writer.write("> ".as_bytes());
                let _ = self.writer.write(prompt.as_bytes());
                let _ = self.writer.flush();
            }
            StreamEvent::Warn(warning) => {
                self.section(Section::Warn);
                let _ = self.writer.write(warning.trim().as_bytes());
            }
            StreamEvent::Missing => {
                let _ = self.writer.write_char(super::MISSING_CHAR);
            }
            StreamEvent::Error(err_string) => {
                if err_string.contains(super::ERR_RATE_LIMITED) {
                    return Err(ort_error(ErrorKind::RateLimited, ""));
                }
                return Err(ort_err(ErrorKind::ResponseStreamError, err_string.into()));
            }
            StreamEvent::Finish(_reason) => {}
        }
        Ok(())
    }

    fn stop(&mut self, include_stats: bool) -> OrtResult<()> {
        if !include_stats || self.is_quiet {
            return Ok(());
        }

        let Some(mut stats) = self.stats_out.take() else {
            return Err(ort_error(ErrorKind::MissingUsageStats, ""));
        };
        stats.usage = self.usage.take();
        stats.metadata = self.metadata.take().unwrap_or_default();
        let _ = self.writer.write("Stats: ".as_bytes());
        let _ = self.writer.write(stats.as_string().as_bytes());
        let _ = self.writer.write_char('\n');
        Ok(())
    }
}
