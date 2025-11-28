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
pub use common::config;
use common::multi_channel;
mod net;
pub use net::{http, tls};

pub use ort_openrouter_core::CancelToken;
pub use ort_openrouter_core::Flushable;
pub use ort_openrouter_core::Stats;
pub use ort_openrouter_core::build_body;
pub use ort_openrouter_core::{ApiKey, ConfigFile, Settings};
pub use ort_openrouter_core::{
    ChatCompletionsResponse, Choice, DEFAULT_MODEL, LastData, Message, Priority, PromptOpts,
    ReasoningConfig, ReasoningEffort, Response, Role, ThinkEvent, Usage,
};
pub use ort_openrouter_core::{
    Context, OrtError, OrtResult, ort_err, ort_error, ort_from_err, slug, tmux_pane_id,
};
