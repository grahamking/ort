//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

#![no_std]

mod common;
mod input;
pub mod libc;
mod net;
mod output;

pub use common::cancel_token::CancelToken;
pub use common::data::{
    ChatCompletionsResponse, Choice, DEFAULT_MODEL, LastData, Message, Priority, PromptOpts,
    ReasoningConfig, ReasoningEffort, Response, Role, ThinkEvent, Usage,
};
pub use common::error::{Context, OrtError, OrtResult, ort_err, ort_error, ort_from_err};
pub use common::thread;
pub use common::utils;
pub use common::{io::Read, io::Write};

pub use input::cli;
pub use input::list;
pub use input::prompt;
pub use input::to_json::build_body;

pub use net::socket::TcpSocket;
pub use net::tls::TlsStream;
pub use net::{chunked, http};

pub use output::writer::{CollectedWriter, ConsoleWriter, FileWriter, LastWriter};
