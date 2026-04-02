//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;

use alloc::vec::Vec;

use crate::cli::Env;
use crate::output::writer::OutputWriter;
use crate::{
    Context, ErrorKind, LastData, Message, OrtResult, PromptOpts, Response, Write, common::config,
    common::file, common::utils,
};
use crate::{Role, ort_error};

/// How many bytes of content tokens to buffer before streaming to disk.
/// This limits max memory.
const TOKEN_MEM_BUFFER: usize = 1024;

/// LastWriter saves to disk the model response and enough information so that we can
/// continue the conversation with `ort -c "next prompt"` later.
pub struct LastWriter {
    w: file::File,
    data: LastData,
    buffer: [u8; TOKEN_MEM_BUFFER + 64],
    buf_idx: usize,
}

impl LastWriter {
    pub fn new(opts: PromptOpts, messages: Vec<Message>, env: &Env) -> OrtResult<Self> {
        let mut last_path = [0u8; 128];
        let idx = config::cache_dir(env, &mut last_path)?;
        last_path[idx] = b'/';
        let last_filename = utils::last_filename(env);
        let start = idx + 1;
        let end = start + last_filename.len();
        last_path[start..end].copy_from_slice(last_filename.as_bytes());
        // end + 1 to add a null byte on the end
        let last_file =
            unsafe { file::File::create(&last_path[..end + 1]).context("create last file")? };
        let data = LastData { opts, messages };
        Ok(LastWriter {
            data,
            w: last_file,
            buffer: [0u8; TOKEN_MEM_BUFFER + 64],
            buf_idx: 0,
        })
    }
}

impl OutputWriter for LastWriter {
    /// Received messages and stream response to disk.
    fn write(&mut self, data: Response) -> OrtResult<()> {
        match data {
            Response::Start => {
                // Includes opening '{' for whole object
                self.w.write_str("{\"messages\":")?;

                // Write the initial messages (system, user)
                self.w.write_char('[')?;
                for (i, msg) in self.data.messages.iter().enumerate() {
                    if i != 0 {
                        self.w.write_char(',')?;
                    }
                    crate::input::to_json::write_json_message(msg, &mut self.w)?;
                }

                // Setup streaming for the response message
                self.w.write_char(',')?;
                self.w.write_str("{\"role\":")?;
                crate::input::to_json::write_json_str_simple(
                    &mut self.w,
                    Role::Assistant.as_str(),
                )?;
                self.w.write_str(",\"content\":\"")?;
            }
            Response::Think(_) => {}
            Response::Content(content) => {
                let b = content.as_bytes();
                let end = self.buf_idx + b.len();
                self.buffer[self.buf_idx..end].copy_from_slice(b);
                self.buf_idx = end;

                if self.buffer.len() >= TOKEN_MEM_BUFFER {
                    crate::input::to_json::write_encoded_bytes(
                        &mut self.w,
                        &self.buffer[..self.buf_idx],
                    )?;
                    self.buf_idx = 0;
                }
            }
            Response::Stats(stats) => {
                self.data.opts.provider = Some(utils::slug(stats.provider()));
            }
            Response::Error(_err) => {
                return Err(ort_error(
                    ErrorKind::LastWriterError,
                    "LastWriter run error",
                ));
            }
            Response::None => {
                return Err(ort_error(
                    ErrorKind::QueueDesync,
                    "Response::None means we read the wrong Queue position",
                ));
            }
        }
        Ok(())
    }

    fn stop(&mut self, _include_stats: bool) -> OrtResult<()> {
        // Write final contents
        crate::input::to_json::write_encoded_bytes(&mut self.w, &self.buffer[..self.buf_idx])?;

        // close the contents message and messages array
        self.w.write_str("\"}]")?;

        self.w.write_str(",\"opts\":")?;
        self.data.opts.to_json_writer(&mut self.w)?;

        self.w.write_char('}')?; // End of whole object
        let _ = self.w.flush();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::string::ToString;
    use alloc::vec;

    use super::*;
    use crate::{LastData, ThinkEvent, common::stats, utils::num_to_string};

    #[test]
    fn test_run_success() {
        const TEST_PATH_C: &[u8] = b"/tmp/ort-last-writer-test.json\0";
        const TEST_PATH: &str = "/tmp/ort-last-writer-test.json";

        let opts = PromptOpts::default();
        let messages = vec![
            Message::system("system prompt".to_string()),
            Message::user("user prompt".to_string(), vec![]),
        ];
        let file = match unsafe { file::File::create(TEST_PATH_C) } {
            Ok(file) => file,
            Err(err) => panic!("{}", err.as_string()),
        };
        let data = LastData { opts, messages };
        let mut writer = LastWriter {
            w: file,
            data,
            buffer: [0u8; TOKEN_MEM_BUFFER + 64],
            buf_idx: 0,
        };

        let mut q = vec![
            Response::Start,
            Response::Think(ThinkEvent::Start),
            Response::Think(ThinkEvent::Content("thinking...".to_string())),
            Response::Think(ThinkEvent::Stop),
        ];
        for i in 1..100 {
            q.push(Response::Content("Hello".to_string()));
            q.push(Response::Content(" world ".to_string()));
            q.push(Response::Content(num_to_string(i)));
            q.push(Response::Content(". ".to_string()));
        }
        q.push(Response::Stats(stats::Stats {
            provider: "OpenRouter AI".to_string(),
            ..Default::default()
        }));

        for event in q {
            writer
                .write(event)
                .map_err(|err| panic!("LastWriter::write failed: {}", err.as_string()))
                .unwrap();
        }
        writer
            .stop(true)
            .map_err(|err| panic!("LastWriter::stop failed: {}", err.as_string()))
            .unwrap();

        let json = utils::filename_read_to_string(TEST_PATH).unwrap();
        let data = LastData::from_json(&json).unwrap();

        assert_eq!(data.opts.provider.as_deref(), Some("openrouter-ai"));
        assert_eq!(data.messages.len(), 3);
        assert_eq!(data.messages[0].content[0].content(), "system prompt");
        assert_eq!(data.messages[1].content[0].content(), "user prompt");
        let content = data.messages[2].content[0].content();
        if content.is_empty() {
            panic!("Assistant message is empty");
        };
        assert!(content.starts_with("Hello world 1. "));
        assert!(content.ends_with("Hello world 99. "));
    }
}
