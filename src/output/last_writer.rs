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
