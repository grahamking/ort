//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::fmt;
use std::str::FromStr;

use serde::Serialize;

const DEFAULT_SHOW_REASONING: bool = false;
const DEFAULT_QUIET: bool = false;
pub const DEFAULT_MODEL: &str = "google/gemma-3n-e4b-it:free";

#[derive(Default, Debug, Clone, Serialize)]
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

#[derive(Default, Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Price,
    #[default]
    Latency,
    Throughput,
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

#[derive(Default, Debug, Clone, Serialize)]
pub struct ReasoningConfig {
    pub enabled: bool,
    pub effort: Option<ReasoningEffort>,
    pub tokens: Option<u32>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    #[default]
    Medium,
    High,
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

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    role: Role,
    content: String,
}

impl Message {
    pub(crate) fn new(role: Role, content: String) -> Self {
        Message { role, content }
    }
    pub fn user(content: String) -> Self {
        Self::new(Role::User, content)
    }
    pub fn assistant(content: String) -> Self {
        Self::new(Role::Assistant, content)
    }
}

#[derive(Debug, Copy, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

impl FromStr for Role {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "user" => Ok(Role::User),
            "assistant" => Ok(Role::Assistant),
            _ => Err("Invalid role"),
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
        }
    }
}

#[derive(Default, Serialize)]
pub struct LastData {
    pub opts: PromptOpts,
    pub messages: Vec<Message>,
}
