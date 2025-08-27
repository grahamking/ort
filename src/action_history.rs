//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::time::SystemTime;
use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use ort::utils;

use crate::{action_prompt, config};

pub fn run_continue(
    api_key: &str,
    is_quiet: bool,
    next_prompt: String,
    mut opts: ort::CommonPromptOpts,
) -> anyhow::Result<()> {
    let dir = config::cache_dir()?;
    let mut last_file = dir.join(format!("last-{}.json", utils::tmux_pane_id()));
    if !last_file.exists() {
        last_file = most_recent(&dir, "last-")?;
    }
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

/// Find the most recent file in `dir` that starts with `filename_prefix`.
/// Uses the minimal amount of disk access to go as fast as possible.
fn most_recent(dir: &Path, filename_prefix: &str) -> anyhow::Result<PathBuf> {
    let mut most_recent_file: Option<(PathBuf, SystemTime)> = None;

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(filename_prefix) {
            continue;
        }
        // Only then get metadata (1 disk read)
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        let modified_time = metadata.modified()?;

        if let Some((_, prev_time)) = &most_recent_file {
            if modified_time > *prev_time {
                most_recent_file = Some((path, modified_time));
            }
        } else {
            most_recent_file = Some((path, modified_time));
        }
    }

    most_recent_file
        .map(|(path, _)| Ok(path))
        .unwrap_or_else(|| {
            Err(anyhow::anyhow!(
                "No files found starting with prefix: {filename_prefix}",
            ))
        })
}
