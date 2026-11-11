//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025,2026 Graham King

extern crate alloc;

use alloc::string::String;

use crate::common::error::ort_err;
use crate::ort_error;
use crate::{ErrorKind, OrtResult, StreamEvent, common::stats};

pub struct CollectedWriter {
    contents: String,
    usage: Option<stats::Usage>,
    metadata: Option<stats::Metadata>,
    stats_out: Option<stats::Stats>,
    pub output: Option<String>,
}

impl CollectedWriter {
    pub fn new() -> Self {
        Self {
            usage: None,
            metadata: None,
            stats_out: None,
            contents: String::with_capacity(4096),
            output: None,
        }
    }
}

impl super::OutputWriter for CollectedWriter {
    fn write(&mut self, data: StreamEvent) -> OrtResult<()> {
        match data {
            StreamEvent::Connecting | StreamEvent::Start => {}
            StreamEvent::Think(_) => {}
            StreamEvent::Content(content) => {
                self.contents.push_str(&content);
            }
            StreamEvent::ToolCalls(_) | StreamEvent::ToolDisplay(_) => {
                // No ToolCalls when using CollectedWriter
            }
            StreamEvent::Annotation(_) => {
                // TODO
            }
            StreamEvent::Metadata(metadata) => {
                self.metadata = Some(metadata);
            }
            StreamEvent::Usage(usage) => {
                self.usage = Some(usage);
            }
            StreamEvent::Stats(stats) => {
                self.stats_out = Some(stats);
            }
            StreamEvent::Prompt(_) => {}
            StreamEvent::Missing => {
                self.contents.push(super::MISSING_CHAR);
            }
            StreamEvent::Warn(_warning) => {
                // TODO
            }
            StreamEvent::Error(err) => {
                return Err(ort_err(ErrorKind::ResponseStreamError, err.into()));
            }
            StreamEvent::Finish(_reason) => {}
        }
        Ok(())
    }

    fn stop(&mut self, _include_stats: bool) -> OrtResult<()> {
        let Some(mut stats) = self.stats_out.take() else {
            return Err(ort_error(ErrorKind::MissingUsageStats, ""));
        };
        stats.usage = self.usage.take();
        stats.metadata = self.metadata.take().unwrap_or_default();
        let stat_string = stats.as_string();
        let mut out = String::with_capacity(stat_string.len() + self.contents.len() + 9);
        out.push_str("--- ");
        out.push_str(&stat_string);
        out.push_str(" ---\n");
        out.push_str(&self.contents);

        self.output = Some(out);
        Ok(())
    }
}
