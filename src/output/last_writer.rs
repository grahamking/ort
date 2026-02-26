//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;

use alloc::vec::Vec;

use crate::{
    Context, ErrorKind, LastData, Message, OrtResult, PromptOpts, Response, Write, common::config,
    common::file, common::queue, common::stats, common::utils,
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
}

impl LastWriter {
    pub fn new(opts: PromptOpts, messages: Vec<Message>) -> OrtResult<Self> {
        let mut last_path = [0u8; 128];
        let idx = config::cache_dir(&mut last_path)?;
        last_path[idx] = b'/';
        let last_filename = utils::last_filename();
        let start = idx + 1;
        let end = start + last_filename.len();
        last_path[start..end].copy_from_slice(last_filename.as_bytes());
        // end + 1 to add a null byte on the end
        let last_file =
            unsafe { file::File::create(&last_path[..end + 1]).context("create last file")? };
        let data = LastData { opts, messages };
        Ok(LastWriter { data, w: last_file })
    }

    /// Received queue messages and stream response to disk.
    pub fn run<const N: usize>(
        &mut self,
        mut rx: queue::Consumer<Response, N>,
    ) -> OrtResult<stats::Stats> {
        //let mut contents = String::with_capacity(TOKEN_MEM_BUFFER + 64);
        let mut buffer = [0u8; TOKEN_MEM_BUFFER + 64];
        let mut buf_idx = 0;
        while let Some(data) = rx.get_next() {
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
                        crate::input::to_json::write_json(msg, &mut self.w)?;
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
                    let end = buf_idx + b.len();
                    buffer[buf_idx..end].copy_from_slice(b);
                    buf_idx = end;

                    if buffer.len() >= TOKEN_MEM_BUFFER {
                        crate::input::to_json::write_encoded_bytes(
                            &mut self.w,
                            &buffer[..buf_idx],
                        )?;
                        buf_idx = 0;
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
        }

        // Write final contents
        crate::input::to_json::write_encoded_bytes(&mut self.w, &buffer[..buf_idx])?;

        // close the contents message and messages array
        self.w.write_str("\"}]")?;

        self.w.write_str(",\"opts\":")?;
        self.data.opts.to_json_writer(&mut self.w)?;

        self.w.write_char('}')?; // End of whole object
        let _ = self.w.flush();

        Ok(stats::Stats::default()) // Stats is not used
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::string::ToString;
    use alloc::vec;

    use super::*;
    use crate::{LastData, ThinkEvent, common::queue, utils::num_to_string};

    #[test]
    fn test_run_success() {
        const TEST_PATH_C: &[u8] = b"/tmp/ort-last-writer-test.json\0";
        const TEST_PATH: &str = "/tmp/ort-last-writer-test.json";

        let opts = PromptOpts::default();
        let messages = vec![
            Message::system("system prompt".to_string()),
            Message::user("user prompt".to_string()),
        ];
        let file = match unsafe { file::File::create(TEST_PATH_C) } {
            Ok(file) => file,
            Err(err) => panic!("{}", err.as_string()),
        };
        let data = LastData { opts, messages };
        let mut writer = LastWriter { w: file, data };

        let q = queue::Queue::<Response, 512>::new();
        let rx = q.consumer();

        q.add(Response::Start);
        q.add(Response::Think(ThinkEvent::Start));
        q.add(Response::Think(ThinkEvent::Content(
            "thinking...".to_string(),
        )));
        q.add(Response::Think(ThinkEvent::Stop));
        for i in 1..100 {
            q.add(Response::Content("Hello".to_string()));
            q.add(Response::Content(" world ".to_string()));
            q.add(Response::Content(num_to_string(i)));
            q.add(Response::Content(". ".to_string()));
        }
        q.add(Response::Stats(stats::Stats {
            provider: "OpenRouter AI".to_string(),
            ..Default::default()
        }));
        q.close();

        let got_stats = match writer.run(rx) {
            Ok(stats) => stats,
            Err(err) => panic!("{}", err.as_string()),
        };
        assert_eq!(got_stats.provider, "");

        let json = utils::filename_read_to_string(TEST_PATH).unwrap();
        let data = LastData::from_json(&json).unwrap();

        assert_eq!(data.opts.provider.as_deref(), Some("openrouter-ai"));
        assert_eq!(data.messages.len(), 3);
        assert_eq!(data.messages[0].content.as_deref(), Some("system prompt"));
        assert_eq!(data.messages[1].content.as_deref(), Some("user prompt"));
        let Some(content) = &data.messages[2].content else {
            panic!("Assistant message is empty");
        };
        assert!(content.starts_with("Hello world 1. "));
        assert!(content.ends_with("Hello world 99. "));
    }
}
