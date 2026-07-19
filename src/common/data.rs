//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

#![allow(dead_code)]

use core::str::FromStr;

extern crate alloc;
use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::common::json_parser::{JsonField, Parser, autoparser};
use crate::common::{base64, config};
use crate::utils::filename_read_to_bytes;
use crate::{ErrorKind, OrtResult, ort_error};

const IMAGE_EXT: [&str; 4] = ["jpg", "JPG", "png", "PNG"];

// Keep in sync with src/input/cli.rs
// Ideally this would be openrouter/free but it picks very bad models.
pub const DEFAULT_MODEL: &str = "nvidia/nemotron-3-super-120b-a12b:free";

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

impl ChatCompletionsResponse {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [
            JsonField::new_simple_string("provider"),
            JsonField::new_simple_string("model"),
            JsonField::new_vec_raw("choices"),
            JsonField::new_raw("usage"),
        ];
        autoparser(json, &mut fields)?;

        let mut choices = vec![];
        if let Some(v) = fields[2].get_vec_raw() {
            for c in v {
                choices.push(Choice::from_json(&c)?);
            }
        }

        let usage = fields[3]
            .get_raw()
            .as_deref()
            .map(Usage::from_json)
            .transpose()?;

        Ok(ChatCompletionsResponse {
            provider: fields[0].get_string(),
            model: fields[1].get_string(),
            choices,
            usage,
        })
    }
}

pub struct Choice {
    pub delta: Message,
    pub finish_reason: Option<String>,
}

impl Choice {
    pub fn is_tool_call_finish(&self) -> bool {
        matches!(self.finish_reason.as_deref(), Some("tool_calls"))
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut fields = [
            JsonField::new_raw("delta"),
            JsonField::new_simple_string("finish_reason"),
        ];
        autoparser(json, &mut fields)?;
        let delta_json = fields[0].get_raw().expect("Missing delta in message");

        Ok(Choice {
            delta: Message::from_json(&delta_json)?,
            finish_reason: fields[1].get_string(),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct ToolCall {
    pub index: u32,
    pub id: Option<String>,
    pub function: Function,
}

impl ToolCall {
    /// Update the fields of this tool call from partial.
    /// Some models send first the name of the function, and then
    /// the arguments in a later message.
    pub fn update_from(&mut self, partial: &ToolCall) {
        if self.id.is_none() {
            self.id = partial.id.clone();
        }
        if !partial.function.arguments.is_empty() {
            self.function
                .arguments
                .push_str(&partial.function.arguments);
        }
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut fields = [
            JsonField::new_int("index"),
            JsonField::new_simple_string("id"),
            JsonField::new_raw("function"),
        ];
        autoparser(json, &mut fields)?;

        let function_json = fields[2].get_raw().expect("Missing function in tool call");
        Ok(ToolCall {
            index: fields[0].get_int().unwrap_or_default(),
            id: fields[1].get_string(),
            function: Function::from_json(&function_json)?,
        })
    }

    pub fn as_string(&self) -> String {
        self.function.name.clone() + ": " + &self.function.arguments
    }
}

#[derive(Debug, Clone, Default)]
pub struct Function {
    pub name: String,
    pub arguments: String, // JSON
}

impl Function {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut fields = [
            JsonField::new_simple_string("name"),
            JsonField::new_string("arguments"),
        ];
        autoparser(json, &mut fields)?;
        Ok(Function {
            name: fields[0].get_string().unwrap_or_default(),
            arguments: fields[1].get_string().unwrap_or_default(),
        })
    }
}

pub struct Usage {
    // In dollars, usually a very small fraction
    pub cost: f32,
    // How many times the OpenRouter server-side search tool was called
    pub web_search_requests: Option<u32>,
}

impl Usage {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut fields = [
            JsonField::new_float("cost"),
            JsonField::new_raw("server_tool_use"),
        ];
        autoparser(json, &mut fields)?;
        let mut web_search_requests = None;
        if let Some(server_tool_json) = fields[1].get_raw() {
            let mut server_tool_fields = [JsonField::new_int("web_search_requests")];
            autoparser(&server_tool_json, &mut server_tool_fields)?;
            server_tool_fields[0]
                .get_int()
                .map(|stu| web_search_requests.replace(stu));
        }
        Ok(Usage {
            cost: fields[0].get_float().unwrap_or_default(),
            web_search_requests,
        })
    }
}

pub struct LastData {
    pub opts: PromptOpts,
    pub messages: Vec<Message>,
    pub tools: Vec<&'static Tool>,
}

impl LastData {
    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        if json.is_empty() {
            return Err(
                "Cannot continue, last-<$TMUX_PANE>.json file is empty. Usually that mains previous run failed.".into(),
            );
        }

        let mut fields = [
            JsonField::new_raw("opts"),
            JsonField::new_vec_raw("messages"),
            JsonField::new_vec_raw("tools"),
        ];
        autoparser(json, &mut fields)?;

        let opts = fields[0]
            .get_raw()
            .as_deref()
            .map(PromptOpts::from_json)
            .transpose()?;

        let mut messages = vec![];
        if let Some(msg_vec) = fields[1].get_vec_raw() {
            for m in msg_vec {
                messages.push(Message::from_json(&m)?);
            }
        }

        let mut tools = vec![];
        if let Some(tools_vec) = fields[2].get_vec_raw() {
            for t in tools_vec {
                tools.push(Tool::from_json(&t)?);
            }
        }

        Ok(LastData {
            opts: opts.expect("Missing prompt opts"),
            messages,
            tools,
        })
    }
}

#[derive(Clone)]
pub struct PromptOpts {
    pub config_file: Option<String>,

    pub prompt: Option<String>,
    /// Model IDs, e.g. 'moonshotai/kimi-k2'
    pub models: Vec<String>,
    /// Preferred provider slug
    pub provider: Option<String>,
    /// System prompt
    pub system: Option<String>,
    /// How to choose a provider
    pub priority: Option<Priority>,
    /// Reasoning effort level
    pub effort: Option<ReasoningEffort>,
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
    // Include web_search and web_fetch server-side tools
    pub include_web_tools: Option<bool>,
}

impl Default for PromptOpts {
    fn default() -> Self {
        Self {
            config_file: None,
            prompt: None,
            models: vec![DEFAULT_MODEL.to_string()],
            provider: None,
            system: None,
            priority: None,
            effort: Some(ReasoningEffort::default()),
            show_reasoning: Some(false),
            quiet: Some(false),
            merge_config: true,
            files: vec![],
            prompt_filename: None,
            include_web_tools: None,
        }
    }
}

impl PromptOpts {
    // Replace any blank or None fields on Self with values from other
    // or with the defaults.
    // After this call a PromptOpts is ready to use.
    pub fn merge(&mut self, cfg: &config::Cfg) {
        if self.models.is_empty() {
            // We don't merge the models, otherwise we'd try to query both the
            // cmd line one, and the config file default.
            self.models = cfg.models.clone();
        }
        if let Some(provider) = cfg.provider.as_ref() {
            self.provider.get_or_insert_with(|| provider.to_string());
        }
        if let Some(prompt) = cfg.prompt.as_ref() {
            self.prompt.get_or_insert_with(|| prompt.to_string());
        }
        if let Some(prompt_filename) = cfg.prompt_filename.as_ref() {
            self.prompt_filename
                .get_or_insert_with(|| prompt_filename.to_string());
        }
        if let Some(system) = cfg.system_prompt.as_ref() {
            self.system.get_or_insert_with(|| system.to_string());
        }
        if let Some(priority) = cfg.priority {
            self.priority.get_or_insert(priority);
        }
        self.quiet.get_or_insert(cfg.quiet);
        self.show_reasoning.get_or_insert(cfg.show_reasoning);
        self.include_web_tools.get_or_insert(cfg.include_web_tools);
        if let Some(effort) = cfg.effort {
            self.effort.get_or_insert(effort);
        }
        if self.files.is_empty() {
            self.files = cfg.files.clone();
        }
    }

    pub fn merge_opts(&mut self, o: PromptOpts) {
        self.prompt.get_or_insert(o.prompt.unwrap_or_default());
        self.quiet.get_or_insert(o.quiet.unwrap_or(false));
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
        self.effort.get_or_insert(o.effort.unwrap_or_default());
        self.show_reasoning
            .get_or_insert(o.show_reasoning.unwrap_or(false));
        self.include_web_tools
            .get_or_insert(o.include_web_tools.unwrap_or_default());
        self.files.extend(o.files);
    }

    pub fn messages(&mut self) -> OrtResult<Vec<Message>> {
        // A Message is quite small, an enum and two Option<String>.
        // Capacity 3 for:
        // - System message (optional)
        // - User message (required)
        // - and the assistant message that LastWriter appends, to save a realloc.
        let mut messages = Vec::with_capacity(3);
        if let Some(sys) = self.system.clone() {
            messages.push(crate::Message::system(sys));
        };
        let user_message = if self.files.is_empty() {
            crate::Message::user(self.prompt.clone().unwrap())
        } else {
            crate::Message::with_files(self.prompt.take().unwrap(), &self.files)?
        };
        messages.push(user_message);
        Ok(messages)
    }

    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [
            JsonField::new_string("prompt"),
            JsonField::new_simple_string("model"),
            JsonField::new_simple_string("provider"),
            JsonField::new_string("system"),
            JsonField::new_simple_string("priority"),
            JsonField::new_simple_string("effort"),
            JsonField::new_bool("show_reasoning"),
            JsonField::new_bool("quiet"),
            JsonField::new_bool("merge_config"),
            JsonField::new_bool("include_web_tools"),
        ];
        autoparser(json, &mut fields)?;

        let priority = fields[4]
            .get_string()
            .as_deref()
            .map(Priority::from_str)
            .transpose()?;
        let effort = fields[5]
            .get_string()
            .as_deref()
            .map(ReasoningEffort::from_str)
            .transpose()?;

        Ok(PromptOpts {
            config_file: None,
            prompt: fields[0].get_string(),
            models: fields[1].get_string().map(|m| vec![m]).unwrap_or_default(),
            provider: fields[2].get_string(),
            system: fields[3].get_string(),
            priority,
            effort,
            show_reasoning: fields[6].get_bool(),
            quiet: fields[7].get_bool(),
            merge_config: fields[8].get_bool().unwrap_or(true),
            prompt_filename: None,
            // TODO: store files in last json, so resume works with files
            files: vec![],
            include_web_tools: fields[9].get_bool(),
        })
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
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
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "price" => Ok(Priority::Price),
            "latency" => Ok(Priority::Latency),
            "throughput" => Ok(Priority::Throughput),
            _ => Err("Priority: Invalid string value"),
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

impl FromStr for ReasoningEffort {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use ReasoningEffort::*;
        match s.to_lowercase().as_str() {
            "none" | "off" => Ok(ReasoningEffort::None),
            "low" => Ok(Low),
            "medium" => Ok(Medium),
            "high" => Ok(High),
            "xhigh" => Ok(XHigh),
            _ => Err("Effort: Invalid string value"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: Vec<Content>,
    pub reasoning: Option<String>,
    /// For Role::Assistant requesting a tool call
    pub tool_calls: Vec<ToolCall>,
    /// For Role::Tool returning a result
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn new(role: Role, content: Option<String>, reasoning: Option<String>) -> Self {
        let content = content.map_or_else(Vec::new, |content| vec![Content::Text(content)]);
        Self::with_content(role, content, reasoning, vec![], None)
    }

    pub fn with_content(
        role: Role,
        content: Vec<Content>,
        reasoning: Option<String>,
        tool_calls: Vec<ToolCall>,
        tool_call_id: Option<String>,
    ) -> Self {
        Message {
            role,
            content,
            reasoning,
            tool_calls,
            tool_call_id,
        }
    }
    pub fn system(content: String) -> Self {
        Self::new(Role::System, Some(content), None)
    }
    pub fn user(content: String) -> Self {
        Self::new(Role::User, Some(content), None)
    }
    pub fn assistant(content: String) -> Self {
        // TODO: also send reasoning back
        Self::new(Role::Assistant, Some(content), None)
    }
    pub fn assistant_with_tool_call(content: String, tool_calls: Vec<ToolCall>) -> Self {
        Self::with_content(
            Role::Assistant,
            vec![Content::Text(content)],
            None, // TODO: also send reasoning back
            tool_calls,
            None,
        )
    }
    pub fn tool(id: String, content: String) -> Self {
        Self::with_content(
            Role::Tool,
            vec![Content::Text(content)],
            None,
            vec![],
            Some(id),
        )
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

    pub fn from_json(json: &str) -> Result<Self, Cow<'static, str>> {
        let mut fields = [
            JsonField::new_simple_string("role"),
            JsonField::new_raw("content"),
            JsonField::new_string("reasoning"),
            JsonField::new_vec_raw("tool_calls"),
        ];
        autoparser(json, &mut fields)?;

        let role = fields[0]
            .get_raw()
            .as_deref()
            .map(Role::from_str)
            .transpose()?;
        let reasoning = fields[2].get_string();

        let mut tool_calls = vec![];
        if let Some(tool_calls_str_vec) = fields[3].get_vec_raw() {
            for t in tool_calls_str_vec {
                tool_calls.push(ToolCall::from_json(&t)?);
            }
        }

        // Content can be a string or an array, so do extra parsing
        let mut content = vec![];
        if let Some(content_str) = fields[1].get_raw() {
            let mut p = Parser::new(&content_str);
            if p.peek_is_null() {
                p.skip_null()?;
            } else if p.peek() == Some(b'[') {
                p.expect(b'[')?;
                p.skip_ws();
                if !p.try_consume(b']') {
                    loop {
                        let j = p.value_slice()?;
                        content.push(Content::from_json(j)?);
                        p.skip_ws();
                        if p.try_consume(b',') {
                            continue;
                        }
                        p.skip_ws();
                        if p.try_consume(b']') {
                            break;
                        }
                    }
                }
            } else {
                content.push(Content::Text(p.parse_string()?));
            }
        }

        Ok(Message::with_content(
            // NVIDIA doesn't always send it. sus.
            role.unwrap_or(Role::Assistant),
            content,
            reasoning,
            tool_calls,
            None,
        ))
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

    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut fields = [
            JsonField::new_simple_string("type"),
            JsonField::new_string("text"),
            JsonField::new_raw("image_url"),
            JsonField::new_raw("file"),
        ];
        autoparser(json, &mut fields)?;

        let kind = fields[0].get_string();
        let text = fields[1].get_string();

        let mut base64_data = None;
        let mut mime_type = None;
        let mut image_url = None;
        if let Some(image_url_str) = fields[2].get_raw() {
            if image_url_str.starts_with("http") {
                image_url = Some(image_url_str);
            } else {
                let (base64, mt) = parse_image_url(&image_url_str)?;
                base64_data = Some(base64);
                mime_type = Some(mt);
            }
        }

        let file = fields[3]
            .get_raw()
            .as_deref()
            .map(PromptFile::from_json)
            .transpose()?;

        match kind.as_deref() {
            Some("text") => Ok(Content::Text(text.ok_or("missing text")?)),
            Some("image_url") => {
                if let Some(image_url) = image_url {
                    Ok(Content::ImageUrl(image_url.to_string()))
                } else {
                    Ok(Content::Image {
                        base64: base64_data.ok_or("missing image_url")?,
                        mime_type: mime_type.unwrap(),
                    })
                }
            }
            Some("file") => Ok(Content::File(file.ok_or("missing file")?)),
            Some(other) => Err("unsupported content type: ".to_string() + other),
            None => Err("missing content type".to_string()),
        }
    }
}

/// Returns (base64_data, mime_type)
fn parse_image_url(json: &str) -> Result<(String, &'static str), String> {
    let mut fields = [JsonField::new_string("url")];
    autoparser(json, &mut fields)?;

    let url_str = fields[0].get_string().expect("Missing image URL");
    if url_str.starts_with("data:image/jpeg") {
        Ok((
            url_str
                .strip_prefix("data:image/jpeg;base64,")
                .unwrap()
                .to_string(),
            "image/jpeg",
        ))
    } else if url_str.starts_with("data:image/png") {
        Ok((
            url_str
                .strip_prefix("data:image/png;base64,")
                .unwrap()
                .to_string(),
            "image/png",
        ))
    } else {
        Err("Invalid mime type in saved image_url".to_string())
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
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
            "tool" => Ok(Role::Tool),
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
    /// Let's do stuff
    ToolCalls(Vec<ToolCall>),
    /// A clean way to display a tool call
    ToolDisplay(ToolDisplay),
    /// Summary stats at the end of the run
    Stats(super::stats::Stats),
    /// Less good things. Often you mistyped the model name.
    Error(String),
    /// For agent mode, user prompt
    Prompt(String),
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

    pub fn from_json(json: &str) -> Result<Self, String> {
        let mut fields = [
            JsonField::new_string("filename"),
            JsonField::new_raw("file_data"),
        ];
        autoparser(json, &mut fields)?;

        let filename = fields[0].get_string();

        let base64 = fields[1].get_raw().map(|data| {
            data.strip_prefix("data:application/pdf;base64,")
                .unwrap_or(data.as_str())
                .to_string()
        });

        Ok(PromptFile::from_parts(
            PromptFileKind::File,
            filename.ok_or("missing filename")?,
            base64.ok_or("missing file_data")?,
        ))
    }
}

#[derive(Clone)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: &'static [ToolParameter],
    pub required_parameters: &'static [&'static str],
}

// This one doesn't use autoparser because we need to skip a lot of the function object.
// Later we likely will use all of it an use autoparser.
impl Tool {
    pub fn find_by_name(name: &str) -> Option<&'static Tool> {
        super::tools::ALL_TOOLS
            .iter()
            .find(|t| t.name == name)
            .map(|v| &**v)
    }

    pub fn from_json(json: &str) -> Result<&'static Self, Cow<'static, str>> {
        let mut p = Parser::new(json);
        p.skip_ws();

        // Skip the preamble:
        // {"type": "function", "function": {
        p.expect(b'{')?;
        p.skip_ws();
        p.skip_value()?; // skip "type"
        p.expect(b':')?;
        p.skip_ws();
        p.skip_value()?; // skip "function" from type:function
        p.expect(b',')?;
        p.skip_ws();
        p.skip_value()?; // skip "function" as key
        p.expect(b':')?;
        p.skip_ws();
        p.expect(b'{')?;

        let mut name = String::new();
        //let mut description = String::new();
        //let mut parameters = vec![];
        //let mut required_parameters = vec![];

        loop {
            p.skip_ws();
            if p.try_consume(b'}') {
                break;
            }

            let key = p
                .parse_simple_str()
                .map_err(|err| "Message parsing key: ".to_string() + err)?;
            p.skip_ws();
            p.expect(b':')?;
            p.skip_ws();

            match key {
                "name" => {
                    name = p.parse_simple_str()?.to_string();
                    // The tools are statically know. We only need the name
                    // to look it up.
                    break;
                }
                /*
                "description" => {
                    description = p.parse_string()?;
                }
                "parameters" => {
                    // Skip
                    // {"type": "object", "properties": {
                    p.expect(b'{')?;
                    p.skip_value()?; // skip "type"
                    p.expect(b':')?;
                    p.skip_ws();
                    p.skip_value()?; // skip "object"
                    p.expect(b',')?;
                    p.skip_ws();
                    p.skip_value()?; // skip "properties"
                    p.expect(b':')?;
                    p.skip_ws();
                    p.expect(b'{')?;

                    let param_name = p.parse_simple_str()?.to_string();
                    p.skip_ws();
                    p.expect(b':')?;
                    p.skip_ws();
                    p.expect(b'{')?;
                    p.skip_ws();

                    let mut param_type = None;
                    let mut description = None;
                    loop {
                        let param_key = p.parse_simple_str()?;
                        p.skip_ws();
                        p.expect(b':')?;
                        p.skip_ws();

                        match param_key {
                            "type" => {
                                param_type = Some(p.parse_simple_str()?.to_string());
                            }
                            "description" => {
                                description = Some(p.parse_simple_str()?.to_string());
                            }
                            _ => {}
                        }
                        p.skip_ws();
                        if p.try_consume(b',') {
                            continue;
                        } else {
                            p.expect(b'}')?;
                            break;
                        }
                    }

                    // TODO: description can be optional. and no unwrap
                    parameters.push(ToolParameter {
                        name: param_name,
                        param_type: param_type.unwrap(),
                        description: description.unwrap(),
                    });
                }
                "required" => {
                    p.expect(b'[')?;
                    p.skip_ws();
                    if !p.try_consume(b']') {
                        loop {
                            let param_name = p.parse_simple_str()?;
                            required_parameters.push(param_name.to_string());
                            p.skip_ws();
                            if p.try_consume(b',') {
                                continue;
                            }
                            p.skip_ws();
                            if p.try_consume(b']') {
                                break;
                            }
                        }
                    }
                }
                */
                _ => {
                    p.skip_value()?;
                }
            }
            p.skip_ws();
            if p.try_consume(b',') {
                continue;
            } else {
                p.expect(b'}')?;
                break;
            }
        }

        let Some(t) = Tool::find_by_name(&name) else {
            // TODO: Error really needs to be a String
            return Err("Tool not found".into());
        };
        Ok(t)
    }
}

#[derive(Clone)]
pub struct ToolParameter {
    pub name: &'static str,
    pub param_type: &'static str,
    pub description: &'static str,
}

/// Info AgentWriter needs to display a tool call.
#[derive(Clone)]
pub struct ToolDisplay {
    // Capitalized and with a space at the end please
    pub name: &'static str,
    pub arguments: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LastData;

    #[test]
    fn cpo1() {
        let s = r#"
 {
     "prompt": "\n\nExample JSON 1: {\"enabled\": false}\n",
     "model": "google/gemma-3n-e4b-it:free",
     "system": "Make your answer concise but complete. No yapping. Direct professional tone. No emoji.",
     "show_reasoning": false,
     "include_web_tools": true,
     "effort": "high",
     "merge_config": true
 }
 "#;
        let opts = PromptOpts::from_json(s).unwrap();
        assert!(!opts.show_reasoning.unwrap());
        assert_eq!(opts.models, vec!["google/gemma-3n-e4b-it:free"]);
        assert_eq!(opts.effort, Some(ReasoningEffort::High));
        assert!(opts.merge_config);
        assert!(opts.include_web_tools.unwrap());
    }

    #[test]
    fn cpo2() {
        let s = r#"
    {"model":"openai/gpt-5","provider":"openai","system":"Make your answer concise but complete. No yapping. Direct professional tone. No emoji.","priority":null,"effort":"high","show_reasoning":false,"quiet":true}
    "#;
        let opts = PromptOpts::from_json(s).unwrap();
        assert!(!opts.show_reasoning.unwrap());
        assert_eq!(opts.models, vec!["openai/gpt-5"]);
        assert_eq!(opts.effort, Some(ReasoningEffort::High));
    }

    #[test]
    fn last_data() {
        let s = r#"
{"opts":{"model":"google/gemma-3n-e4b-it:free","provider":"google-ai-studio","system":"Make your answer concise but complete. No yapping. Direct professional tone. No emoji.","priority":null,"reasoning":{"enabled":false,"effort":null,"tokens":null},"show_reasoning":false},"messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hello there! 😊How can I help you today? I'm ready for anything – questions, stories, ideas, or just a friendly chat!Let me know what's on your mind. ✨"}]}
"#;
        let l = LastData::from_json(s).unwrap();
        assert_eq!(l.opts.provider.as_deref(), Some("google-ai-studio"));
        assert_eq!(l.messages.len(), 2);
    }

    #[test]
    fn test_usage() {
        let s = r#"{"prompt_tokens":42,"completion_tokens":2,"total_tokens":44,"cost":0.0534,"is_byok":false,"prompt_tokens_details":{"cached_tokens":0,"audio_tokens":0},"cost_details":{"upstream_inference_cost":null,"upstream_inference_prompt_cost":0,"upstream_inference_completions_cost":0},"completion_tokens_details":{"reasoning_tokens":0,"image_tokens":0}}"#;
        let usage = Usage::from_json(s).unwrap();
        assert_eq!(usage.cost, 0.0534);
    }

    #[test]
    fn test_choice() {
        let s = r#"{"index":0,"delta":{"role":"assistant","content":"Hello"},"finish_reason":"stop","native_finish_reason":"stop","logprobs":null}"#;
        let choice = Choice::from_json(s).unwrap();
        assert_eq!(choice.delta.text(), Some("Hello"));
    }

    #[test]
    fn test_chat_completions_response_simple() {
        let arr = [
            r#"{"id":"gen-1756743299-7ytIBcjALWQQShwMQfw9","provider":"Meta","model":"meta-llama/llama-3.3-8b-instruct:free","object":"chat.completion.chunk","created":1756743300,"choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756743299-7ytIBcjALWQQShwMQfw9","provider":"Meta","model":"meta-llama/llama-3.3-8b-instruct:free","object":"chat.completion.chunk","created":1756743300,"choices":[{"index":0,"delta":{"role":"assistant","content":"Hello"},"finish_reason":"stop","native_finish_reason":"stop","logprobs":null}]}"#,
            r#"{"id":"gen-1756743299-7ytIBcjALWQQShwMQfw9","provider":"Meta","model":"meta-llama/llama-3.3-8b-instruct:free","object":"chat.completion.chunk","created":1756743300,"choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null,"native_finish_reason":null,"logprobs":null}],"usage":{"prompt_tokens":42,"completion_tokens":2,"total_tokens":44,"cost":0,"is_byok":false,"prompt_tokens_details":{"cached_tokens":0,"audio_tokens":0},"cost_details":{"upstream_inference_cost":null,"upstream_inference_prompt_cost":0,"upstream_inference_completions_cost":0},"completion_tokens_details":{"reasoning_tokens":0,"image_tokens":0}}}"#,
        ];
        for a in arr {
            let ccr = ChatCompletionsResponse::from_json(a).unwrap();
            assert_eq!(ccr.provider.as_deref(), Some("Meta"));
            assert_eq!(
                ccr.model.as_deref(),
                Some("meta-llama/llama-3.3-8b-instruct:free")
            );
            assert_eq!(ccr.choices.len(), 1);
        }
    }

    #[test]
    fn test_chat_completions_response_more() {
        let arr = [
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"Rea","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":"l","reasoning":null,"reasoning_details":[]},"finish_reason":null,"native_finish_reason":null,"logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":" Madrid, 14 times.","reasoning":null,"reasoning_details":[]},"finish_reason":"stop","native_finish_reason":"stop","logprobs":null}]}"#,
            r#"{"id":"gen-1756749262-liysSWPMM37eb25U5gXO","provider":"WandB","model":"deepseek/deepseek-chat-v3.1","object":"chat.completion.chunk","created":1756749262,"choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null,"native_finish_reason":null,"logprobs":null}],"usage":{"prompt_tokens":33,"completion_tokens":8,"total_tokens":41,"cost":0.0000310365,"is_byok":false,"prompt_tokens_details":{"cached_tokens":0,"audio_tokens":0},"cost_details":{"upstream_inference_cost":null,"upstream_inference_prompt_cost":0.00001815,"upstream_inference_completions_cost":0.0000132},"completion_tokens_details":{"reasoning_tokens":0,"image_tokens":0}}}"#,
        ];
        for a in arr {
            let ccr = ChatCompletionsResponse::from_json(a).unwrap();
            assert_eq!(ccr.provider.as_deref(), Some("WandB"));
            assert_eq!(ccr.model.as_deref(), Some("deepseek/deepseek-chat-v3.1"));
            assert_eq!(ccr.choices.len(), 1);
        }
    }

    // Various null fields, including inside the message, and usage.
    #[test]
    fn test_nvidia_misc() {
        let s = r#"{"id":"8f20d6699e194a0abed38c671384d32d","object":"chat.completion.chunk","created":1770582573,"model":"qwen/qwen3-next-80b-a3b-instruct","choices":[{"index":0,"delta":{"role":null,"content":"Ta","reasoning_content":null,"tool_calls":null},"logprobs":null,"finish_reason":null,"matched_stop":null}],"usage":null}"#;
        let ccr = ChatCompletionsResponse::from_json(s).unwrap();
        assert_eq!(ccr.choices[0].delta.text(), Some("Ta"));
    }

    #[test]
    fn message_content_array() {
        let s = r#"{"role":"user","content":[{"type":"text","text":"Hello"},{"type":"text","text":" there"}]}"#;
        let msg = Message::from_json(s).unwrap();
        assert_eq!(msg.content.len(), 2);
        assert_eq!(msg.content[0].text(), Some("Hello"));
        assert_eq!(msg.content[1].text(), Some(" there"));
    }

    #[test]
    fn parse_bash_command_null_bytes() {
        let mut json = r#"{"command":"apply_patch <<'PATCH'\n*** Begin Patch\n*** Update File: CODE_OF_CONDUCT.md\n@@\n The community values respectful and constructive communication at all times.\n+\n+We encourage empathy: strive to understand others' perspectives and experiences, and respond with kindness and consideration.\n*** End Patch\nPATCH"}"#.to_string();
        for b in unsafe { json.as_bytes_mut().iter_mut() } {
            if *b == b'@' {
                *b = 0;
            }
        }

        let mut fields = [JsonField::new_string("command")];
        autoparser(&json, &mut fields).unwrap();
        let cmd = fields[0].get_string().expect("Missing 'command' field");
        assert!(cmd.contains("empathy"));
    }
}
