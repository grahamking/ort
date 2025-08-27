//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct CommonPromptOpts {
    pub model: Option<String>,
    pub provider: Option<String>,
    pub system: Option<String>,
    pub priority: Option<Priority>,
    /// Reasoning config
    pub reasoning: Option<ReasoningConfig>,
    /// Show reasoning output
    pub show_reasoning: Option<bool>,
}

#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize)]
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

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningConfig {
    pub enabled: bool,
    pub effort: Option<ReasoningEffort>,
    pub tokens: Option<u32>,
}

#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize)]
pub struct Message {
    role: Role,
    content: String,
}

impl Message {
    pub fn new(role: Role, content: String) -> Self {
        Message { role, content }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
        }
    }
}
