//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

pub mod input;
pub use input::cli;
pub mod output;
pub use output::writer;
pub mod common;
use common::multi_channel;
mod net;
pub use net::{http, tls};

pub use ort_openrouter_core::CancelToken;
pub use ort_openrouter_core::Flushable;
pub use ort_openrouter_core::Stats;
pub use ort_openrouter_core::build_body;
pub use ort_openrouter_core::{ApiKey, ConfigFile, Settings};
pub use ort_openrouter_core::{
    ArgParseError, Context, OrtError, OrtResult, cache_dir, ensure_dir_exists, get_env,
    load_config, ort_err, ort_error, ort_from_err, path_exists, read_to_string, slug, tmux_pane_id,
    xdg_dir,
};
pub use ort_openrouter_core::{
    ChatCompletionsResponse, Choice, Cmd, Consumer, DEFAULT_MODEL, LastData, ListOpts, Message,
    Priority, PromptOpts, Queue, ReasoningConfig, ReasoningEffort, Response, Role, ThinkEvent,
    Usage, parse_list_args, parse_prompt_args,
};
