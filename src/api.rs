//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

//! Abstract over chat completions and responses APIs.

// #[cfg(feature = "chat_completions")]
mod chat_completions;

#[cfg(feature = "responses")]
mod responses;

pub use chat_completions::build_body;
pub(crate) use chat_completions::write_json_message;

#[cfg(feature = "chat_completions")]
pub(crate) use chat_completions::PROMPT_URL;

#[cfg(feature = "responses")]
pub(crate) use responses::PROMPT_URL;
