//! art: Open Router Agent
//! Part of the `ort` project
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use ort_openrouter_cli::{
    ErrorKind, OrtResult, OutputWriter, Response, ThinkEvent, Write, ort_err, ort_error,
};

const MSG_THINK_START: &[u8] = "\x1b[2m".as_bytes();
const MSG_THINK_END: &[u8] = "\x1b[0m\n".as_bytes();

const TOOL_CALL_START: &[u8] = "\n\x1b[0m".as_bytes();
const TOOL_CALL_ARGUMENT_START: &[u8] = "\x1b[96m".as_bytes();
const TOOL_CALL_END: &[u8] = "\x1b[0m\n".as_bytes();

const AGENT_STATS_START: &[u8] = "\n\x1b[35m".as_bytes();
const AGENT_STATS_END: &[u8] = "\x1b[0m\n".as_bytes();

const PROMPT_START: &[u8] = "\n\x1b[3m".as_bytes();

const ERR_RATE_LIMITED: &str = "429 Too Many Requests";
const RESET: &[u8] = "\x1b[0m".as_bytes();

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

impl<'a, W: Write + Send> OutputWriter for AgentWriter<'a, W> {
    fn write(&mut self, data: Response) -> OrtResult<()> {
        match data {
            Response::Start => {}
            Response::Think(think) => {
                if self.show_reasoning {
                    match think {
                        ThinkEvent::Start => {
                            let _ = self.writer.write(MSG_THINK_START);
                            let _ = self.writer.flush();
                        }
                        ThinkEvent::Content(s) => {
                            let _ = self.writer.write_all(s.as_bytes());
                            let _ = self.writer.flush();
                        }
                        ThinkEvent::Stop => {
                            let _ = self.writer.write(MSG_THINK_END);
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
                let _ = self.writer.write(TOOL_CALL_START);
                let _ = self.writer.write(tool.name.as_bytes());
                let _ = self.writer.write(TOOL_CALL_ARGUMENT_START);
                let _ = self.writer.write(tool.arguments.trim().as_bytes());
                let _ = self.writer.write(TOOL_CALL_END);
                let _ = self.writer.flush();
            }
            Response::Annotation(_) => {
                // TODO
            }
            Response::Stats(mut stats) => {
                // Prevent timing display
                stats.time_to_first_token = None;

                // TODO: Align flush right
                let _ = self.writer.write(AGENT_STATS_START);
                let _ = self.writer.write(stats.as_string().as_bytes());
                let _ = self.writer.write(AGENT_STATS_END);
                let _ = self.writer.flush();
            }
            Response::Prompt(prompt) => {
                let _ = self.writer.write(PROMPT_START);
                let _ = self.writer.write(prompt.as_bytes());
                let _ = self.writer.write(RESET);
                let _ = self.writer.write(b"\n");
                let _ = self.writer.flush();
            }
            Response::Error(err_string) => {
                if err_string.contains(ERR_RATE_LIMITED) {
                    return Err(ort_error(ErrorKind::RateLimited, ""));
                }
                return Err(ort_err(ErrorKind::ResponseStreamError, err_string.into()));
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
