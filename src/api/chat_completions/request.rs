//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025, 2026 Graham King

extern crate alloc;
use crate::api;
use alloc::string::String;

use crate::common::json_writer::{write_json_str, write_json_str_simple};
use crate::{
    Message, OrtResult, Write,
    common::{
        config::Cfg,
        data::{Content, Tool, ToolCall},
    },
};

/// Where we POST a completions request to. The part after `/v1/`.
pub(crate) const PROMPT_URL: &str = "/chat/completions";

/// Build the POST body
/// The system and user prompts must already be in messages.
pub fn build_body(
    idx: usize,
    cfg: &Cfg,
    messages: &[Message],
    // Does not include server-side tools like web_search
    client_tools: &[&'static Tool],
) -> OrtResult<String> {
    let mut string_buf = api::body_buffer(messages);
    let w = unsafe { string_buf.as_mut_vec() };
    api::write_request_options(idx, cfg, api::Format::Chat, w)?;

    w.write_str(", \"messages\":")?;
    Message::write_json_array(messages, w)?;

    // Some inference platforms reject empty tools
    if cfg.include_web_tools || !client_tools.is_empty() {
        w.write_str(", \"tools\":")?;
        api::write_tools(client_tools, cfg.include_web_tools, api::Format::Chat, w)?;
    }

    // I think PDFs are not sent natively to the model, they are pre-parsed by open router.
    // This disables that parsing. Experimental, does not help.
    // w.write_str(", \"plugins\": [{\"id\": \"file-parser\", \"pdf\": { \"engine\": \"native\" } }]")?;

    w.write_char('}')?;

    Ok(string_buf)
}

impl Message {
    pub fn write_json_array<W: Write>(msgs: &[Message], w: &mut W) -> OrtResult<()> {
        w.write_char('[')?;
        for (i, msg) in msgs.iter().enumerate() {
            if i != 0 {
                w.write_char(',')?;
            }
            write_json_message(msg, w)?;
        }
        w.write_char(']')?;
        Ok(())
    }
}

impl ToolCall {
    /// Write out a ToolCall as JSON. It looks like this:
    ///    {
    ///      "id": "call_abc123",
    ///      "type": "function",
    ///      "function": {
    ///        "name": "search_gutenberg_books",
    ///        "arguments": "{\"search_terms\": [\"James\", \"Joyce\"]}"
    ///      }
    ///    }
    pub fn write_json<W: Write>(&self, w: &mut W) -> OrtResult<()> {
        w.write_str(r#"{"id": "#)?;
        write_json_str_simple(w, self.id.as_deref().unwrap_or_default())?;

        w.write_str(r#", "type": "function", "function": {"name": "#)?;
        write_json_str_simple(w, &self.function.name)?;

        w.write_str(r#", "arguments": "#)?;
        write_json_str(w, &self.function.arguments)?;

        w.write_str("}}")?;

        Ok(())
    }
}

pub fn write_json_message<W: Write>(data: &Message, w: &mut W) -> OrtResult<()> {
    w.write_str("{\"role\":")?;
    write_json_str_simple(w, data.role.as_str())?;
    if let Some(tool_call_id) = &data.tool_call_id {
        w.write_str(",\"tool_call_id\":")?;
        write_json_str_simple(w, tool_call_id)?;
    }
    if !data.content.is_empty() {
        w.write_str(",\"content\":")?;
        match data.content.as_slice() {
            [Content::Text(text)] => write_json_str(w, text)?,
            _ => {
                w.write_char('[')?;
                for (i, item) in data.content.iter().enumerate() {
                    if i != 0 {
                        w.write_char(',')?;
                    }
                    api::write_content_part(item, api::Wrap::Chat, w)?;
                }
                w.write_char(']')?;
            }
        }
    }
    if let Some(reasoning) = &data.reasoning {
        w.write_str(",\"reasoning\":")?;
        write_json_str(w, reasoning)?;
    }
    if let Some(reasoning_details) = &data.reasoning_details {
        w.write_str(",\"reasoning_details\":")?;
        // reasoning_details is unparsed JSON
        w.write(reasoning_details.as_bytes())?;
    }
    if !data.tool_calls.is_empty() {
        w.write_str(",\"tool_calls\": [")?;
        for (i, tc) in data.tool_calls.iter().enumerate() {
            if i != 0 {
                w.write_char(',')?;
            }
            tc.write_json(w)?;
        }
        w.write_char(']')?;
    }

    w.write_char('}')?;
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::string::ToString;
    use alloc::vec;

    use super::*;
    use crate::common::config::DEFAULT_BASE_URL;

    #[test]
    fn test_build_body() {
        let cfg = Cfg {
            api_key: None,
            base_url: DEFAULT_BASE_URL.to_string(),
            dns: vec![],
            prompt: None,
            models: vec!["google/gemma-3n-e4b-it:free".to_string()],
            provider: Some("google-ai-studio".to_string()),
            system_prompt: Some("System prompt here".to_string()),
            priority: None,
            effort: None,
            context_size: None,
            show_reasoning: false,
            quiet: false,
            prompt_filename: None,
            files: vec![], // TODO
            include_web_tools: true,
            is_private: false,
            session_id: "test".to_string(),
        };
        let messages = vec![
            Message::user("Hello".to_string()),
            Message::assistant("Hello there!".to_string()),
        ];
        let got = match build_body(0, &cfg, &messages, &[]) {
            Ok(got) => got,
            Err(err) => {
                panic!("{}", err.as_string());
            }
        };

        let expected = r#"{"stream": true, "model": "google/gemma-3n-e4b-it:free", "provider": {"order": ["google-ai-studio"]}, "reasoning_effort": "none", "messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hello there!"}], "tools":[{"type": "openrouter:web_search"}, {"type": "openrouter:web_fetch"}]}"#;

        assert_eq!(got, expected);
    }
}
