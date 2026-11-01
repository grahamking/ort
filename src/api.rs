//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

//! Abstract over chat completions and responses APIs.

extern crate alloc;
use alloc::string::String;

use crate::common::{
    config::Cfg,
    data::{Content, Tool, ToolParameter},
    json_writer::{write_json_str, write_json_str_simple},
};
use crate::{Message, OrtResult, ReasoningEffort, Write};

// Both are always compiled. Only the `selected` alias below picks one, so the
// feature flag is checked in a single place. The inactive module is dead code,
// which the linker drops, so it costs no binary size.
#[cfg_attr(feature = "responses", allow(dead_code))]
mod chat_completions;
#[cfg_attr(not(feature = "responses"), allow(dead_code))]
mod responses;

#[cfg(not(feature = "responses"))]
use chat_completions as selected;
#[cfg(feature = "responses")]
use responses as selected;

pub(crate) use selected::PROMPT_URL;
pub use selected::build_body;

/// Writing `last.json` always uses the chat completions format, whatever we
/// send to the server, so this is not feature gated.
pub(crate) use chat_completions::write_json_message;

#[derive(Clone, Copy)]
enum Format {
    Chat,
    Responses,
}

fn body_buffer(messages: &[Message]) -> String {
    // Leave room for request options and message framing. Payloads are written
    // directly into this buffer, including large base64 attachments.
    // TODO: Add tools encoded byte size to avoid realloc.
    let capacity = 1024 + messages.iter().map(Message::size).sum::<u32>();
    String::with_capacity(capacity as usize)
}

fn write_request_options<W: Write>(
    idx: usize,
    cfg: &Cfg,
    format: Format,
    w: &mut W,
) -> OrtResult<()> {
    w.write_str("{\"stream\": true, \"model\": ")?;
    write_json_str(w, cfg.models.get(idx).expect("Missing model"))?;

    if cfg.priority.is_some() || cfg.provider.is_some() {
        w.write_str(", \"provider\": {")?;
        if let Some(p) = cfg.priority {
            w.write_str("\"sort\":")?;
            write_json_str_simple(w, p.as_str())?;
        }
        if let Some(provider) = &cfg.provider {
            if cfg.priority.is_some() {
                w.write_str(", ")?;
            }
            w.write_str("\"order\": [")?;
            write_json_str(w, provider)?;
            w.write_char(']')?;
        }
        w.write_char('}')?;
    }

    w.write_str(match format {
        Format::Chat => ", \"reasoning_effort\": ",
        Format::Responses => ", \"reasoning\": {\"effort\": ",
    })?;
    write_json_str_simple(w, cfg.effort.unwrap_or(ReasoningEffort::None).as_str())?;
    if matches!(format, Format::Responses) {
        w.write_char('}')?;
    }
    Ok(())
}

const WEB_TOOLS: &str = r#"{"type": "openrouter:web_search"}, {"type": "openrouter:web_fetch"}"#;

fn write_tools<W: Write>(
    tools: &[&'static Tool],
    include_web_tools: bool,
    format: Format,
    w: &mut W,
) -> OrtResult<()> {
    w.write_char('[')?;
    if include_web_tools {
        w.write_str(WEB_TOOLS)?;
    }
    for (i, tool) in tools.iter().enumerate() {
        if include_web_tools || i != 0 {
            w.write_char(',')?;
        }
        w.write_str(r#"{"type": "function", "#)?;
        if matches!(format, Format::Chat) {
            w.write_str(r#""function": {"#)?;
        }
        write_tool_fields(tool, w)?;
        if matches!(format, Format::Chat) {
            w.write_char('}')?;
        }
        w.write_char('}')?;
    }
    w.write_char(']')?;
    Ok(())
}

fn write_tool_fields<W: Write>(tool: &Tool, w: &mut W) -> OrtResult<()> {
    w.write_str(r#""name": "#)?;
    write_json_str_simple(w, tool.name)?;
    w.write_str(r#", "description": "#)?;
    write_json_str(w, tool.description)?;
    w.write_str(r#", "parameters": {"type": "object", "properties": {"#)?;
    for (idx, param) in tool.parameters.iter().enumerate() {
        if idx != 0 {
            w.write_char(',')?;
        }
        param.write_json(w)?;
    }
    w.write_str(r#"}, "required": ["#)?;
    for (idx, req) in tool.required_parameters.iter().enumerate() {
        if idx != 0 {
            w.write_char(',')?;
        }
        write_json_str_simple(w, req)?;
    }
    w.write_str("]}")?;
    Ok(())
}

impl ToolParameter {
    fn write_json<W: Write>(&self, w: &mut W) -> OrtResult<()> {
        write_json_str_simple(w, self.name)?;
        w.write_str(r#": {"type": "#)?;
        write_json_str_simple(w, self.param_type)?;
        // TODO: support arrays, which need an "items" schema.
        w.write_str(r#", "description": "#)?;
        write_json_str(w, self.description)?;
        w.write_char('}')?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Wrap {
    /// `{"type":"text", ...}` nesting `image_url` and `file` objects.
    Chat,
    /// `{"type":"input_text", ...}`; images and files are flat.
    InputText,
    /// `{"type":"output_text", ...}`; otherwise as `InputText`.
    OutputText,
}

impl Wrap {
    /// Responses puts image and file fields at the top level; chat nests them.
    fn chat(self) -> bool {
        matches!(self, Wrap::Chat)
    }

    fn text_type(self) -> &'static str {
        match self {
            Wrap::Chat => "text",
            Wrap::InputText => "input_text",
            Wrap::OutputText => "output_text",
        }
    }
}

fn write_content_part<W: Write>(content: &Content, wrap: Wrap, w: &mut W) -> OrtResult<()> {
    match content {
        Content::Text(text) => {
            w.write_str("{\"type\":")?;
            write_json_str_simple(w, wrap.text_type())?;
            w.write_str(", \"text\": ")?;
            write_json_str(w, text)?;
        }
        Content::Image { .. } | Content::ImageUrl(_) => {
            w.write_str(if wrap.chat() {
                r#"{"type":"image_url", "image_url": { "url": "#
            } else {
                r#"{"type":"input_image", "image_url": "#
            })?;
            match content {
                Content::Image { base64, mime_type } => {
                    w.write_str("\"data:")?;
                    w.write_str(mime_type)?;
                    w.write_str(";base64,")?;
                    w.write_str(base64)?;
                    w.write_char('"')?;
                }
                Content::ImageUrl(url) => write_json_str(w, url)?,
                _ => unreachable!(),
            }
            if wrap.chat() {
                w.write_char('}')?;
            }
        }
        Content::File(file) => {
            w.write_str(if wrap.chat() {
                r#"{"type":"file", "file": {"filename": "#
            } else {
                r#"{"type":"input_file", "filename": "#
            })?;
            write_json_str(w, &file.filename)?;
            // TODO: Support non-PDF, or restrict -f to PDF.
            w.write_str(", \"file_data\": \"data:application/pdf;base64,")?;
            w.write_str(&file.base64)?;
            w.write_char('"')?;
            if wrap.chat() {
                w.write_char('}')?;
            }
        }
    }
    w.write_char('}')?;
    Ok(())
}

/// Join text parts without allocating a temporary string (tool outputs are strings).
fn write_text_content<W: Write>(content: &[Content], w: &mut W) -> OrtResult<()> {
    w.write_char('"')?;
    for part in content {
        if let Content::Text(text) = part {
            crate::common::json_writer::write_encoded_bytes(w, text.as_bytes())?;
        }
    }
    w.write_char('"')?;
    Ok(())
}
