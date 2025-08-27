//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::fs;

use anyhow::Context as _;

use crate::{action_prompt, config};

pub fn run_continue(
    api_key: &str,
    is_quiet: bool,
    next_prompt: String,
    mut opts: ort::CommonPromptOpts,
) -> anyhow::Result<()> {
    let last_file = config::cache_dir()?.join("last.json");
    let mut last: crate::writer::LastData = match fs::read_to_string(&last_file) {
        Ok(cfg_str) => serde_json::from_str(&cfg_str).context("Failed to parse last")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("No last conversation, cannot continue")
        }
        Err(e) => {
            return Err(e).context(last_file.display().to_string());
        }
    };

    opts.merge(last.opts);
    last.messages.push(ort::Message::user(next_prompt));

    let save_to_file = true; // can't continue without this
    action_prompt::run(api_key, save_to_file, is_quiet, opts, last.messages)
}
