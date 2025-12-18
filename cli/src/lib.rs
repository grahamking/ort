//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

pub mod input;
pub use input::cli;

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
    ChatCompletionsResponse, Choice, Cmd, CollectedWriter, ConsoleWriter, Consumer, DEFAULT_MODEL,
    DirFiles, File, FileWriter, Instant, LastData, LastWriter, ListOpts, Message, OrtBufReader,
    Priority, PromptOpts, Queue, Read, ReasoningConfig, ReasoningEffort, Response, Role, TcpSocket,
    ThinkEvent, TlsStream, Usage, Write, last_modified, parse_list_args, parse_prompt_args,
    resolve, thread,
};
pub use ort_openrouter_core::{chunked, http, list};
