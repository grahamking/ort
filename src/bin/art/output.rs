//! art: Open Router Agent
//! Part of the `ort` project
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use ort_openrouter_cli::{
    ErrorKind, OrtResult, OutputWriter, Response, ThinkEvent, Write, ort_err, ort_error,
};

// No \n in these constants!
// That all goes in`section`

const MSG_THINK_START: &[u8] = "\x1b[2m".as_bytes();
const MSG_THINK_END: &[u8] = "\x1b[0m".as_bytes();

const TOOL_CALL_START: &[u8] = "\x1b[0m".as_bytes();
const TOOL_CALL_ARGUMENT_START: &[u8] = "\x1b[96m".as_bytes();
const TOOL_CALL_END: &[u8] = "\x1b[0m".as_bytes();

const AGENT_STATS_START: &[u8] = "\x1b[35m".as_bytes();
const AGENT_STATS_END: &[u8] = "\x1b[0m".as_bytes();

const PROMPT_START: &[u8] = "\x1b[3m".as_bytes();
const MSG_WEB_FETCH: &[u8] = "\x1b[0m\x1b[2mWeb search: \x1b[0m".as_bytes();

const ERR_RATE_LIMITED: &str = "429 Too Many Requests";
const RESET: &[u8] = "\x1b[0m".as_bytes();
const WARN_START: &[u8] = "\x1b[38;5;208m".as_bytes();

const MISSING_CHAR: char = '□';

#[derive(PartialEq, Eq)]
enum Section {
    Prompt,
    Think,
    WebSearch,
    Tool,
    Content,
    Stats,
    Warn,
}

pub struct AgentWriter<'a, W: Write + Send> {
    writer: &'a mut W,
    show_reasoning: bool,
    section: Section,
}

impl<'a, W: Write + Send> AgentWriter<'a, W> {
    pub fn new(writer: &'a mut W, show_reasoning: bool) -> AgentWriter<'a, W> {
        Self {
            writer,
            show_reasoning,
            section: Section::Prompt,
        }
    }

    fn section(&mut self, to_section: Section) {
        if self.section == to_section {
            match to_section {
                Section::WebSearch | Section::Tool => {
                    // These must go one per line
                    let _ = self.writer.write_char('\n');
                }
                _ => {}
            }
            return;
        }
        // Blank line between each section
        let _ = self.writer.write(b"\n\n");
        self.section = to_section;
    }
}

impl<'a, W: Write + Send> OutputWriter for AgentWriter<'a, W> {
    fn write(&mut self, data: Response) -> OrtResult<()> {
        match data {
            Response::Start => {}
            Response::Think(think) => {
                if self.show_reasoning {
                    self.section(Section::Think);
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
                        }
                    }
                }
            }
            Response::Content(content) => {
                self.section(Section::Content);
                let _ = self.writer.write_all(content.as_bytes());
            }
            Response::ToolCalls(_tool_calls) => {
                // We use ToolDisplay instead
            }
            Response::ToolDisplay(tool) => {
                self.section(Section::Tool);
                let _ = self.writer.write(TOOL_CALL_START);
                let _ = self.writer.write(tool.name.as_bytes());
                let _ = self.writer.write(TOOL_CALL_ARGUMENT_START);
                let _ = self.writer.write(tool.arguments.trim().as_bytes());
                let _ = self.writer.write(TOOL_CALL_END);
                if let Some(extra) = tool.extra {
                    let _ = self.writer.write(extra.as_bytes());
                }
                let _ = self.writer.flush();
            }
            Response::Annotation(annotation) => {
                // These are url_citation from remote web_search tool
                self.section(Section::WebSearch);
                let _ = self.writer.write(MSG_WEB_FETCH);
                let _ = self.writer.write(annotation.citation_url().as_bytes());
            }
            Response::Stats(mut stats) => {
                self.section(Section::Stats);
                // Prevent timing display
                stats.time_to_first_token = None;

                // TODO: Align flush right
                let _ = self.writer.write(AGENT_STATS_START);
                let _ = self.writer.write(stats.as_string().as_bytes());
                let _ = self.writer.write(AGENT_STATS_END);
                let _ = self.writer.flush();
            }
            Response::Prompt(prompt) => {
                self.section(Section::Prompt);
                let _ = self.writer.write(PROMPT_START);
                let _ = self.writer.write(prompt.trim().as_bytes());
                let _ = self.writer.write(RESET);
                let _ = self.writer.flush();
            }
            Response::Missing => {
                let _ = self.writer.write_char(MISSING_CHAR);
            }
            Response::Warn(warning) => {
                self.section(Section::Warn);
                let _ = self.writer.write(WARN_START);
                let _ = self.writer.write(warning.trim().as_bytes());
                let _ = self.writer.write(RESET);
                let _ = self.writer.flush();
            }
            Response::Error(err_string) => {
                if err_string.contains(ERR_RATE_LIMITED) {
                    return Err(ort_error(ErrorKind::RateLimited, ""));
                }
                return Err(ort_err(ErrorKind::ResponseStreamError, err_string.into()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::tools::{ActiveTool, ReadTool};

    use super::*;
    use core::time::Duration;
    use ort_openrouter_cli::{Annotation, Stats, StdoutWriter, ToolDisplay};

    // Test agent output to stdout, particularly new lines.
    // Run with `-- --nocapture` and eyeball it.
    #[test]
    fn test_output() {
        let read = ReadTool {
            path: "LICENSE".to_string(),
            offset: Some(100),
            limit: Some(200),
        };
        let events = [
            Response::Prompt("What is the license of this project?".to_string()),
            Response::Start,
            Response::Think(ThinkEvent::Start),
            Response::Think(ThinkEvent::Content("Search a bit first".to_string())),
            Response::Annotation(Annotation::UrlCitation {
                url: "http://ort.example".to_string(),
                content: String::new(),
            }),
            Response::Annotation(Annotation::UrlCitation {
                url: "http://ort.example/other".to_string(),
                content: String::new(),
            }),
            Response::Think(ThinkEvent::Content(
                "We need to find license file. Use bash to list.".to_string(),
            )),
            Response::Warn("Tool does not exist. No such tool: 'bosh'".to_string()),
            Response::ToolDisplay(ToolDisplay {
                name: "Bash ",
                arguments: "ls -R".to_string(),
                extra: None,
            }),
            Response::ToolDisplay(ToolDisplay {
                name: "Bash ",
                arguments: r#"find . -name "*LICENS*""#.to_string(),
                extra: Some(" limit 10000".to_string()),
            }),
            Response::Start,
            Response::Think(ThinkEvent::Start),
            Response::Think(ThinkEvent::Content(
                "We need license file. Look at LICENSE.".to_string(),
            )),
            Response::ToolDisplay(read.display()),
            Response::Think(ThinkEvent::Start),
            Response::Think(ThinkEvent::Content("The license is MIT.".to_string())),
            Response::Think(ThinkEvent::Stop),
            Response::Content("The".to_string()),
            Response::Content(" project".to_string()),
            Response::Content(" is".to_string()),
            Response::Content(" licensed".to_string()),
            Response::Stats(Stats {
                used_model: "openai/gpt-oss-120b".to_string(),
                provider: "OpenAI".to_string(),
                cost_in_cents: Some(0.6020),
                elapsed_time: Duration::from_secs(24),
                ..Default::default()
            }),
        ];

        let mut stdout_writer = StdoutWriter {};
        let mut aw = AgentWriter::new(&mut stdout_writer, true);
        for ev in events {
            let _ = aw.write(ev);
        }
    }
}
