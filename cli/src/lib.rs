//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

pub mod input;
pub use input::cli;
pub mod output;
pub use output::{from_json, writer};
pub mod common;
pub use common::cancel_token::CancelToken;
pub use common::config;
use common::multi_channel;
mod net;
pub use net::{http, tls};

pub use ort_openrouter_core::common::Flushable;
pub use ort_openrouter_core::common::data::{
    ChatCompletionsResponse, Choice, DEFAULT_MODEL, LastData, Message, Priority, PromptOpts,
    ReasoningConfig, ReasoningEffort, Response, Role, ThinkEvent, Usage,
};
pub use ort_openrouter_core::common::error::{
    Context, OrtError, OrtResult, ort_err, ort_error, ort_from_err,
};
pub use ort_openrouter_core::common::stats;
