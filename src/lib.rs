//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

#![no_std]
// feature(test) for benchmarking
#![cfg_attr(test, feature(test))]

mod common;
mod input;
mod net;
mod output;
pub mod syscall;

pub use common::alloc::ArenaAlloc;
pub use common::buf_read::{OrtBufReader, StringReader};
pub use common::config;
pub use common::data::{
    Annotation, ChatCompletionsResponse, Choice, Content, Function, LastData, Message, Priority,
    ReasoningEffort, Response, Role, ThinkEvent, Tool, ToolDisplay, ToolParameter, Usage,
};
pub use common::error::{Context, ErrorKind, OrtError, OrtResult, ort_err, ort_error};
pub use common::file;
pub use common::json_parser;
pub use common::stats::Stats;
pub use common::{io::Read, io::Write};
pub use common::{time, utils};
// Only for panic_handler.rs
pub use common::utils::num_to_string;

// Shorten the import path
#[allow(unused)]
pub(crate) use common::utils::{eprint_string, print_string};

pub use input::args;
pub use input::cli;
pub use input::prompt::{ActivePrompt, PromptReader};
pub use input::to_json::{build_body, write_json_str};

pub use net::socket::TcpSocket;
pub use net::tls::TlsStream;
pub use net::{chunked, http};

pub use output::OutputWriter;
pub use output::logger::Logger;
pub use output::writer::StdoutWriter;
