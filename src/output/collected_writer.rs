//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025,2026 Graham King

extern crate alloc;

use alloc::string::String;

use crate::common::error::ort_err;
use crate::ort_error;
use crate::{ErrorKind, OrtResult, Response, common::stats};

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
    fn write(&mut self, data: Response) -> OrtResult<()> {
        match data {
            Response::Connecting | Response::Start => {}
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
            Response::Metadata(metadata) => {
                self.metadata = Some(metadata);
            }
            Response::Usage(usage) => {
                self.usage = Some(usage);
            }
            Response::Stats(stats) => {
                self.stats_out = Some(stats);
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
            Response::Finish(_reason) => {}
        }
        Ok(())
    }

    fn stop(&mut self, _include_stats: bool) -> OrtResult<()> {
        let Some(mut stats) = self.stats_out.take() else {
            return Err(ort_error(ErrorKind::MissingUsageStats, ""));
        };
        stats.usage = self.usage.take();
        stats.metadata = self.metadata.take();
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
