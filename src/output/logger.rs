//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

use crate::{
    Context as _, OrtResult, Write as _,
    cli::Env,
    common::{config, file},
};

pub struct Logger {
    w: file::File,
}

const LOG_FILENAME: &str = "_log.jsonl";

impl Logger {
    pub fn new(env: &Env, model_slug: &str) -> OrtResult<Self> {
        let mut log_path = [0u8; 128];
        let idx = config::cache_dir(env, &mut log_path)?;
        log_path[idx] = b'/';
        let start = idx + 1;
        let end = start + model_slug.len();
        log_path[start..end].copy_from_slice(model_slug.as_bytes());
        let start = end;
        let end = start + LOG_FILENAME.len();
        log_path[start..end].copy_from_slice(LOG_FILENAME.as_bytes());
        // end + 1 to add a null byte on the end
        let log = unsafe { file::File::create(&log_path[..end + 1]).context("create log file")? };
        Ok(Logger { w: log })
    }

    pub fn log(&mut self, msg: &str) {
        let _ = self.w.write(msg.as_bytes());
        let _ = self.w.write_char('\n');
    }
}
