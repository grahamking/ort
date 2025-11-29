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
pub use common::config::{ApiKey, ConfigFile, Settings, xdg_dir};
pub use common::data::{
    ChatCompletionsResponse, Choice, DEFAULT_MODEL, LastData, Message, Priority, PromptOpts,
    ReasoningConfig, ReasoningEffort, Response, Role, ThinkEvent, Usage,
};
pub use common::error::{Context, OrtError, OrtResult, ort_err, ort_error, ort_from_err};
pub use common::stats::Stats;
pub use common::utils::{get_env, path_exists, slug, tmux_pane_id};

pub use input::to_json::build_body;

pub use net::tls::{aead, ecdh, hkdf, hmac, sha2};
