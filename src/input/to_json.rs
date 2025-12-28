//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::string::String;

use crate::{LastData, Message, OrtResult, PromptOpts, Write, ort_err};

/// Build the POST body
/// The system and user prompts must already by in messages.
pub fn build_body(idx: usize, opts: &PromptOpts, messages: &[Message]) -> OrtResult<String> {
    let capacity: u32 = messages.iter().map(|m| m.size()).sum::<u32>() + 100;
    let mut string_buf = String::with_capacity(capacity as usize);
    let mut w = unsafe { string_buf.as_mut_vec() };

    w.write_str("{\"stream\": true, \"usage\": {\"include\": true}, \"model\": ")?;
    write_json_str(&mut w, opts.models.get(idx).expect("Missing model"))?;

    if opts.priority.is_some() || opts.provider.is_some() {
        w.write_str(", \"provider\": {")?;
        let mut is_first = true;
        if let Some(p) = opts.priority {
            w.write_str("\"sort\":")?;
            write_json_str_simple(&mut w, p.as_str())?;
            is_first = false;
        }
        if let Some(pr) = &opts.provider {
            if !is_first {
                w.write_str(", ")?;
            }
            w.write_str("\"order\": [")?;
            write_json_str(&mut w, pr)?;
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
                write_json_str_simple(&mut w, effort.as_str())?;
                w.write_char('}')?;
            }
            (_, Some(tokens)) => {
                w.write_str("{\"exclude\": false, \"enabled\": true, \"max_tokens\":")?;
                write_u32(&mut w, tokens)?;
                w.write_char('}')?;
            }
            _ => unreachable!("Reasoning effort and tokens cannot both be null"),
        },
    };

    w.write_str(", \"messages\":")?;
    Message::write_json_array(messages, &mut w)?;

    w.write_char('}')?;

    Ok(string_buf)
}

impl LastData {
    pub fn to_json_writer<W: Write>(&self, writer: W) -> OrtResult<()> {
        let mut w = writer;

        w.write_str("{\"opts\":{")?;
        let mut first = true;

        if let Some(ref v) = self.opts.prompt {
            if !first {
                w.write_char(',')?;
            } else {
                first = false;
            }
            w.write_str("\"prompt\":")?;
            write_json_str(&mut w, v)?;
        }
        // TODO: consider multi-model
        if let Some(v) = self.opts.models.first() {
            if !first {
                w.write_char(',')?;
            } else {
                first = false;
            }
            w.write_str("\"model\":")?;
            write_json_str(&mut w, v)?;
        }
        if let Some(ref v) = self.opts.provider {
            if !first {
                w.write_char(',')?;
            } else {
                first = false;
            }
            w.write_str("\"provider\":")?;
            write_json_str(&mut w, v)?;
        }
        if let Some(ref v) = self.opts.system {
            if !first {
                w.write_char(',')?;
            } else {
                first = false;
            }
            w.write_str("\"system\":")?;
            write_json_str(&mut w, v)?;
        }
        if let Some(ref p) = self.opts.priority {
            if !first {
                w.write_char(',')?;
            } else {
                first = false;
            }
            w.write_str("\"priority\":")?;
            write_json_str_simple(&mut w, p.as_str())?;
        }
        if let Some(ref rc) = self.opts.reasoning {
            if !first {
                w.write_char(',')?;
            } else {
                first = false;
            }
            w.write_str("\"reasoning\":{")?;
            // always include enabled
            w.write_str("\"enabled\":")?;
            write_bool(&mut w, rc.enabled)?;
            if let Some(ref eff) = rc.effort {
                w.write_str(",\"effort\":")?;
                write_json_str_simple(&mut w, eff.as_str())?;
            }
            if let Some(tokens) = rc.tokens {
                w.write_str(",\"tokens\":")?;
                write_u32(&mut w, tokens)?;
            }
            w.write_char('}')?;
        }
        if let Some(show) = self.opts.show_reasoning {
            if !first {
                w.write_char(',')?;
            } else {
                first = false;
            }
            w.write_str("\"show_reasoning\":")?;
            write_bool(&mut w, show)?;
        }
        if let Some(quiet) = self.opts.quiet {
            if !first {
                w.write_char(',')?;
            } else {
                //first = false;
            }
            w.write_str("\"quiet\":")?;
            write_bool(&mut w, quiet)?;
        }

        // merge_config
        w.write_char(',')?;
        w.write_str("\"merge_config\":")?;
        write_bool(&mut w, self.opts.merge_config)?;

        w.write_str("},\"messages\":")?;
        Message::write_json_array(&self.messages, &mut w)?;

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
    let s = core::str::from_utf8(&buf[i..]).unwrap();
    w.write_str(s)
}

impl Message {
    pub fn write_json_array<W: Write>(msgs: &[Message], w: &mut W) -> OrtResult<()> {
        w.write_char('[')?;
        for (i, msg) in msgs.iter().enumerate() {
            if i != 0 {
                w.write_char(',')?;
            }
            write_json(msg, w)?;
        }
        w.write_char(']')?;
        Ok(())
    }
}

pub fn write_json<W: Write>(data: &Message, w: &mut W) -> OrtResult<()> {
    w.write_str("{\"role\":")?;
    write_json_str_simple(w, data.role.as_str())?;
    match (&data.content, &data.reasoning) {
        (Some(_), Some(_)) | (None, None) => {
            return ort_err("Message must have exactly one of 'content' or 'reasoning'.");
        }
        (Some(content), _) => {
            w.write_str(",\"content\":")?;
            write_json_str(w, content)?;
        }
        (_, Some(reasoning)) => {
            w.write_str(",\"reasoning\":")?;
            write_json_str(w, reasoning)?;
        }
    }
    w.write_char('}')?;
    Ok(())
}

/// No escapes or special characters, just write the bytes
fn write_json_str_simple<W: Write>(w: &mut W, s: &str) -> OrtResult<()> {
    w.write_char('"')?;
    w.write_str(s)?;
    w.write_char('"')?;
    Ok(())
}

// Writes a JSON string (with surrounding quotes) with proper escaping, no allocations.
fn write_json_str<W: Write>(w: &mut W, s: &str) -> OrtResult<()> {
    w.write_char('"')?;
    let bytes = s.as_bytes();
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
            w.write_str(core::str::from_utf8(&bytes[start..i]).unwrap())?;
        }

        if let Some(e) = esc {
            w.write_str(core::str::from_utf8(e).unwrap())?;
        } else {
            // Generic control char: \u00XX
            let mut buf = [0u8; 6];
            buf[0] = b'\\';
            buf[1] = b'u';
            buf[2] = b'0';
            buf[3] = b'0';
            buf[4] = HEX[((b >> 4) & 0xF) as usize];
            buf[5] = HEX[(b & 0xF) as usize];
            w.write_str(core::str::from_utf8(&buf).unwrap())?;
        }

        start = i + 1;
    }

    if start < bytes.len() {
        w.write_str(core::str::from_utf8(&bytes[start..]).unwrap())?;
    }
    w.write_char('"')?;
    Ok(())
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::string::ToString;
    use alloc::vec;

    use super::*;
    use crate::ReasoningConfig;

    #[test]
    fn test_last_data() {
        let opts = PromptOpts {
            prompt: None,
            models: vec!["google/gemma-3n-e4b-it:free".to_string()],
            provider: Some("google-ai-studio".to_string()),
            system: Some("System prompt here".to_string()),
            priority: None,
            reasoning: Some(ReasoningConfig::off()),
            show_reasoning: Some(false),
            quiet: None,
            merge_config: true,
        };
        let messages = vec![
            Message::user("Hello".to_string()),
            Message::assistant("Hello there!".to_string()),
        ];
        let l = LastData { opts, messages };

        let mut got = String::with_capacity(64);
        l.to_json_writer(unsafe { got.as_mut_vec() }).unwrap();

        let expected = r#"{"opts":{"model":"google/gemma-3n-e4b-it:free","provider":"google-ai-studio","system":"System prompt here","reasoning":{"enabled":false},"show_reasoning":false,"merge_config":true},"messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hello there!"}]}"#;

        assert_eq!(got, expected);
    }

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
        };
        let messages = vec![
            Message::user("Hello".to_string()),
            Message::assistant("Hello there!".to_string()),
        ];
        let got = build_body(0, &opts, &messages).unwrap();

        let expected = r#"{"stream": true, "usage": {"include": true}, "model": "google/gemma-3n-e4b-it:free", "provider": {"order": ["google-ai-studio"]}, "reasoning": {"enabled": false}, "messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hello there!"}]}"#;

        assert_eq!(got, expected);
    }
}
