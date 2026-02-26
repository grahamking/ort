//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use crate::ort_error;
use crate::{
    Context, ErrorKind, LastData, Message, OrtResult, PromptOpts, Response, Write, common::config,
    common::file, common::queue, common::stats, common::utils,
};

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

    pub fn run<const N: usize>(
        &mut self,
        mut rx: queue::Consumer<Response, N>,
    ) -> OrtResult<stats::Stats> {
        // This will contain the entire model response. Start with a size that includes most
        // answers, but allow realloc. Maybe we should stream to disk?
        let mut contents = String::with_capacity(4096);
        while let Some(data) = rx.get_next() {
            match data {
                Response::Start => {}
                Response::Think(_) => {}
                Response::Content(content) => {
                    contents.push_str(&content);
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

        let message = Message::assistant(contents);
        self.data.messages.push(message);

        self.data.to_json_writer(&mut self.w)?;
        let _ = (&mut self.w).flush();

        Ok(stats::Stats::default()) // Stats is not used
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::string::ToString;
    use alloc::vec;

    use super::*;
    use crate::{LastData, ThinkEvent, common::queue};

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

        let q = queue::Queue::<Response, 16>::new();
        let rx = q.consumer();

        q.add(Response::Start);
        q.add(Response::Think(ThinkEvent::Start));
        q.add(Response::Think(ThinkEvent::Content(
            "thinking...".to_string(),
        )));
        q.add(Response::Think(ThinkEvent::Stop));
        q.add(Response::Content("Hello".to_string()));
        q.add(Response::Content(" world".to_string()));
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
        assert_eq!(data.messages[2].content.as_deref(), Some("Hello world"));
    }
}
