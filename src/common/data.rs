//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

#![allow(dead_code)]

use core::str::FromStr;

extern crate alloc;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::common::base64;
use crate::utils::filename_read_to_bytes;
use crate::{ErrorKind, OrtError, OrtResult, ort_error};

const DEFAULT_SHOW_REASONING: bool = false;
const DEFAULT_QUIET: bool = false;
const IMAGE_EXT: [&str; 4] = ["jpg", "JPG", "png", "PNG"];

// Keep in sync with src/lib.rs
pub const DEFAULT_MODEL: &str = "google/gemma-3n-e4b-it:free";

const MIME_TYPES: [(&str, &str); 2] = [("jpg", "image/jpeg"), ("png", "image/png")];

// {
//  "id":"gen-1756743299-7ytIBcjALWQQShwMQfw9",
//  "provider":"Meta",
//  "model":"meta-llama/llama-3.3-8b-instruct:free",
//  "object":"chat.completion.chunk",
//  "created":1756743300,
//  "choices":[
//      {
//      "index":0,
//      "delta":{"role":"assistant","content":""},
//      "finish_reason":null,
//      "native_finish_reason":null,
//      "logprobs":null
//      }
//  ],
//  "usage":{
//      "prompt_tokens":42,
//      "completion_tokens":2,
//      "total_tokens":44,
//      "cost":0,"
//      is_byok":false,
//      "prompt_tokens_details":{"cached_tokens":0,"audio_tokens":0},
//      "cost_details":{"upstream_inference_cost":null,"upstream_inference_prompt_cost":0,"upstream_inference_completions_cost":0},
//      "completion_tokens_details":{"reasoning_tokens":0,"image_tokens":0}}
//  }

pub struct ChatCompletionsResponse {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

pub struct Choice {
    pub delta: Message,
}

pub struct Usage {
    pub cost: f32, // In dollars, usually a very small fraction
}

pub struct LastData {
    pub opts: PromptOpts,
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,
}

#[derive(Clone)]
pub struct PromptOpts {
    pub prompt: Option<String>,
    /// Model IDs, e.g. 'moonshotai/kimi-k2'
    pub models: Vec<String>,
    /// Prefered provider slug
    pub provider: Option<String>,
    /// System prompt
    pub system: Option<String>,
    /// How to choose a provider
    pub priority: Option<Priority>,
    /// Reasoning config
    pub reasoning: Option<ReasoningConfig>,
    /// Show reasoning output
    pub show_reasoning: Option<bool>,
    /// Don't show stats after request
    pub quiet: Option<bool>,
    /// Whether to merge in the default settings from config file
    pub merge_config: bool,
    /// Images to attach to the request.
    pub files: Vec<String>,
    // If the prompt is '@<filename>' we save filename in here
    pub prompt_filename: Option<String>,
}

impl Default for PromptOpts {
    fn default() -> Self {
        Self {
            prompt: None,
            models: vec![DEFAULT_MODEL.to_string()],
            provider: None,
            system: None,
            priority: None,
            reasoning: Some(ReasoningConfig::default()),
            show_reasoning: Some(false),
            quiet: Some(false),
            merge_config: true,
            files: vec![],
            prompt_filename: None,
        }
    }
}

impl PromptOpts {
    // Replace any blank or None fields on Self with values from other
    // or with the defaults.
    // After this call a PromptOpts is ready to use.
    pub fn merge(&mut self, o: PromptOpts) {
        self.prompt.get_or_insert(o.prompt.unwrap_or_default());
        self.quiet.get_or_insert(o.quiet.unwrap_or(DEFAULT_QUIET));
        if self.models.is_empty() {
            // We don't merge the models, otherwise we'd try to query both the
            // cmd line one, and the config file default.
            self.models = o.models;
        }
        if let Some(provider) = o.provider {
            self.provider.get_or_insert(provider);
        }
        if let Some(system) = o.system {
            self.system.get_or_insert(system);
        }
        if let Some(priority) = o.priority {
            self.priority.get_or_insert(priority);
        }
        self.reasoning
            .get_or_insert(o.reasoning.unwrap_or_default());
        self.show_reasoning
            .get_or_insert(o.show_reasoning.unwrap_or(DEFAULT_SHOW_REASONING));
        self.files.extend(o.files);
    }

    pub fn messages(&mut self) -> OrtResult<Vec<Message>> {
        // A Message is quite small, an enum and two Option<String>.
        // Capacity 3 for:
        // - System message (optional)
        // - User message (required)
        // - and the assistant message that LastWriter appends, to save a realloc.
        let mut messages = Vec::with_capacity(3);
        if let Some(sys) = self.system.take() {
            messages.push(crate::Message::system(sys));
        };
        let user_message = if self.files.is_empty() {
            crate::Message::user(self.prompt.take().unwrap())
        } else {
            crate::Message::with_files(self.prompt.take().unwrap(), &self.files)?
        };
        messages.push(user_message);
        Ok(messages)
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub enum Priority {
    Price,
    #[default]
    Latency,
    Throughput,
}

impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::Price => "price",
            Priority::Latency => "latency",
            Priority::Throughput => "throughput",
        }
    }
}

impl FromStr for Priority {
    type Err = OrtError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "price" => Ok(Priority::Price),
            "latency" => Ok(Priority::Latency),
            "throughput" => Ok(Priority::Throughput),
            _ => Err(ort_error(
                ErrorKind::FormatError,
                "Priority: Invalid string value",
            )), // Handle unknown strings
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct ReasoningConfig {
    pub enabled: bool,
    pub effort: Option<ReasoningEffort>,
    pub tokens: Option<u32>,
}

impl ReasoningConfig {
    pub fn off() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub enum ReasoningEffort {
    None, // GPT 5.x only
    Low,
    #[default]
    Medium,
    High,
    XHigh, // GPT 5.x only
}

impl ReasoningEffort {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReasoningEffort::None => "none",
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium => "medium",
            ReasoningEffort::High => "high",
            ReasoningEffort::XHigh => "xhigh",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: Vec<Content>,
    pub reasoning: Option<String>,
}

impl Message {
    pub fn new(role: Role, content: Option<String>, reasoning: Option<String>) -> Self {
        let content = content.map_or_else(Vec::new, |content| vec![Content::Text(content)]);
        Self::with_content(role, content, reasoning)
    }

    pub fn with_content(role: Role, content: Vec<Content>, reasoning: Option<String>) -> Self {
        Message {
            role,
            content,
            reasoning,
        }
    }
    pub fn system(content: String) -> Self {
        Self::new(Role::System, Some(content), None)
    }
    pub fn user(content: String) -> Self {
        Self::new(Role::User, Some(content), None)
    }
    pub fn assistant(content: String) -> Self {
        Self::new(Role::Assistant, Some(content), None)
    }

    pub fn with_files(prompt: String, filenames: &[String]) -> OrtResult<Self> {
        // First message is the user's prompt as Text
        let mut m = Self::user(prompt);
        // Then the files as Image
        for f in filenames {
            if f.starts_with("http") {
                m.content.push(Content::ImageUrl(f.clone()));
            } else {
                let pf = PromptFile::load(f).map_err(|err| ort_error(ErrorKind::Other, err))?;
                m.content.push(pf.into_content());
            }
        }
        Ok(m)
    }

    pub fn text(&self) -> Option<&str> {
        match self.content.as_slice() {
            [Content::Text(text)] => Some(text.as_str()),
            _ => None,
        }
    }

    /// Estimate size in bytes
    pub fn size(&self) -> u32 {
        let content_len: usize = self.content.iter().map(Content::len).sum();
        let reasoning_len = self.reasoning.as_ref().map(|c| c.len()).unwrap_or(0);
        (content_len.max(reasoning_len) + 10) as u32
    }
}

#[derive(Debug, Clone)]
pub enum Content {
    Text(String),
    // Just the base64 encoded data
    Image {
        mime_type: &'static str,
        base64: String,
    },
    ImageUrl(String),
    File(PromptFile),
}

impl Content {
    pub fn len(&self) -> usize {
        use Content::*;
        match self {
            Text(s) => s.len(),
            Image { base64, .. } => base64.len(),
            ImageUrl(s) => s.len(),
            File(f) => f.len(),
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Content::Text(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn content(&self) -> &str {
        use Content::*;
        match self {
            Text(s) => s.as_ref(),
            Image { base64, .. } => base64.as_ref(),
            ImageUrl(s) => s.as_ref(),
            File(f) => f.base64.as_ref(),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

impl FromStr for Role {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "system" => Ok(Role::System),
            "user" => Ok(Role::User),
            "assistant" => Ok(Role::Assistant),
            _ => Err("Invalid role"),
        }
    }
}

#[derive(Clone, Default)]
pub enum Response {
    /// The first time we get anything at all on the SSE stream
    Start,
    /// Reasoning events - start, some thoughts, stop
    Think(ThinkEvent),
    /// The good stuff
    Content(String),
    /// Summary stats at the end of the run
    Stats(super::stats::Stats),
    /// Less good things. Often you mistyped the model name.
    Error(String),
    /// For default
    #[default]
    None,
}

#[derive(Debug, Clone)]
pub enum ThinkEvent {
    Start,
    Content(String),
    Stop,
}

#[derive(Debug, Clone)]
pub enum PromptFileKind {
    Image,
    // Typically a PDF
    File,
    //Audio,
}

#[derive(Debug, Clone)]
pub struct PromptFile {
    kind: PromptFileKind,
    pub filename: String,
    pub base64: String,
}

impl PromptFile {
    /// Load disk file, identify, and base64 encode it
    pub fn load(filename: &str) -> Result<Self, &'static str> {
        let kind = if IMAGE_EXT.iter().any(|ext| filename.ends_with(ext)) {
            PromptFileKind::Image
        } else {
            PromptFileKind::File
        };
        let data = filename_read_to_bytes(filename)?;
        Ok(PromptFile {
            kind,
            filename: filename.split('/').next_back().unwrap().to_string(),
            base64: base64::encode(&data),
        })
    }

    pub fn len(&self) -> usize {
        self.base64.len()
    }

    pub(crate) fn from_parts(kind: PromptFileKind, filename: String, base64: String) -> Self {
        Self {
            kind,
            filename,
            base64,
        }
    }

    pub fn into_content(self) -> Content {
        match self.kind {
            PromptFileKind::Image => Content::Image {
                mime_type: self.mime_type(),
                base64: self.base64,
            },
            PromptFileKind::File => Content::File(self),
        }
    }

    pub fn mime_type(&self) -> &'static str {
        for (ext, mime) in MIME_TYPES {
            if self.filename.to_lowercase().ends_with(ext) {
                return mime;
            }
        }
        "application/octet-stream"
    }
}

#[derive(Clone)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParameter>,
    pub required_parameters: Vec<String>,
}

#[derive(Clone)]
pub struct ToolParameter {
    pub name: String,
    pub param_type: String,
    pub description: String,
}
