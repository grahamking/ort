//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;
use alloc::string::String;

use crate::api::{
    Format, Wrap, body_buffer, write_content_part, write_request_options, write_text_content,
    write_tools,
};
use crate::common::{
    config::Cfg,
    data::{Content, Tool},
    json_writer::{write_json_str, write_json_str_simple},
};
use crate::{Message, OrtResult, Role, Write};

/// Where we POST a Responses request to. The part after `/v1/`.
pub(crate) const PROMPT_URL: &str = "/responses";

/// Build the POST body. System and user prompts must already be in messages.
pub fn build_body(
    idx: usize,
    cfg: &Cfg,
    messages: &[Message],
    // Does not include server-side tools like web_search.
    client_tools: &[&'static Tool],
) -> OrtResult<String> {
    let mut string_buf = body_buffer(messages);
    let w = unsafe { string_buf.as_mut_vec() };
    write_request_options(idx, cfg, Format::Responses, w)?;
    w.write_str(", \"input\":[")?;
    let mut first = true;
    for message in messages {
        write_input(message, &mut first, w)?;
    }
    w.write_char(']')?;
    // Some inference platforms reject empty tools.
    if cfg.include_web_tools || !client_tools.is_empty() {
        w.write_str(", \"tools\":")?;
        write_tools(client_tools, cfg.include_web_tools, Format::Responses, w)?;
    }
    w.write_char('}')?;
    Ok(string_buf)
}

fn separator<W: Write>(first: &mut bool, w: &mut W) -> OrtResult<()> {
    if !*first {
        w.write_char(',')?;
    }
    *first = false;
    Ok(())
}

fn write_input<W: Write>(message: &Message, first: &mut bool, w: &mut W) -> OrtResult<()> {
    if matches!(message.role, Role::Tool) {
        separator(first, w)?;
        w.write_str(r#"{"type":"function_call_output","call_id":"#)?;
        write_json_str(w, message.tool_call_id.as_deref().unwrap_or_default())?;
        w.write_str(r#", "output":"#)?;
        write_text_content(&message.content, w)?;
        w.write_char('}')?;
        return Ok(());
    }

    let wrap = if matches!(message.role, Role::Assistant) {
        Wrap::OutputText
    } else {
        Wrap::InputText
    };
    if !message.content.is_empty() || message.tool_calls.is_empty() {
        separator(first, w)?;
        w.write_str(r#"{"role":"#)?;
        write_json_str_simple(w, message.role.as_str())?;
        w.write_str(r#", "content":"#)?;
        match message.content.as_slice() {
            [Content::Text(text)] => write_json_str(w, text)?,
            parts => {
                w.write_char('[')?;
                for (i, part) in parts.iter().enumerate() {
                    if i != 0 {
                        w.write_char(',')?;
                    }
                    write_content_part(part, wrap, w)?;
                }
                w.write_char(']')?;
            }
        }
        w.write_char('}')?;
    }

    // Responses function calls are input items not nested message fields.
    for call in &message.tool_calls {
        separator(first, w)?;
        w.write_str(r#"{"type":"function_call","call_id":"#)?;
        write_json_str(w, call.id.as_deref().unwrap_or_default())?;
        w.write_str(r#", "name":"#)?;
        write_json_str(w, &call.function.name)?;
        w.write_str(r#", "arguments":"#)?;
        write_json_str(w, &call.function.arguments)?;
        w.write_char('}')?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::data::{Function, PromptFile, ToolCall};
    use crate::{Priority, ReasoningEffort, ToolParameter};
    use alloc::{string::ToString, vec};

    fn cfg() -> Cfg {
        Cfg {
            models: vec!["model".to_string(), "other".to_string()],
            ..Cfg::default()
        }
    }

    fn body(cfg: &Cfg, messages: &[Message], tools: &[&'static Tool]) -> String {
        build_body(0, cfg, messages, tools).unwrap_or_else(|err| panic!("{}", err.as_string()))
    }

    #[test]
    fn text_and_options() {
        let cfg = Cfg {
            priority: Some(Priority::Latency),
            provider: Some("provider\"name".to_string()),
            effort: Some(ReasoningEffort::High),
            ..cfg()
        };
        let messages = [
            Message::system("Be helpful".to_string()),
            Message::user("Hello\n\"世界\"".to_string()),
            Message::assistant("Hi".to_string()),
        ];
        assert_eq!(
            body(&cfg, &messages, &[]),
            r#"{"stream": true, "model": "model", "provider": {"sort":"latency", "order": ["provider\"name"]}, "reasoning": {"effort": "high"}, "input":[{"role":"system", "content":"Be helpful"},{"role":"user", "content":"Hello\n\"世界\""},{"role":"assistant", "content":"Hi"}]}"#
        );
    }

    #[test]
    fn empty_input_and_model_index() {
        assert_eq!(
            build_body(1, &cfg(), &[], &[]).unwrap_or_else(|err| panic!("{}", err.as_string())),
            r#"{"stream": true, "model": "other", "reasoning": {"effort": "none"}, "input":[]}"#
        );
    }

    #[test]
    fn tools() {
        static TOOL: Tool = Tool {
            name: "test",
            description: "A \"test\"",
            parameters: &[ToolParameter {
                name: "arg",
                description: "Line\nbreak",
                param_type: "string",
            }],
            required_parameters: &["arg"],
        };
        let cfg = Cfg {
            include_web_tools: true,
            ..cfg()
        };
        assert_eq!(
            body(&cfg, &[], &[&TOOL]),
            r#"{"stream": true, "model": "model", "reasoning": {"effort": "none"}, "input":[], "tools":[{"type": "openrouter:web_search"}, {"type": "openrouter:web_fetch"},{"type": "function", "name": "test", "description": "A \"test\"", "parameters": {"type": "object", "properties": {"arg": {"type": "string", "description": "Line\nbreak"}}, "required": ["arg"]}}]}"#
        );
        let without_web = Cfg {
            include_web_tools: false,
            ..cfg
        };
        assert!(!body(&without_web, &[], &[&TOOL]).contains("openrouter:"));
    }

    #[test]
    fn tool_round_trip() {
        let mut assistant = Message::assistant("Checking".to_string());
        assistant.reasoning = Some("not a Responses item".to_string());
        assistant.reasoning_details = Some("[]".to_string());
        assistant.tool_calls = vec![
            ToolCall {
                id: Some("call_1".to_string()),
                function: Function {
                    name: "test".to_string(),
                    arguments: "{\"arg\":\"value\"}".to_string(),
                },
                ..ToolCall::default()
            },
            ToolCall {
                id: None,
                ..ToolCall::default()
            },
        ];
        let mut output = Message::new(Role::Tool, None, None);
        output.tool_call_id = Some("call_1".to_string());
        output.content = vec![
            Content::Text("a\n".to_string()),
            Content::Text("\"b\"".to_string()),
        ];
        assert_eq!(
            body(&cfg(), &[assistant.clone(), output], &[]),
            r#"{"stream": true, "model": "model", "reasoning": {"effort": "none"}, "input":[{"role":"assistant", "content":"Checking"},{"type":"function_call","call_id":"call_1", "name":"test", "arguments":"{\"arg\":\"value\"}"},{"type":"function_call","call_id":"", "name":"", "arguments":""},{"type":"function_call_output","call_id":"call_1", "output":"a\n\"b\""}]}"#
        );
        assistant.content.clear();
        assert!(!body(&cfg(), &[assistant], &[]).contains("\"role\""));
    }

    #[test]
    fn multimodal_content() {
        let mut message = Message::user("Look".to_string());
        message.content.extend([
            Content::Image {
                mime_type: "image/png",
                base64: "aGVsbG8=".to_string(),
            },
            Content::ImageUrl("https://example.com/\"image\"".to_string()),
            Content::File(PromptFile::from_parts(
                crate::common::data::PromptFileKind::File,
                "file\".pdf".to_string(),
                "cGRm".to_string(),
            )),
        ]);
        assert_eq!(
            body(&cfg(), &[message], &[]),
            r#"{"stream": true, "model": "model", "reasoning": {"effort": "none"}, "input":[{"role":"user", "content":[{"type":"input_text", "text": "Look"},{"type":"input_image", "image_url": "data:image/png;base64,aGVsbG8="},{"type":"input_image", "image_url": "https://example.com/\"image\""},{"type":"input_file", "filename": "file\".pdf", "file_data": "data:application/pdf;base64,cGRm"}]}]}"#
        );
        let mut assistant = Message::assistant("a".to_string());
        assistant.content.push(Content::Text("b".to_string()));
        assert!(body(&cfg(), &[assistant], &[]).contains("\"type\":\"output_text\""));
    }
}
