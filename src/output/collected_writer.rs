//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025,2026 Graham King

extern crate alloc;

use alloc::string::String;

use crate::common::error::ort_err;
use crate::{ErrorKind, OrtResult, Response, common::stats};

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
