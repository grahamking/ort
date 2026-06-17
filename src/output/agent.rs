//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::ToString;

use crate::ErrorKind;
use crate::OrtResult;
use crate::ThinkEvent;
use crate::Write;
use crate::common::data::Response;
use crate::ort_error;
use crate::syscall;
use crate::utils::zclean;

pub struct AgentWriter<'a, W: Write + Send> {
    pub writer: &'a mut W,
    pub show_reasoning: bool,
}

impl<'a, W: Write + Send> AgentWriter<'a, W> {
    pub fn new(writer: &'a mut W, show_reasoning: bool) -> AgentWriter<'a, W> {
        Self {
            writer,
            show_reasoning,
        }
    }
}

impl<'a, W: Write + Send> super::OutputWriter for AgentWriter<'a, W> {
    fn write(&mut self, data: Response) -> OrtResult<()> {
        match data {
            Response::Start => {}
            Response::Think(think) => {
                if self.show_reasoning {
                    match think {
                        ThinkEvent::Start => {
                            let _ = self.writer.write(super::MSG_THINK_START);
                            let _ = self.writer.flush();
                        }
                        ThinkEvent::Content(s) => {
                            let _ = self.writer.write_all(s.as_bytes());
                            let _ = self.writer.flush();
                        }
                        ThinkEvent::Stop => {
                            let _ = self.writer.write(super::MSG_THINK_END);
                            let _ = self.writer.write_char('\n');
                        }
                    }
                }
            }
            Response::Content(content) => {
                let _ = self.writer.write_all(content.as_bytes());
            }
            Response::ToolCalls(_tool_calls) => {
                // We use ToolDisplay instead
            }
            Response::ToolDisplay(tool) => {
                let _ = self.writer.write(super::TOOL_CALL_START);
                let _ = self.writer.write(tool.name.as_bytes());
                let _ = self.writer.write(super::TOOL_CALL_ARGUMENT_START);
                let _ = self.writer.write(tool.arguments.trim().as_bytes());
                let _ = self.writer.write(super::TOOL_CALL_END);
                let _ = self.writer.flush();
            }
            Response::Stats(_stats) => {}
            Response::Prompt(prompt) => {
                let _ = self.writer.write(super::PROMPT_START);
                let _ = self.writer.write(prompt.as_bytes());
                let _ = self.writer.write(super::RESET);
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

    fn stop(&mut self, _include_stats: bool) -> OrtResult<()> {
        let _ = self.writer.write(b"\n");
        Ok(())
    }
}
