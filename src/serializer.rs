//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::io::{self, Cursor, Write};

use crate::{LastData, Message, PromptOpts};

/// Build the POST body
/// The system and user prompts must already by in messages.
pub fn build_body(opts: &PromptOpts, messages: &[Message]) -> anyhow::Result<String> {
    let capacity: u32 = messages.iter().map(|m| m.size()).sum::<u32>() + 100;
    let mut w = Cursor::new(Vec::with_capacity(capacity as usize));

    w.write_all(b"{\"stream\": true, \"usage\": {\"include\": true}, \"model\": ")?;
    write_json_str(&mut w, opts.model.as_ref().expect("Missing model"))?;

    if opts.priority.is_some() || opts.provider.is_some() {
        w.write_all(b", \"provider\": {")?;
        let mut is_first = true;
        if let Some(p) = opts.priority {
            w.write_all(b"\"sort\":")?;
            write_json_str_simple(&mut w, p.as_str())?;
            is_first = false;
        }
        if let Some(pr) = &opts.provider {
            if !is_first {
                w.write_all(b", ")?;
            }
            w.write_all(b"\"order\": [")?;
            write_json_str(&mut w, pr)?;
            w.write_all(b"]")?;
        }
        w.write_all(b"}")?;
    }

    w.write_all(b", \"reasoning\": ")?;
    match &opts.reasoning {
        // No -r and nothing in config file
        None => w.write_all(b"{\"enabled\": false}")?,
        // cli "-r off" or config file '"enabled": false'
        Some(r_cfg) if !r_cfg.enabled => w.write_all(b"{\"enabled\": false}")?,
        // Reasoning on
        Some(r_cfg) => match (r_cfg.effort, r_cfg.tokens) {
            (Some(effort), _) => {
                w.write_all(b"{\"exclude\": false, \"enabled\": true, \"effort\":")?;
                write_json_str_simple(&mut w, effort.as_str())?;
                w.write_all(b"}")?;
            }
            (_, Some(tokens)) => {
                w.write_all(b"{\"exclude\": false, \"enabled\": true, \"max_tokens\":")?;
                write_u32(&mut w, tokens)?;
                w.write_all(b"}")?;
            }
            _ => unreachable!("Reasoning effort and tokens cannot both be null"),
        },
    };

    w.write_all(b", \"messages\":")?;
    Message::write_json_array(messages, &mut w)?;

    w.write_all(b"}")?;

    let messages_buf = w.into_inner();
    Ok(String::from_utf8_lossy(&messages_buf).to_string())
}

impl LastData {
    pub fn to_json_writer<W: io::Write>(&self, writer: W) -> io::Result<()> {
        // Use a buffered writer for fewer syscalls when writing to files.
        let mut w = io::BufWriter::with_capacity(4096, writer);

        w.write_all(b"{\"opts\":{")?;
        let mut first = true;

        if let Some(ref v) = self.opts.prompt {
            if !first {
                w.write_all(b",")?;
            } else {
                first = false;
            }
            w.write_all(b"\"prompt\":")?;
            write_json_str(&mut w, v)?;
        }
        if let Some(ref v) = self.opts.model {
            if !first {
                w.write_all(b",")?;
            } else {
                first = false;
            }
            w.write_all(b"\"model\":")?;
            write_json_str(&mut w, v)?;
        }
        if let Some(ref v) = self.opts.provider {
            if !first {
                w.write_all(b",")?;
            } else {
                first = false;
            }
            w.write_all(b"\"provider\":")?;
            write_json_str(&mut w, v)?;
        }
        if let Some(ref v) = self.opts.system {
            if !first {
                w.write_all(b",")?;
            } else {
                first = false;
            }
            w.write_all(b"\"system\":")?;
            write_json_str(&mut w, v)?;
        }
        if let Some(ref p) = self.opts.priority {
            if !first {
                w.write_all(b",")?;
            } else {
                first = false;
            }
            w.write_all(b"\"priority\":")?;
            write_json_str_simple(&mut w, p.as_str())?;
        }
        if let Some(ref rc) = self.opts.reasoning {
            if !first {
                w.write_all(b",")?;
            } else {
                first = false;
            }
            w.write_all(b"\"reasoning\":{")?;
            // always include enabled
            w.write_all(b"\"enabled\":")?;
            write_bool(&mut w, rc.enabled)?;
            if let Some(ref eff) = rc.effort {
                w.write_all(b",\"effort\":")?;
                write_json_str_simple(&mut w, eff.as_str())?;
            }
            if let Some(tokens) = rc.tokens {
                w.write_all(b",\"tokens\":")?;
                write_u32(&mut w, tokens)?;
            }
            w.write_all(b"}")?;
        }
        if let Some(show) = self.opts.show_reasoning {
            if !first {
                w.write_all(b",")?;
            } else {
                first = false;
            }
            w.write_all(b"\"show_reasoning\":")?;
            write_bool(&mut w, show)?;
        }
        if let Some(quiet) = self.opts.quiet {
            if !first {
                w.write_all(b",")?;
            } else {
                //first = false;
            }
            w.write_all(b"\"quiet\":")?;
            write_bool(&mut w, quiet)?;
        }

        // merge_config
        w.write_all(b",")?;
        w.write_all(b"\"merge_config\":")?;
        write_bool(&mut w, self.opts.merge_config)?;

        w.write_all(b"},\"messages\":")?;
        Message::write_json_array(&self.messages, &mut w)?;

        w.write_all(b"}")?;
        w.flush()
    }
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn write_bool<W: io::Write>(w: &mut W, v: bool) -> io::Result<()> {
    if v {
        w.write_all(b"true")
    } else {
        w.write_all(b"false")
    }
}

fn write_u32<W: io::Write>(w: &mut W, mut n: u32) -> io::Result<()> {
    if n == 0 {
        return w.write_all(b"0");
    }
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    w.write_all(&buf[i..])
}

impl Message {
    pub fn write_json_array<W: io::Write>(msgs: &[Message], w: &mut W) -> io::Result<()> {
        w.write_all(b"[")?;
        for (i, msg) in msgs.iter().enumerate() {
            if i != 0 {
                w.write_all(b",")?;
            }
            msg.write_json(w)?;
        }
        w.write_all(b"]")?;
        Ok(())
    }

    pub fn write_json<W: io::Write>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(b"{\"role\":")?;
        write_json_str_simple(w, self.role.as_str())?;
        w.write_all(b",\"content\":")?;
        write_json_str(w, &self.content)?;
        w.write_all(b"}")?;
        Ok(())
    }
}

/// No escapes or special characters, just write the bytes
fn write_json_str_simple<W: io::Write>(w: &mut W, s: &str) -> io::Result<()> {
    w.write_all(b"\"")?;
    w.write_all(s.as_bytes())?;
    w.write_all(b"\"")?;
    Ok(())
}

// Writes a JSON string (with surrounding quotes) with proper escaping, no allocations.
fn write_json_str<W: io::Write>(w: &mut W, s: &str) -> io::Result<()> {
    w.write_all(b"\"")?;
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
            w.write_all(&bytes[start..i])?;
        }

        if let Some(e) = esc {
            w.write_all(e)?;
        } else {
            // Generic control char: \u00XX
            let mut buf = [0u8; 6];
            buf[0] = b'\\';
            buf[1] = b'u';
            buf[2] = b'0';
            buf[3] = b'0';
            buf[4] = HEX[((b >> 4) & 0xF) as usize];
            buf[5] = HEX[(b & 0xF) as usize];
            w.write_all(&buf)?;
        }

        start = i + 1;
    }

    if start < bytes.len() {
        w.write_all(&bytes[start..])?;
    }
    w.write_all(b"\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{LastData, Message, PromptOpts, ReasoningConfig};
    use std::io::Cursor;

    #[test]
    fn test_last_data() {
        let opts = PromptOpts {
            prompt: None,
            model: Some("google/gemma-3n-e4b-it:free".to_string()),
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

        let mut c = Cursor::new(Vec::with_capacity(64));
        l.to_json_writer(&mut c).unwrap();

        let buf = c.into_inner();
        let got = String::from_utf8_lossy(&buf);

        let expected = r#"{"opts":{"model":"google/gemma-3n-e4b-it:free","provider":"google-ai-studio","system":"System prompt here","reasoning":{"enabled":false},"show_reasoning":false,"merge_config":true},"messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hello there!"}]}"#;

        assert_eq!(got, expected);
    }

    #[test]
    fn test_build_body() {
        let opts = PromptOpts {
            prompt: None,
            model: Some("google/gemma-3n-e4b-it:free".to_string()),
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
        let got = build_body(&opts, &messages).unwrap();

        let expected = r#"{"stream": true, "usage": {"include": true}, "model": "google/gemma-3n-e4b-it:free", "provider": {"order": ["google-ai-studio"]}, "reasoning": {"enabled": false}, "messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hello there!"}]}"#;

        assert_eq!(got, expected);
    }
}
