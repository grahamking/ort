//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::fmt;
use std::str::FromStr;

const DEFAULT_SHOW_REASONING: bool = false;
const DEFAULT_QUIET: bool = false;
pub const DEFAULT_MODEL: &str = "google/gemma-3n-e4b-it:free";

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

#[derive(Default, Debug)]
pub struct LastData {
    pub opts: PromptOpts,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone)]
pub struct PromptOpts {
    pub prompt: Option<String>,
    /// Model ID, e.g. 'moonshotai/kimi-k2'
    pub model: Option<String>,
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
}

impl Default for PromptOpts {
    fn default() -> Self {
        Self {
            prompt: None,
            model: Some(DEFAULT_MODEL.to_string()),
            provider: None,
            system: None,
            priority: None,
            reasoning: Some(ReasoningConfig::default()),
            show_reasoning: Some(false),
            quiet: Some(false),
            merge_config: true,
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
        if o.model.is_some() {
            self.model.get_or_insert(o.model.unwrap());
        }
        if o.provider.is_some() {
            self.provider.get_or_insert(o.provider.unwrap());
        }
        if o.system.is_some() {
            self.system.get_or_insert(o.system.unwrap());
        }
        if o.priority.is_some() {
            self.priority.get_or_insert(o.priority.unwrap());
        }
        self.reasoning
            .get_or_insert(o.reasoning.unwrap_or_default());
        self.show_reasoning
            .get_or_insert(o.show_reasoning.unwrap_or(DEFAULT_SHOW_REASONING));
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
    type Err = fmt::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "price" => Ok(Priority::Price),
            "latency" => Ok(Priority::Latency),
            "throughput" => Ok(Priority::Throughput),
            _ => Err(fmt::Error), // Handle unknown strings
        }
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Priority::Price => write!(f, "price"),
            Priority::Latency => write!(f, "latency"),
            Priority::Throughput => write!(f, "throughput"),
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
    Low,
    #[default]
    Medium,
    High,
}

impl ReasoningEffort {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium => "medium",
            ReasoningEffort::High => "high",
        }
    }
}

impl fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReasoningEffort::Low => write!(f, "low"),
            ReasoningEffort::Medium => write!(f, "medium"),
            ReasoningEffort::High => write!(f, "high"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub(crate) role: Role,
    pub(crate) content: Option<String>,
    pub(crate) reasoning: Option<String>,
}

impl Message {
    pub(crate) fn new(role: Role, content: Option<String>, reasoning: Option<String>) -> Self {
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

    /// Estimate size in bytes
    pub fn size(&self) -> u32 {
        self.content
            .as_ref()
            .or(self.reasoning.as_ref())
            .map(|c| c.len())
            .unwrap_or(0) as u32
            + 10
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

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::System => write!(f, "system"),
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
        }
    }
}
