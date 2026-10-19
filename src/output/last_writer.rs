//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;

use alloc::ffi::CString;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::cli::Env;
use crate::common::config::Cfg;
use crate::common::data::ToolCall;
use crate::common::json_parser::Parser;
use crate::input::to_json::{write_encoded_bytes, write_json_str};
use crate::output::OutputWriter;
use crate::{
    Context, ErrorKind, LastData, Message, OrtResult, Response, Write, common::config,
    common::file, common::utils,
};
use crate::{ThinkEvent, ort_error};

/// How many bytes of content tokens to buffer before streaming to disk.
/// Large chunks bypass this buffer after pending bytes have been flushed.
const TOKEN_MEM_BUFFER: usize = 4096;

/// LastWriter saves to disk the model response and enough information so that we can
/// continue the conversation with `ort -c "next prompt"` later.
pub struct LastWriter {
    w: file::File,
    env: Env,
    cfg: Cfg,
    data: LastData,
    buffer: [u8; TOKEN_MEM_BUFFER],
    buf_idx: usize,
    reasoning: String,
    details: Vec<Vec<(String, String)>>,
    tool_calls: Vec<ToolCall>,
    started: bool,
    stopped: bool,
    temp_path: CString,
    final_path: CString,
}

fn last_path(env: &Env, ext: &str) -> OrtResult<([u8; 128], usize)> {
    let mut last_path = [0u8; 128];
    // Leave space for /ort, the filename and its terminating NUL.
    let base_len = env
        .XDG_CACHE_HOME
        .filter(|s| !s.is_empty())
        .map_or_else(|| env.HOME.map_or(0, |s| s.len() + 7), str::len);
    let filename_len = utils::last_filename(env, ext).len();
    if base_len + 4 + 1 + filename_len >= last_path.len() {
        return Err(ort_error(ErrorKind::LastWriterError, "Cache path too long"));
    }
    let idx = config::cache_dir(env, &mut last_path)?;
    last_path[idx] = b'/';
    let last_filename = utils::last_filename(env, ext);
    let start = idx + 1;
    let end = start + last_filename.len();
    last_path[start..end].copy_from_slice(last_filename.as_bytes());

    Ok((last_path, end))
}

/// The full path of the file where we stored the last conversation
pub(crate) fn last_data_file(env: &Env) -> OrtResult<String> {
    let (last_path, end) = last_path(env, ".json")?;

    let cs = CString::new(&last_path[..end]).expect("Null bytes in config cache dir");
    if utils::path_exists(cs.as_ref()) {
        Ok(unsafe { String::from_utf8_unchecked(last_path[..end].into()) })
    } else {
        let mut last_path = [0u8; 128];
        let cache_dir_end = config::cache_dir(env, &mut last_path)?;
        let cache_dir = unsafe { str::from_utf8_unchecked(&last_path[..cache_dir_end]) };
        utils::most_recent(cache_dir, "last-", ".json").context("most_recent")
    }
}

// The config we used last time
pub(crate) fn last_cfg(env: &Env) -> OrtResult<Cfg> {
    let data_path = last_data_file(env)?;
    let prev_config = data_path
        .strip_suffix(".json")
        .ok_or_else(|| ort_error(ErrorKind::ConfigReadFailed, "Invalid history path"))?
        .to_string()
        + ".cfg";
    let Ok(cfg_str) = utils::filename_read_to_string(&prev_config) else {
        return Err(ort_error(ErrorKind::ConfigReadFailed, ""));
    };
    let prev_cfg = config::Cfg::from_str(&cfg_str)?;
    Ok(prev_cfg)
}

impl LastWriter {
    pub fn new(messages: Vec<Message>, env: &Env, cfg: &Cfg) -> OrtResult<Self> {
        let (lp, end) = last_path(env, ".json")?;
        let final_path = CString::new(&lp[..end]).unwrap();
        let temp_path = CString::new(
            final_path.to_str().unwrap().to_string() + "." + &utils::generate_session_id() + ".tmp",
        )
        .unwrap();
        let last_file = unsafe {
            file::File::create(temp_path.as_bytes_with_nul())
                .context("create temporary last file")?
        };
        let data = LastData { messages };
        Ok(LastWriter {
            data,
            env: env.clone(),
            cfg: cfg.clone(),
            w: last_file,
            buffer: [0u8; TOKEN_MEM_BUFFER],
            buf_idx: 0,
            reasoning: String::new(),
            details: Vec::new(),
            tool_calls: Vec::new(),
            started: false,
            stopped: false,
            temp_path,
            final_path,
        })
    }
}

impl LastWriter {
    fn flush_content(&mut self) -> OrtResult<()> {
        write_encoded_bytes(&mut self.w, &self.buffer[..self.buf_idx])?;
        self.buf_idx = 0;
        Ok(())
    }

    fn content(&mut self, bytes: &[u8]) -> OrtResult<()> {
        if self.buf_idx + bytes.len() > TOKEN_MEM_BUFFER {
            self.flush_content()?;
        }
        if bytes.len() >= TOKEN_MEM_BUFFER {
            write_encoded_bytes(&mut self.w, bytes)?;
        } else {
            let end = self.buf_idx + bytes.len();
            self.buffer[self.buf_idx..end].copy_from_slice(bytes);
            self.buf_idx = end;
        }
        Ok(())
    }

    fn add_details(&mut self, json: &str) -> OrtResult<()> {
        let mut p = Parser::new(json);
        p.skip_ws();
        p.expect(b'[')?;
        p.skip_ws();
        while !p.try_consume(b']') {
            let mut fields = Vec::new();
            p.expect(b'{')?;
            p.skip_ws();
            while !p.try_consume(b'}') {
                let key = p.parse_string()?;
                p.skip_ws();
                p.expect(b':')?;
                p.skip_ws();
                fields.push((key, p.value_slice()?.to_string()));
                p.skip_ws();
                if !p.try_consume(b',') {
                    p.expect(b'}')?;
                    break;
                }
                p.skip_ws();
            }
            // Merge consecutive text/summary deltas only. Encrypted blocks are opaque,
            // discrete objects, even when providers reuse the same streaming index.
            let kind = field(&fields, "type");
            let payload = match kind {
                Some("\"reasoning.text\"") => Some("text"),
                Some("\"reasoning.summary\"") => Some("summary"),
                _ => None,
            };
            let merge = payload.is_some()
                && self.details.last().is_some_and(|prev| {
                    field(prev, "type") == kind
                        && ["id", "index", "format"].iter().all(|key| {
                            match (field(prev, key), field(&fields, key)) {
                                (Some(a), Some(b)) if a != "null" && b != "null" => a == b,
                                _ => true,
                            }
                        })
                });
            if merge {
                let prev = self.details.last_mut().unwrap();
                for (key, value) in fields {
                    if value == "null" {
                        continue;
                    }
                    if let Some((_, old)) = prev.iter_mut().find(|(k, _)| *k == key) {
                        if key == payload.unwrap() || key == "signature" {
                            if old != "null" {
                                // Both values are JSON strings. Join their encoded interiors
                                // without repeatedly decoding/copying the whole accumulated block.
                                if !(old.starts_with('"')
                                    && old.ends_with('"')
                                    && value.starts_with('"')
                                    && value.ends_with('"'))
                                {
                                    return Err(ort_error(
                                        ErrorKind::LastWriterError,
                                        "Invalid reasoning string",
                                    ));
                                }
                                old.pop();
                                old.push_str(&value[1..]);
                            } else {
                                *old = value;
                            }
                        } else {
                            *old = value;
                        }
                    } else {
                        prev.push((key, value));
                    }
                }
            } else {
                self.details.push(fields);
            }
            p.skip_ws();
            if !p.try_consume(b',') {
                p.expect(b']')?;
                break;
            }
            p.skip_ws();
        }
        p.skip_ws();
        if p.peek().is_some() {
            return Err(ort_error(
                ErrorKind::LastWriterError,
                "Invalid reasoning details",
            ));
        }
        Ok(())
    }
}

fn field<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

impl OutputWriter for LastWriter {
    fn write(&mut self, data: Response) -> OrtResult<()> {
        if self.stopped {
            return Ok(());
        }
        match data {
            Response::Start => {
                if self.started {
                    return Ok(());
                }
                self.started = true;
                self.w.write_str("{\"messages\":[")?;
                for (i, msg) in self.data.messages.iter().enumerate() {
                    if i != 0 {
                        self.w.write_char(',')?;
                    }
                    crate::input::to_json::write_json_message(msg, &mut self.w)?;
                }
                if !self.data.messages.is_empty() {
                    self.w.write_char(',')?;
                }
                self.w.write_str("{\"role\":\"assistant\",\"content\":\"")?;
            }
            Response::Think(ThinkEvent::Content(text)) => self.reasoning.push_str(&text),
            Response::Think(ThinkEvent::Details(json)) => self.add_details(&json)?,
            Response::Content(text) => self.content(text.as_bytes())?,
            Response::ToolCalls(calls) => self.tool_calls.extend(calls),
            Response::Missing => {
                let mut buf = [0u8; 4];
                self.content(super::MISSING_CHAR.encode_utf8(&mut buf).as_bytes())?;
            }
            Response::Stats(stats) => {
                if !stats.provider().is_empty() {
                    self.cfg.provider = Some(utils::slug(stats.provider()));
                }
            }
            Response::Error(_) => {
                return Err(ort_error(
                    ErrorKind::LastWriterError,
                    "LastWriter run error",
                ));
            }
            _ => {}
        }
        Ok(())
    }

    fn stop(&mut self, _include_stats: bool) -> OrtResult<()> {
        if self.stopped || !self.started {
            return Ok(());
        }
        self.flush_content()?;
        self.w.write_char('"')?;
        if !self.reasoning.is_empty() {
            self.w.write_str(",\"reasoning\":")?;
            write_json_str(&mut self.w, &self.reasoning)?;
        }
        if !self.details.is_empty() {
            self.w.write_str(",\"reasoning_details\":[")?;
            for (i, fields) in self.details.iter().enumerate() {
                if i != 0 {
                    self.w.write_char(',')?;
                }
                self.w.write_char('{')?;
                for (j, (key, value)) in fields.iter().enumerate() {
                    if j != 0 {
                        self.w.write_char(',')?;
                    }
                    write_json_str(&mut self.w, key)?;
                    self.w.write_char(':')?;
                    self.w.write_all(value.as_bytes())?;
                }
                self.w.write_char('}')?;
            }
            self.w.write_char(']')?;
        }
        if !self.tool_calls.is_empty() {
            self.w.write_str(",\"tool_calls\":[")?;
            for (i, call) in self.tool_calls.iter().enumerate() {
                if i != 0 {
                    self.w.write_char(',')?;
                }
                call.write_json(&mut self.w)?;
            }
            self.w.write_char(']')?;
        }
        self.w.write_str("}]}")?;
        self.w.flush()?;
        let (path, len) = last_path(&self.env, ".cfg")?;
        self.cfg.save(path, len)?;
        if crate::syscall::rename(&self.temp_path, &self.final_path) < 0 {
            return Err(ort_error(
                ErrorKind::LastWriterError,
                "Cannot replace last conversation",
            ));
        }
        self.stopped = true;
        Ok(())
    }
}

impl Drop for LastWriter {
    fn drop(&mut self) {
        use crate::net::AsFd;
        crate::syscall::close(self.w.as_fd());
        crate::syscall::unlink(&self.temp_path);
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::{Function, Role, build_body, time};
    use alloc::{boxed::Box, format, vec};
    use std::fs;

    fn test_cfg() -> Cfg {
        let mut cfg = Cfg::default();
        cfg.models.push("test/model".into());
        cfg
    }

    struct Fixture {
        env: Env,
        root: String,
    }
    impl Fixture {
        fn new() -> Self {
            let root = format!("/tmp/ort-last-{}", utils::generate_session_id());
            fs::create_dir(&root).unwrap();
            let env = Env {
                XDG_CACHE_HOME: Some(Box::leak(root.clone().into_boxed_str())),
                TMUX_PANE: Some("%12345"),
                ..Default::default()
            };
            Self { env, root }
        }
        fn writer(&self, messages: Vec<Message>) -> LastWriter {
            LastWriter::new(messages, &self.env, &Cfg::default()).unwrap()
        }
        fn read(&self) -> LastData {
            let path = last_data_file(&self.env).unwrap();
            LastData::from_json(&fs::read_to_string(path).unwrap()).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    #[test]
    fn reasoning_round_trip_into_continue_request() {
        let f = Fixture::new();
        let mut w = f.writer(vec![Message::user("question".into())]);
        w.write(Response::Start).unwrap();
        for text in ["\nthink ", "\"carefully\"\\\t🦀"] {
            w.write(Response::Think(ThinkEvent::Content(text.into())))
                .unwrap();
        }
        for details in [
            r#"[{"type":"reasoning.text","text":"one ","signature":null,"index":0,"extra":{"keep":true}}]"#,
            r#"[{"type":"reasoning.text","text":"two","signature":"signed","index":0}]"#,
            r#"[{"type":"reasoning.summary","summary":"sum ","index":0}]"#,
            r#"[{"type":"reasoning.summary","summary":"mary","index":0}]"#,
            r#"[{"type":"reasoning.encrypted","data":"opaque1","index":0,"id":"a"},{"type":"reasoning.encrypted","data":"opaque2","index":0,"id":"b"}]"#,
            "[]",
        ] {
            w.write(Response::Think(ThinkEvent::Details(details.into())))
                .unwrap();
        }
        w.write(Response::Content("answer".into())).unwrap();
        w.stop(false).unwrap();
        w.stop(false).unwrap(); // Repeated finalization must not append JSON.
        let mut last = f.read();
        let answer = last.messages[1].clone();
        assert_eq!(
            answer.reasoning.as_deref(),
            Some("\nthink \"carefully\"\\\t🦀")
        );
        let details = answer.reasoning_details.as_deref().unwrap();
        assert!(details.contains(r#""text":"one two""#));
        assert!(details.contains(r#""summary":"sum mary""#));
        assert!(details.contains(r#""signature":"signed""#));
        assert!(details.contains(r#""extra":{"keep":true}"#));
        assert_eq!(details.matches("reasoning.encrypted").count(), 2);
        last.messages.push(Message::user("next".into()));
        let body = build_body(0, &test_cfg(), &last.messages, &[]).unwrap();
        let sent = LastData::from_json(&body).unwrap();
        assert_eq!(sent.messages[1].reasoning, answer.reasoning);
        assert_eq!(sent.messages[1].reasoning_details, answer.reasoning_details);
        assert_eq!(sent.messages[2].text(), Some("next"));
        // A second save retains the reasoning from the earlier turn too.
        let mut next = f.writer(sent.messages);
        next.write(Response::Start).unwrap();
        next.write(Response::Content("second answer".into()))
            .unwrap();
        next.stop(false).unwrap();
        assert_eq!(
            f.read().messages[1].reasoning_details,
            answer.reasoning_details
        );
    }

    #[test]
    fn large_chunks_tools_and_missing_keep_valid_json_and_order() {
        let f = Fixture::new();
        let mut w = f.writer(vec![]);
        w.write(Response::Start).unwrap();
        let large = "🦀\n\"\\".repeat(3000);
        w.write(Response::Content("before".into())).unwrap();
        w.write(Response::Missing).unwrap();
        w.write(Response::Content(large.clone())).unwrap();
        w.write(Response::ToolCalls(vec![ToolCall {
            id: Some("call_1".into()),
            function: Function {
                name: "read".into(),
                arguments: r#"{"path":"a"}"#.into(),
            },
            ..Default::default()
        }]))
        .unwrap();
        w.write(Response::Content("after".into())).unwrap();
        w.stop(false).unwrap();
        let last = f.read();
        assert_eq!(last.messages.len(), 1);
        let expected = format!("before{}{large}after", super::super::MISSING_CHAR);
        assert_eq!(last.messages[0].text(), Some(expected.as_str()));
        assert_eq!(last.messages[0].tool_calls.len(), 1);
        assert_eq!(
            last.messages[0].tool_calls[0].function.arguments,
            r#"{"path":"a"}"#
        );
    }

    #[test]
    fn abandoned_response_preserves_previous_chat_and_cleans_temp() {
        let f = Fixture::new();
        let mut first = f.writer(vec![]);
        first.write(Response::Start).unwrap();
        first.write(Response::Content("saved".into())).unwrap();
        first.stop(false).unwrap();
        let temp;
        {
            let mut interrupted = f.writer(vec![]);
            temp = interrupted.temp_path.to_str().unwrap().to_string();
            interrupted.write(Response::Start).unwrap();
            interrupted
                .write(Response::Content("partial".into()))
                .unwrap();
            assert_eq!(f.read().messages[0].text(), Some("saved"));
        }
        assert!(!std::path::Path::new(&temp).exists());
        assert_eq!(f.read().messages[0].text(), Some("saved"));
        let mut never_started = f.writer(vec![]);
        never_started.stop(false).unwrap();
        assert_eq!(f.read().messages[0].text(), Some("saved"));
    }

    #[test]
    fn fallback_uses_json_and_matching_config() {
        let f = Fixture::new();
        let mut first = f.writer(vec![]);
        first.cfg.provider = Some("chosen-provider".into());
        first.write(Response::Start).unwrap();
        first.write(Response::Content("saved".into())).unwrap();
        first.stop(false).unwrap();
        // A newer config and an abandoned temporary file are not conversations.
        fs::write(format!("{}/ort/last-999.cfg", f.root), "model: unrelated").unwrap();
        fs::write(
            format!("{}/ort/last-999.json.abc.tmp", f.root),
            "incomplete",
        )
        .unwrap();
        let env = Env {
            TMUX_PANE: Some("%7"),
            ..f.env.clone()
        };
        assert!(last_data_file(&env).unwrap().ends_with("last-12345.json"));
        assert_eq!(
            last_cfg(&env).unwrap().provider.as_deref(),
            Some("chosen-provider")
        );
    }

    #[test]
    fn details_only_and_empty_messages_serialize_without_array_holes() {
        let messages = vec![
            Message::new(Role::Assistant, None, None),
            Message::with_content(
                Role::Assistant,
                vec![],
                None,
                Some(r#"[{"type":"reasoning.encrypted","data":"x"}]"#.into()),
                vec![],
                None,
                vec![],
            ),
            Message::user("next".into()),
        ];
        let body = build_body(0, &test_cfg(), &messages, &[]).unwrap();
        let parsed = LastData::from_json(&body).unwrap();
        assert_eq!(parsed.messages.len(), 3);
        assert_eq!(
            parsed.messages[1].reasoning_details,
            messages[1].reasoning_details
        );
    }

    #[test]
    fn long_cache_path_returns_error_instead_of_panicking() {
        let env = Env {
            XDG_CACHE_HOME: Some(Box::leak("x".repeat(200).into_boxed_str())),
            ..Default::default()
        };
        assert!(last_path(&env, ".json").is_err());
    }

    #[test]
    fn streaming_whitespace_does_not_drop_reasoning_details_or_content() {
        let f = Fixture::new();
        let mut w = f.writer(vec![Message::user("question".into())]);
        let mut prompt =
            crate::ActivePrompt::new("test".into(), &test_cfg(), vec![], vec![], 0, None);
        prompt.start = Some(time::Ticks::now());

        let stream = concat!(
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"reasoning\":\" \",\"reasoning_details\":[{\"type\":\"reasoning.encrypted\",\"data\":\"opaque\"}],\"content\":\"answer\"},\"finish_reason\":\"stop\"}]}\n",
            "data: [DONE]\n"
        );
        prompt.reader = Some(Box::new(crate::OrtBufReader::new(crate::StringReader {
            data: stream.into(),
            pos: 0,
        })));
        while let Some(events) = prompt.next().unwrap() {
            for event in events {
                w.write(event).unwrap();
            }
        }
        assert_eq!(prompt.finish_reason(), Some("stop"));
        w.stop(false).unwrap();
        let last = f.read();
        assert_eq!(last.messages[1].text(), Some("answer"));
        assert_eq!(last.messages[1].reasoning.as_deref(), Some(" "));
        assert!(
            last.messages[1]
                .reasoning_details
                .as_deref()
                .unwrap()
                .contains("opaque")
        );
    }

    #[test]
    fn distinct_reasoning_blocks_are_not_merged() {
        let f = Fixture::new();
        let mut w = f.writer(vec![]);
        w.add_details(r#"[{"type":"reasoning.text","id":"a","text":"one"},{"type":"reasoning.text","id":"b","text":"two"}]"#).unwrap();
        assert_eq!(w.details.len(), 2);
    }
}
