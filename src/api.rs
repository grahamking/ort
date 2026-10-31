//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

pub mod chat_completions;

/// Where we POST a completions request to. The part after `/v1/`.
pub(crate) const PROMPT_URL: &str = "/chat/completions";
