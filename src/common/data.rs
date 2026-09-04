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

use crate::common::base64;
use crate::common::error::ort_err;
use crate::common::json_parser::{JsonField, Parser, autoparser};
use crate::common::utils::filename_read_to_bytes;
use crate::{Context as _, ErrorKind, OrtResult};

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
    pub error: Option<RemoteError>,
}

impl ChatCompletionsResponse {
    pub fn from_json(json: &str) -> OrtResult<Self> {
        let mut fields = [
            JsonField::new_simple_string("provider"),
            JsonField::new_simple_string("model"),
            JsonField::new_vec_raw("choices"),
            JsonField::new_raw("usage"),
            JsonField::new_raw("error"),
        ];
        autoparser(json, &mut fields).context("ChatCompletionsResponse autoparser")?;

        let mut choices = vec![];
        if let Some(v) = fields[2].get_vec_raw() {
            for c in v {
                choices.push(Choice::from_json(&c).context("Choice")?);
            }
        }

        let usage = fields[3]
            .get_raw()
            .as_deref()
            .map(Usage::from_json)
            .transpose()?;

        let error = fields[4]
            .get_raw()
            .as_deref()
            .map(RemoteError::from_json)
            .transpose()?;

        Ok(ChatCompletionsResponse {
            provider: fields[0].get_string(),
            model: fields[1].get_string(),
            choices,
            usage,
            error,
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

    pub fn from_json(json: &str) -> OrtResult<Self> {
        let mut fields = [
            JsonField::new_raw("delta"),
            JsonField::new_simple_string("finish_reason"),
        ];
        autoparser(json, &mut fields)?;
        let delta_json = fields[0].get_raw().expect("Missing delta in message");

        Ok(Choice {
            delta: Message::from_json(&delta_json).context("Message")?,
            finish_reason: fields[1].get_string(),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct ToolCall {
    pub index: u32,
    pub id: Option<String>,
    pub function: Function,
    // If we failed to parse the whole call (it arrives over multiple messages)
    // we must not attempt to run it, and notify the model of failure .
    pub has_error: bool,
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

    pub fn from_json(json: &str) -> OrtResult<Self> {
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
            has_error: false,
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
    pub fn from_json(json: &str) -> OrtResult<Self> {
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
    pub fn from_json(json: &str) -> OrtResult<Self> {
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

// {
//  "code":400,
//  "message":"Server tool request failed",
//  "metadata":{"error_type":"invalid_request"}
// }
pub struct RemoteError {
    code: u32,
    message: String,
}

impl RemoteError {
    pub fn from_json(json: &str) -> OrtResult<Self> {
        let mut fields = [
            JsonField::new_int("code"),
            JsonField::new_simple_string("message"),
        ];
        autoparser(json, &mut fields)?;
        Ok(RemoteError {
            code: fields[0].get_int().unwrap_or_default(),
            message: fields[1].get_string().unwrap_or_default(),
        })
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

pub struct LastData {
    pub messages: Vec<Message>,
    //pub tools: Vec<&'static Tool>,
}

impl LastData {
    pub fn from_json(json: &str) -> OrtResult<Self> {
        if json.is_empty() {
            return Err(ort_err(
                ErrorKind::FormatError,
                "Cannot continue, last-<$TMUX_PANE>.json file is empty. Usually that mains previous run failed.".into(),
            ));
        }

        let mut fields = [
            JsonField::new_vec_raw("messages"),
            //JsonField::new_vec_raw("tools"),
        ];
        autoparser(json, &mut fields)?;

        let mut messages = vec![];
        if let Some(msg_vec) = fields[0].get_vec_raw() {
            for m in msg_vec {
                messages.push(Message::from_json(&m)?);
            }
        }

        /*
        let mut tools = vec![];
        if let Some(tools_vec) = fields[1].get_vec_raw() {
            for t in tools_vec {
                tools.push(Tool::from_json(&t)?);
            }
        }
        */

        Ok(LastData { messages })
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
    pub annotations: Vec<Annotation>,
    pub reasoning: Option<String>,
    pub reasoning_details: Option<String>,
    /// For Role::Assistant requesting a tool call
    pub tool_calls: Vec<ToolCall>,
    /// For Role::Tool returning a result
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn new(role: Role, content: Option<String>, reasoning: Option<String>) -> Self {
        let content = content.map_or_else(Vec::new, |content| vec![Content::Text(content)]);
        Self::with_content(role, content, reasoning, None, vec![], None, vec![])
    }

    pub fn with_content(
        role: Role,
        content: Vec<Content>,
        reasoning: Option<String>,
        reasoning_details: Option<String>,
        tool_calls: Vec<ToolCall>,
        tool_call_id: Option<String>,
        annotations: Vec<Annotation>,
    ) -> Self {
        Message {
            role,
            content,
            reasoning,
            reasoning_details,
            tool_calls,
            tool_call_id,
            annotations,
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

    pub fn assistant_with_tool_call(
        content: String,
        tool_calls: Vec<ToolCall>,
        reasoning: Option<String>,
        reasoning_details: Option<String>,
    ) -> Self {
        Self::with_content(
            Role::Assistant,
            vec![Content::Text(content)],
            reasoning,
            reasoning_details,
            tool_calls,
            None,
            vec![],
        )
    }
    pub fn tool(id: String, content: String) -> Self {
        Self::with_content(
            Role::Tool,
            vec![Content::Text(content)],
            None,
            None,
            vec![],
            Some(id),
            vec![],
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
                let pf = PromptFile::load(f).map_err(|err| {
                    ort_err(
                        ErrorKind::ReadingPromptFile,
                        (f.to_string() + " - " + err).into(),
                    )
                })?;
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

    pub fn from_json(json: &str) -> OrtResult<Self> {
        let mut fields = [
            JsonField::new_simple_string("role"),
            JsonField::new_raw("content"),
            JsonField::new_string("reasoning"),
            JsonField::new_string("reasoning_content"),
            JsonField::new_vec_raw("tool_calls"),
            JsonField::new_vec_raw("annotations"),
            JsonField::new_raw("reasoning_details"),
        ];
        autoparser(json, &mut fields)?;

        let role = fields[0]
            .get_raw()
            .as_deref()
            .map(|role| {
                Role::from_str(role).map_err(|err| ort_err(ErrorKind::FormatError, err.into()))
            })
            .transpose()?;
        let reasoning = fields[2].get_string().or_else(|| fields[3].get_string());

        let mut tool_calls = vec![];
        if let Some(tool_calls_str_vec) = fields[4].get_vec_raw() {
            for t in tool_calls_str_vec {
                tool_calls.push(ToolCall::from_json(&t)?);
            }
        }

        let mut annotations = vec![];
        if let Some(v) = fields[5].get_vec_raw() {
            for a in v {
                annotations.push(Annotation::from_json(&a)?);
            }
        }

        // Must send whole block back unchanged, so save the unparsed JSON
        let reasoning_details = fields[6].get_raw();

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
                        content.push(Content::from_json(j).context("Content")?);
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
            reasoning_details,
            tool_calls,
            None,
            annotations,
        ))
    }
}

/// These are url_citation. Example:
/// {"type":"url_citation","url_citation":{"url":"https://linkerd.io/docs/reference/load-balancing/","title":"Load Balancing | Linkerd","start_index":0,"end_index":0,"content":"Linkerd uses a sophisticated .."}}
#[derive(Debug, Clone)]
pub enum Annotation {
    UrlCitation { url: String, content: String },
}

impl Annotation {
    /// Convenience function given there's only one type
    pub fn citation_url(&self) -> &str {
        match self {
            Annotation::UrlCitation { url, .. } => url.as_ref(),
        }
    }
}

impl Annotation {
    pub fn from_json(json: &str) -> OrtResult<Self> {
        let mut fields = [
            JsonField::new_simple_string("type"),
            JsonField::new_raw("url_citation"),
        ];
        autoparser(json, &mut fields)?;

        let annotation_type = fields[0].get_string().unwrap_or_default();
        if annotation_type != "url_citation" {
            return Err(ort_err(
                ErrorKind::FormatError,
                Cow::from("Unknown annotation type: ".to_string() + &annotation_type),
            ));
        }

        let citation_body = fields[1].get_raw().unwrap_or_default();
        let mut citation_fields = [
            JsonField::new_simple_string("url"),
            JsonField::new_string("content"),
        ];
        autoparser(&citation_body, &mut citation_fields)?;

        Ok(Annotation::UrlCitation {
            url: citation_fields[0].get_string().unwrap_or_default(),
            content: citation_fields[1].get_string().unwrap_or_default(),
        })
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

    pub fn is_empty(&self) -> bool {
        self.len() == 0
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

    pub fn from_json(json: &str) -> OrtResult<Self> {
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
            Some("text") => {
                Ok(Content::Text(text.ok_or_else(|| {
                    ort_err(ErrorKind::FormatError, "missing text".into())
                })?))
            }
            Some("image_url") => {
                if let Some(image_url) = image_url {
                    Ok(Content::ImageUrl(image_url.to_string()))
                } else {
                    Ok(Content::Image {
                        base64: base64_data.ok_or_else(|| {
                            ort_err(ErrorKind::FormatError, "missing image_url".into())
                        })?,
                        mime_type: mime_type.unwrap(),
                    })
                }
            }
            Some("file") => {
                Ok(Content::File(file.ok_or_else(|| {
                    ort_err(ErrorKind::FormatError, "missing file".into())
                })?))
            }
            Some(other) => Err(ort_err(
                ErrorKind::FormatError,
                ("unsupported content type: ".to_string() + other).into(),
            )),
            None => Err(ort_err(
                ErrorKind::FormatError,
                "missing content type".into(),
            )),
        }
    }
}

/// Returns (base64_data, mime_type)
fn parse_image_url(json: &str) -> OrtResult<(String, &'static str)> {
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
        Err(ort_err(
            ErrorKind::FormatError,
            "Invalid mime type in saved image_url".into(),
        ))
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

#[derive(Clone, Debug)]
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
    /// A url_citation from web_search tool, or maybe other things
    Annotation(Annotation),
    /// Summary stats at the end of the run
    Stats(super::stats::Stats),
    /// Survivable error. Model tried to use a tool that does not exist.
    Warn(String),
    /// Fatal error. Often you mistyped the model name. Or 429 rate limit.
    Error(String),
    /// For agent mode, user prompt
    Prompt(String),
    /// We couldn't parse this, could be thinking or content. Writer should display
    /// a character to indicate the gap.
    Missing,
}

#[derive(Debug, Clone)]
pub enum ThinkEvent {
    Start,
    Content(String),
    // reasoning_details, which is encrypted, summary, or text. Only encrypted matters, the rest
    // repeats Content.
    Details(String),
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

    pub fn from_json(json: &str) -> OrtResult<Self> {
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
            filename.ok_or_else(|| ort_err(ErrorKind::FormatError, "missing filename".into()))?,
            base64.ok_or_else(|| ort_err(ErrorKind::FormatError, "missing file_data".into()))?,
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
/*
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
            let msg = "Tool not found: ".to_string() + &name;
            return Err(msg.into());
        };
        Ok(t)
    }
}
*/

#[derive(Clone)]
pub struct ToolParameter {
    pub name: &'static str,
    pub param_type: &'static str,
    pub description: &'static str,
}

/// Info AgentWriter needs to display a tool call.
#[derive(Clone, Debug)]
pub struct ToolDisplay {
    // Capitalized and with a space at the end please
    pub name: &'static str,
    pub arguments: String,
    pub extra: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LastData;

    #[test]
    fn last_data() {
        let s = r#"
{"messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hello there! 😊How can I help you today? I'm ready for anything – questions, stories, ideas, or just a friendly chat!Let me know what's on your mind. ✨"}]}
"#;
        let l = LastData::from_json(s).unwrap();
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
    fn message_reasoning_field_aliases() {
        let msg =
            Message::from_json(r#"{"role":"assistant","reasoning_content":"from alias"}"#).unwrap();
        assert_eq!(msg.reasoning.as_deref(), Some("from alias"));

        let msg = Message::from_json(
            r#"{"role":"assistant","reasoning":"preferred","reasoning_content":"alias"}"#,
        )
        .unwrap();
        assert_eq!(msg.reasoning.as_deref(), Some("preferred"));
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

    #[test]
    fn test_parse_chat_error() {
        let json = r#"{"id":"gen-1787255401-CBB2yYetBs5rQ1TxQ3zc","object":"chat.completion.chunk","created":1787255401,"model":"openai/gpt-5.6-terra","provider":"OpenAI","choices":[],"error":{"code":400,"message":"Server tool request failed","metadata":{"error_type":"invalid_request"}}}"#;
        let ccr = ChatCompletionsResponse::from_json(json).unwrap();
        assert_eq!(ccr.error.unwrap().message, "Server tool request failed");
    }

    #[test]
    fn test_preserves_reasoning_details() {
        let json = r#"{"id":"gen-1788467537-QS8BVdZN4vsO5mtNhoN1","object":"chat.completion.chunk","created":1788467537,"model":"openai/gpt-5.6-sol","provider":"OpenAI","choices":[{"index":0,"delta":{"content":"","role":"assistant","reasoning":null,"reasoning_details":[{"type":"reasoning.encrypted","data":"gAAAAABqJ9","format":"openai-responses-v1","id":"rs_05a6389df207c51e016a99d959ecd887d1b672d700b322a752","index":0}]},"finish_reason":null,"native_finish_reason":null}]}"#;
        let ccr = ChatCompletionsResponse::from_json(json).unwrap();
        let Some(ref rd) = ccr.choices.first().unwrap().delta.reasoning_details else {
            panic!("Missing reasoning_details");
        };
        assert!(rd.contains("reasoning.encrypted"));
    }
}
