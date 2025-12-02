//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

#![no_std]

mod common;
mod input;
mod net;
mod output;

pub use common::Flushable;
pub use common::cancel_token::CancelToken;
pub use common::config::{ApiKey, ConfigFile, Settings, cache_dir, load_config, xdg_dir};
pub use common::data::{
    ChatCompletionsResponse, Choice, DEFAULT_MODEL, LastData, Message, Priority, PromptOpts,
    ReasoningConfig, ReasoningEffort, Response, Role, ThinkEvent, Usage,
};
pub use common::error::{Context, OrtError, OrtResult, ort_err, ort_error, ort_from_err};
pub use common::queue::{Consumer, Queue};
pub use common::stats::Stats;
pub use common::utils::{
    ensure_dir_exists, get_env, path_exists, read_to_string, slug, tmux_pane_id,
};

pub use input::args::{ArgParseError, Cmd, ListOpts, parse_list_args, parse_prompt_args};
pub use input::to_json::build_body;

pub use net::tls::{aead, ecdh, hkdf, hmac, sha2};
