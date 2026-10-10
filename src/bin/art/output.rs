//! art: Open Router Agent
//! Part of the `ort` project
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::borrow::Cow;

use ort_openrouter_cli::{
    ErrorKind, Message, OrtResult, OutputWriter, Response, Section, ThinkEvent, Write, ort_err,
    ort_error, utils,
};

// No \n in these constants!
// That all goes in`section`

const THINK_START: &[u8] = "\x1b[0m\x1b[2m".as_bytes();

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

// These start with \n because are inside a section
const TOOL_REMOVED: &[u8] = "\n\x1b[48;5;88m".as_bytes();
const TOOL_ADDED: &[u8] = "\n\x1b[48;5;22m".as_bytes();

pub struct AgentWriter<'a, W: Write + Send> {
    writer: &'a mut W,
    show_reasoning: bool,
    context_size: usize,
    context_limit: Option<usize>,
    // Message bytes and Usage.total_tokens from the latest conversation request.
    // Summary requests have a different context and must not update this baseline.
    context_baseline: Option<(usize, u32)>,
    section: Section,
}

impl<'a, W: Write + Send> AgentWriter<'a, W> {
    pub fn new(
        writer: &'a mut W,
        show_reasoning: bool,
        context_limit: Option<usize>,
    ) -> AgentWriter<'a, W> {
        Self {
            writer,
            show_reasoning,
            context_size: 0,
            context_limit,
            context_baseline: None,
            section: Section::Prompt,
        }
    }

    /// Refresh from actual usage when available; otherwise estimate with the
    /// latest measured tokens/byte ratio (including after repeated compaction).
    pub fn update_context_size(&mut self, messages: &[Message], total_tokens: Option<u32>) {
        let bytes = messages
            .iter()
            .map(super::compact::estimated_bytes)
            .sum::<usize>();
        if let Some(tokens) = total_tokens.filter(|&tokens| tokens > 0) {
            self.context_baseline = Some((bytes, tokens));
            self.context_size = tokens as usize;
        } else {
            self.context_size = match self.context_baseline {
                Some((baseline_bytes, tokens)) if baseline_bytes > 0 => {
                    (bytes as u128 * tokens as u128 / baseline_bytes as u128) as usize
                }
                _ => bytes / 4,
            };
        }
    }

    fn section(&mut self, to_section: Section) {
        if self.section != to_section {
            Section::change(self.section, to_section, THINK_START, self.writer);
            self.section = to_section;
        } else {
            Section::same(self.section, self.writer);
        }
    }
}

impl<'a, W: Write + Send> OutputWriter for AgentWriter<'a, W> {
    fn write(&mut self, data: Response) -> OrtResult<()> {
        match data {
            // TODO: Should we show activity?
            Response::Connecting | Response::Start => {}
            Response::Think(think) => {
                if self.show_reasoning {
                    match think {
                        ThinkEvent::Start | ThinkEvent::Stop | ThinkEvent::Details(_) => {}
                        ThinkEvent::Content(s) => {
                            self.section(Section::Think);
                            let _ = self.writer.write_all(s.as_bytes());
                            let _ = self.writer.flush();
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
                if let Some(removed) = tool.removed {
                    let _ = self.writer.write(TOOL_REMOVED);
                    let _ = self.writer.write(truncate(&removed, 120).as_bytes());
                    let _ = self.writer.write(RESET);
                }
                if let Some(added) = tool.added {
                    let _ = self.writer.write(TOOL_ADDED);
                    let _ = self.writer.write(truncate(&added, 120).as_bytes());
                    let _ = self.writer.write(RESET);
                }
                let _ = self.writer.flush();
            }
            Response::Annotation(annotation) => {
                // These are url_citation from remote web_search tool
                self.section(Section::WebSearch);
                let _ = self
                    .writer
                    .writev(&[MSG_WEB_FETCH, annotation.citation_url().as_bytes()]);
            }
            Response::Stats(mut stats) => {
                self.section(Section::Stats);
                // Prevent timing display
                stats.time_to_first_token = None;

                let mut writes = Vec::with_capacity(8);

                // TODO: Align flush right
                writes.push(AGENT_STATS_START);
                let stats_s = stats.as_string();
                writes.push(stats_s.as_bytes());
                writes.push(b". Ctx: ");
                let context_size_h = utils::num_to_human_string(self.context_size);
                writes.push(context_size_h.as_bytes());
                let context_limit_h;
                if let Some(context_limit) = self.context_limit {
                    writes.push(b" / ");
                    context_limit_h = utils::num_to_human_string(context_limit);
                    writes.push(context_limit_h.as_bytes());
                }
                writes.push(b" tokens.");
                writes.push(AGENT_STATS_END);

                let _ = self.writer.writev(&writes);
                let _ = self.writer.flush();
            }
            Response::Prompt(prompt) => {
                self.section(Section::Prompt);
                let _ = self.writer.writev(&[
                    PROMPT_START,
                    prompt.trim_matches('\n').as_bytes(),
                    RESET,
                ]);
                let _ = self.writer.flush();
            }
            Response::Missing => {
                let _ = self.writer.write_char(MISSING_CHAR);
            }
            Response::Warn(warning) => {
                self.section(Section::Warn);
                let _ = self
                    .writer
                    .writev(&[WARN_START, warning.trim().as_bytes(), RESET]);
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

/// UTF-8 safe truncation
fn truncate(s: &str, max_chars: usize) -> Cow<'_, str> {
    if s.chars().count() <= max_chars {
        return Cow::Borrowed(s);
    }
    let end = s
        .char_indices()
        .nth(max_chars - 2)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    Cow::Owned(s[..end].to_string() + "..")
}

#[cfg(test)]
mod test {
    use crate::tools::{ActiveTool, ReadTool};

    use super::*;
    use core::time::Duration;
    use ort_openrouter_cli::{Annotation, Stats, StdoutWriter, ToolDisplay};

    #[test]
    fn context_compaction_uses_calibration_and_next_usage_refreshes_it() {
        let mut buffer = String::new();
        let mut writer = AgentWriter::new(&mut buffer, false, None);
        // Including message overhead, these are 400, 200 and 100 bytes.
        let original = vec![Message::user("x".repeat(368))];
        let compacted = vec![Message::user("x".repeat(168))];
        let compacted_again = vec![Message::user("x".repeat(68))];
        writer.update_context_size(&original, Some(200));
        writer.update_context_size(&compacted, None);
        assert_eq!(writer.context_size, 100);
        writer.update_context_size(&compacted_again, None);
        assert_eq!(writer.context_size, 50);
        writer.update_context_size(&compacted_again, Some(80));
        assert_eq!(writer.context_size, 80);
        writer.update_context_size(&compacted, None);
        assert_eq!(writer.context_size, 160);
    }

    #[test]
    fn stats_display_context_usage_and_configured_limit() {
        let mut buffer = String::new();
        let mut writer = AgentWriter::new(&mut buffer, false, Some(1_000_000));
        writer.context_size = 26_000;
        writer.write(Response::Stats(Stats::default())).unwrap();
        assert!(buffer.contains("Ctx: 26K / 1M tokens."));
    }

    #[test]
    fn context_missing_usage_estimates_growth_and_preserves_calibration() {
        let mut buffer = String::new();
        let mut writer = AgentWriter::new(&mut buffer, false, None);
        let mut messages = vec![Message::user("x".repeat(368))];
        writer.update_context_size(&messages, None);
        assert_eq!(writer.context_size, 100); // Initial bytes/4 fallback.
        writer.update_context_size(&messages, Some(200));
        writer.update_context_size(&messages, None);
        assert_eq!(writer.context_size, 200); // Failed/no-op compaction.
        messages.push(Message::tool("id".into(), "x".repeat(368)));
        writer.update_context_size(&messages, None);
        assert_eq!(writer.context_size, 400);
        writer.update_context_size(&messages, Some(0));
        assert_eq!(writer.context_size, 400); // Missing total_tokens defaults to zero.
    }

    // Test agent output to stdout, particularly new lines.
    // Run with `-- --nocapture` and eyeball it.
    #[test]
    fn test_output() {
        let read = ReadTool {
            path: "LICENSE".to_string(),
            offset: Some(100),
            limit: Some(200),
            line_numbers: true,
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
                removed: None,
                added: None,
            }),
            Response::ToolDisplay(ToolDisplay {
                name: "Bash ",
                arguments: r#"find . -name "*LICENS*""#.to_string(),
                extra: Some(" limit 10000".to_string()),
                removed: None,
                added: None,
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
        let mut aw = AgentWriter::new(&mut stdout_writer, true, None);
        for ev in events {
            let _ = aw.write(ev);
        }
    }
}
