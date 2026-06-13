//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::string::String;

use crate::{
    ErrorKind, Message, OrtResult, PromptOpts, Write,
    common::data::{Content, Tool, ToolCall, ToolParameter},
    ort_error,
};

/// Build the POST body
/// The system and user prompts must already by in messages.
pub fn build_body(
    idx: usize,
    opts: &PromptOpts,
    messages: &[Message],
    tools: &[&'static Tool],
) -> OrtResult<String> {
    // TODO: Add tools encoded byte size to avoid realloc
    let capacity: u32 = 1024 + messages.iter().map(|m| m.size()).sum::<u32>();
    let mut string_buf = String::with_capacity(capacity as usize);
    let w = unsafe { string_buf.as_mut_vec() };

    w.write_str("{\"stream\": true, \"model\": ")?;
    write_json_str(w, opts.models.get(idx).expect("Missing model"))?;

    if opts.priority.is_some() || opts.provider.is_some() {
        w.write_str(", \"provider\": {")?;
        let mut is_first = true;
        if let Some(p) = opts.priority {
            w.write_str("\"sort\":")?;
            write_json_str_simple(w, p.as_str())?;
            is_first = false;
        }
        if let Some(pr) = &opts.provider {
            if !is_first {
                w.write_str(", ")?;
            }
            w.write_str("\"order\": [")?;
            write_json_str(w, pr)?;
            w.write_char(']')?;
        }
        w.write_char('}')?;
    }

    w.write_str(", \"reasoning\": ")?;
    match &opts.reasoning {
        // No -r and nothing in config file
        None => {
            w.write_str("{\"enabled\": false}")?;
        }
        // cli "-r off" or config file '"enabled": false'
        Some(r_cfg) if !r_cfg.enabled => {
            w.write_str("{\"enabled\": false}")?;
        }
        // Reasoning on
        Some(r_cfg) => match (r_cfg.effort, r_cfg.tokens) {
            (Some(effort), _) => {
                w.write_str("{\"exclude\": false, \"enabled\": true, \"effort\":")?;
                write_json_str_simple(w, effort.as_str())?;
                w.write_char('}')?;
            }
            (_, Some(tokens)) => {
                w.write_str("{\"exclude\": false, \"enabled\": true, \"max_tokens\":")?;
                write_u32(w, tokens)?;
                w.write_char('}')?;
            }
            _ => unreachable!("Reasoning effort and tokens cannot both be null"),
        },
    };

    w.write_str(", \"messages\":")?;
    Message::write_json_array(messages, w)?;

    w.write_str(", \"tools\":")?;
    Tool::write_json_array(tools, w)?;

    // I think PDFs are not sent natively to the model, they are pre-parsed by open router.
    // This disables that parsing. Experimental, does not help.
    // w.write_str(", \"plugins\": [{\"id\": \"file-parser\", \"pdf\": { \"engine\": \"native\" } }]")?;

    w.write_char('}')?;

    Ok(string_buf)
}

impl PromptOpts {
    pub fn to_json_writer<W: Write>(&self, writer: &mut W) -> OrtResult<()> {
        let w = writer;

        w.write_char('{')?;
        let mut first = true;

        if let Some(ref v) = self.prompt {
            if !first {
                w.write_char(',')?;
            } else {
                first = false;
            }
            w.write_str("\"prompt\":")?;
            write_json_str(w, v)?;
        }
        // TODO: consider multi-model
        if let Some(v) = self.models.first() {
            if !first {
                w.write_char(',')?;
            } else {
                first = false;
            }
            w.write_str("\"model\":")?;
            write_json_str(w, v)?;
        }
        if let Some(ref v) = self.provider {
            if !first {
                w.write_char(',')?;
            } else {
                first = false;
            }
            w.write_str("\"provider\":")?;
            write_json_str(w, v)?;
        }
        if let Some(ref v) = self.system {
            if !first {
                w.write_char(',')?;
            } else {
                first = false;
            }
            w.write_str("\"system\":")?;
            write_json_str(w, v)?;
        }
        if let Some(ref p) = self.priority {
            if !first {
                w.write_char(',')?;
            } else {
                first = false;
            }
            w.write_str("\"priority\":")?;
            write_json_str_simple(w, p.as_str())?;
        }
        if let Some(ref rc) = self.reasoning {
            if !first {
                w.write_char(',')?;
            } else {
                first = false;
            }
            w.write_str("\"reasoning\":{")?;
            // always include enabled
            w.write_str("\"enabled\":")?;
            write_bool(w, rc.enabled)?;
            if let Some(ref eff) = rc.effort {
                w.write_str(",\"effort\":")?;
                write_json_str_simple(w, eff.as_str())?;
            }
            if let Some(tokens) = rc.tokens {
                w.write_str(",\"tokens\":")?;
                write_u32(w, tokens)?;
            }
            w.write_char('}')?;
        }
        if let Some(show) = self.show_reasoning {
            if !first {
                w.write_char(',')?;
            } else {
                first = false;
            }
            w.write_str("\"show_reasoning\":")?;
            write_bool(w, show)?;
        }
        if let Some(quiet) = self.quiet {
            if !first {
                w.write_char(',')?;
            } else {
                //first = false;
            }
            w.write_str("\"quiet\":")?;
            write_bool(w, quiet)?;
        }

        // merge_config
        w.write_char(',')?;
        w.write_str("\"merge_config\":")?;
        write_bool(w, self.merge_config)?;

        w.write_char('}')?;
        Ok(())
    }
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn write_bool<W: Write>(w: &mut W, v: bool) -> OrtResult<usize> {
    if v {
        w.write_str("true")
    } else {
        w.write_str("false")
    }
}

fn write_u32<W: Write>(w: &mut W, mut n: u32) -> OrtResult<usize> {
    if n == 0 {
        return w.write_str("0");
    }
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    w.write(&buf[i..])
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

impl Tool {
    pub fn write_json_array<W: Write>(tools: &[&'static Tool], w: &mut W) -> OrtResult<()> {
        w.write_char('[')?;
        for (i, tool) in tools.iter().enumerate() {
            if i != 0 {
                w.write_char(',')?;
            }
            tool.write_json(w)?;
        }
        w.write_char(']')?;
        Ok(())
    }

    pub fn write_json<W: Write>(&self, w: &mut W) -> OrtResult<()> {
        w.write_str(r#"{"type": "function", "function": {"name": "#)?;
        write_json_str_simple(w, self.name)?;

        w.write_str(r#", "description": "#)?;
        write_json_str(w, self.description)?;

        w.write_str(r#", "parameters": {"type": "object", "properties": {"#)?;
        for (idx, param) in self.parameters.iter().enumerate() {
            if idx != 0 {
                w.write_char(',')?;
            }
            param.write_json(w)?;
        }

        w.write_str(r#"}, "required": ["#)?;
        for (idx, req) in self.required_parameters.iter().enumerate() {
            if idx != 0 {
                w.write_char(',')?;
            }
            write_json_str_simple(w, req)?;
        }
        // Close the required params array,
        // the 'properties' object,
        // the 'function' object,
        // and the tool object.
        w.write_str("]}}}")?;
        Ok(())
    }
}

impl ToolParameter {
    fn write_json<W: Write>(&self, w: &mut W) -> OrtResult<()> {
        write_json_str_simple(w, self.name)?;
        w.write_str(r#": {"type": "#)?;
        write_json_str_simple(w, self.param_type)?;
        // TODO: support arrays. They need
        // "items": {"type": "string"},
        w.write_str(r#", "description": "#)?;
        write_json_str(w, self.description)?;
        w.write_char('}')?;
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
    if data.content.is_empty() && data.reasoning.is_none() && data.tool_calls.is_empty() {
        return Ok(());
    }
    w.write_str("{\"role\":")?;
    write_json_str_simple(w, data.role.as_str())?;
    if let Some(tool_call_id) = &data.tool_call_id {
        w.write_str(",\"tool_call_id\":")?;
        write_json_str_simple(w, tool_call_id)?;
    }
    match (&data.content, &data.reasoning) {
        (content, Some(_)) if !content.is_empty() => {
            return Err(ort_error(
                ErrorKind::InvalidMessageSchema,
                "Message must have exactly one of 'content' or 'reasoning'.",
            ));
        }
        (content, None) if content.is_empty() => {
            return Err(ort_error(
                ErrorKind::InvalidMessageSchema,
                "Message must have exactly one of 'content' or 'reasoning'.",
            ));
        }
        (_, Some(reasoning)) => {
            w.write_str(",\"reasoning\":")?;
            write_json_str(w, reasoning)?;
        }
        (content, _) => {
            w.write_str(",\"content\":")?;
            match content.as_slice() {
                [Content::Text(text)] => write_json_str(w, text)?,
                _ => {
                    w.write_char('[')?;
                    for (i, item) in content.iter().enumerate() {
                        if i != 0 {
                            w.write_char(',')?;
                        }
                        item.to_json(w)?;
                    }
                    w.write_char(']')?;
                }
            }
        }
    }
    if !data.tool_calls.is_empty() {
        w.write_str(",\"tool_calls\": [")?;
        for tc in &data.tool_calls {
            tc.write_json(w)?;
        }
        w.write_char(']')?;
    }

    w.write_char('}')?;
    Ok(())
}

impl Content {
    #[allow(dead_code)]
    pub fn to_json<W: Write>(&self, w: &mut W) -> OrtResult<()> {
        w.write_str("{\"type\":")?;
        use Content::*;
        match self {
            Text(s) => {
                write_json_str(w, "text")?;
                w.write_str(", \"text\": ")?;
                write_json_str(w, s.as_str())?;
            }
            Image { base64, mime_type } => {
                write_json_str(w, "image_url")?;
                w.write_str(", \"image_url\": { \"url\": \"data:")?;
                w.write_str(mime_type)?;
                w.write_str(";base64,")?; // end of the data: URL prefix
                w.write_str(base64.as_str())?;
                w.write_str("\"}")?;
            }
            ImageUrl(url) => {
                write_json_str(w, "image_url")?;
                w.write_str(", \"image_url\": { \"url\": \"")?;
                w.write_str(url)?;
                w.write_str("\"}")?;
            }
            File(f) => {
                write_json_str(w, "file")?;
                w.write_str(", \"file\": {\"filename\": ")?;
                write_json_str(w, &f.filename)?;
                // TODO: Support non-PDF, or restrict -f to PDF
                w.write_str(", \"file_data\": \"data:application/pdf;base64,")?;
                w.write_str(&f.base64)?;
                w.write_str("\"}")?;
            }
        }
        w.write_char('}')?;
        Ok(())
    }
}

/// No escapes or special characters, just write the bytes
pub(crate) fn write_json_str_simple<W: Write>(w: &mut W, s: &str) -> OrtResult<()> {
    w.write_char('"')?;
    w.write_str(s)?;
    w.write_char('"')?;
    Ok(())
}

// Writes a JSON string (with surrounding quotes) with proper escaping, no allocations.
pub fn write_json_str<W: Write>(w: &mut W, s: &str) -> OrtResult<()> {
    w.write_char('"')?;
    write_encoded_bytes(w, s.as_bytes())?;
    w.write_char('"')?;

    Ok(())
}

pub fn write_encoded_bytes<W: Write>(w: &mut W, bytes: &[u8]) -> OrtResult<()> {
    let mut start = 0;

    for i in 0..bytes.len() {
        let b = bytes[i];
        let esc = match b {
            b'"' => Some(b"\\\""), // as &[u8]),
            b'\\' => Some(b"\\\\"),
            b'\n' => Some(b"\\n"),
            b'\r' => Some(b"\\r"),
            b'\t' => Some(b"\\t"),
            0x08 => Some(b"\\b"),
            0x0C => Some(b"\\f"),
            0x00..=0x1F => None, // will use \u00XX
            _ => continue,
        };

        if start < i {
            w.write(&bytes[start..i])?;
        }

        if let Some(e) = esc {
            w.write(e)?;
        } else {
            // Generic control char: \u00XX
            let mut buf = [0u8; 6];
            buf[0] = b'\\';
            buf[1] = b'u';
            buf[2] = b'0';
            buf[3] = b'0';
            buf[4] = HEX[((b >> 4) & 0xF) as usize];
            buf[5] = HEX[(b & 0xF) as usize];
            w.write(&buf)?;
        }

        start = i + 1;
    }

    if start < bytes.len() {
        w.write(&bytes[start..])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::string::ToString;
    use alloc::vec;

    use super::*;
    use crate::ReasoningConfig;
    use crate::common::tools::ALL_TOOLS;

    #[test]
    fn test_build_body() {
        let opts = PromptOpts {
            prompt: None,
            models: vec!["google/gemma-3n-e4b-it:free".to_string()],
            provider: Some("google-ai-studio".to_string()),
            system: Some("System prompt here".to_string()),
            priority: None,
            reasoning: Some(ReasoningConfig::off()),
            show_reasoning: Some(false),
            quiet: None,
            merge_config: false,
            prompt_filename: None,
            files: vec![], // TODO
        };
        let messages = vec![
            Message::user("Hello".to_string()),
            Message::assistant("Hello there!".to_string()),
        ];
        let got = match build_body(0, &opts, &messages, &[&ALL_TOOLS[0]]) {
            Ok(got) => got,
            Err(err) => {
                panic!("{}", err.as_string());
            }
        };

        let expected = r#"{"stream": true, "model": "google/gemma-3n-e4b-it:free", "provider": {"order": ["google-ai-studio"]}, "reasoning": {"enabled": false}, "messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hello there!"}], "tools":[{"type": "function", "function": {"name": "read", "description": "Read the contents of a text file.", "parameters": {"type": "object", "properties": {"path": {"type": "string", "description": "Path to the file to read (relative or absolute)"},"offset": {"type": "number", "description": "Line number to start reading from (1-indexed)"},"limit": {"type": "number", "description": "Maximum number of lines to read"}}, "required": ["path"]}}}]}"#;

        assert_eq!(got, expected);
    }
}
