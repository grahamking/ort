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

use crate::{CancelToken, LastData, OrtError, OrtResult, ort_err, ort_error};
use crate::{config, utils};

use crate::action_prompt;

pub fn run_continue(
    api_key: &str,
    cancel_token: CancelToken,
    settings: config::Settings,
    mut opts: crate::PromptOpts,
) -> OrtResult<()> {
    let dir = config::cache_dir()?;
    let mut last_file = dir.join(format!("last-{}.json", utils::tmux_pane_id()));
    if !last_file.exists() {
        last_file = most_recent(&dir, "last-")?;
    }
    let mut last = match fs::read_to_string(&last_file) {
        Ok(hist_str) => LastData::from_json(&hist_str)
            .map_err(|err| ort_error(format!("Failed to parse last: {err}")))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return ort_err("No last conversation, cannot continue");
        }
        Err(e) => {
            let mut err: OrtError = e.into();
            err.context(last_file.display().to_string());
            return Err(err);
        }
    };

    opts.merge(last.opts);
    last.messages
        .push(crate::Message::user(opts.prompt.take().unwrap()));

    action_prompt::run(api_key, cancel_token, settings, opts, last.messages)
}

/// Find the most recent file in `dir` that starts with `filename_prefix`.
/// Uses the minimal amount of disk access to go as fast as possible.
fn most_recent(dir: &Path, filename_prefix: &str) -> OrtResult<PathBuf> {
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
            ort_err(format!(
                "No files found starting with prefix: {filename_prefix}"
            ))
        })
}
