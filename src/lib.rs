//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::fmt;
use std::io::{BufRead, BufReader};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

pub const DEFAULT_MODEL: &str = "openai/gpt-oss-20b:free";
const DEFAULT_QUIET: bool = false;
const DEFAULT_SHOW_REASONING: bool = false;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct PromptOpts {
    pub prompt: Option<String>,
    pub model: Option<String>,
    pub system: Option<String>,
    pub priority: Option<String>,
    /// Don't show stats after request
    pub quiet: Option<bool>,
    /// Reasoning config
    pub reasoning: Option<ReasoningConfig>,
    /// Show reasoning output
    pub show_reasoning: Option<bool>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningConfig {
    pub enabled: bool,
    pub effort: Option<ReasoningEffort>,
    pub tokens: Option<u32>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
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

impl PromptOpts {
    // Replace any blank or None fields on Self with values from other
    // or with the defaults.
    // After this call a PromptOpts is ready to use.
    pub fn merge(&mut self, o: PromptOpts) {
        self.prompt.get_or_insert(o.prompt.unwrap_or_default());
        self.model
            .get_or_insert(o.model.unwrap_or_else(|| DEFAULT_MODEL.to_string()));
        if o.system.is_some() {
            self.system.get_or_insert(o.system.unwrap());
        }
        if o.priority.is_some() {
            self.priority.get_or_insert(o.priority.unwrap());
        }
        self.quiet.get_or_insert(o.quiet.unwrap_or(DEFAULT_QUIET));
        self.reasoning
            .get_or_insert(o.reasoning.unwrap_or_default());
        self.show_reasoning
            .get_or_insert(o.show_reasoning.unwrap_or(DEFAULT_SHOW_REASONING));
    }
}

#[derive(Debug, Clone)]
pub enum Response {
    /// The first time we get anything at all on the SSE stream
    Start,
    /// Reasoning events - start, some thoughts, stop
    Think(ThinkEvent),
    /// The good stuff
    Content(String),
    /// Summary stats at the end of the run
    Stats(Stats),
    /// Less good things. Often you mistyped the model name.
    Error(String),
}

#[derive(Debug, Clone)]
pub enum ThinkEvent {
    Start,
    Content(String),
    Stop,
}

#[derive(Default, Debug, Clone)]
pub struct Stats {
    used_model: String,
    provider: String,
    cost_in_cents: f64, // Divide by 100 for $
    elapsed_time: Duration,
    time_to_first_token: Option<Duration>,
    inter_token_latency_ms: u128,
}

impl Stats {
    pub fn provider(&self) -> &str {
        &self.provider
    }
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}. {:.4} cents. {} ({} TTFT, {}ms ITL)",
            self.used_model,
            self.provider,
            self.cost_in_cents,
            format_duration(self.elapsed_time),
            format_duration(self.time_to_first_token.unwrap_or_default()),
            self.inter_token_latency_ms,
        )
    }
}

pub fn list_models(api_key: &str) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut req = ureq::get(MODELS_URL);
    req = req.header("Authorization", &format!("Bearer {api_key}",));
    let mut resp = req.call()?;
    let body = resp.body_mut().read_to_string()?;
    let mut doc: serde_json::Value = serde_json::from_str(&body)?;
    let models: Vec<serde_json::Value> = doc["data"].take().as_array().unwrap().to_vec();
    Ok(models)
}

/// Start prompt in a new thread. Returns almost immediately with a channel. Streams the response to the channel.
pub fn prompt(api_key: &str, opts: PromptOpts) -> anyhow::Result<mpsc::Receiver<Response>> {
    let (tx, rx) = mpsc::channel();
    let api_key = api_key.to_string();

    std::thread::spawn(move || {
        let body = build_body(
            &opts.model.unwrap(),
            &opts.prompt.unwrap(),
            opts.system.as_deref(),
            opts.reasoning,
            opts.priority.as_deref(),
        );

        //
        // TODO: Remember the IP address from last time (in config probably)
        // and do .with_parts and pass a Resolver - skip DNS!
        //
        // TODO2: Make a TlsConfig and call disable_verification for speed
        //

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(5)))
            .timeout_recv_response(None)
            .https_only(true)
            .user_agent("ort/0.1.0")
            .http_status_as_error(false)
            .build()
            .into();

        let req = agent
            .post(API_URL)
            .header("Authorization", &format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream");

        let start = Instant::now();
        let mut resp = match req.send(&body) {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(Response::Error(format!("Request error: {e}")));
                return;
            }
        };

        if resp.status() != 200 {
            let status = resp.status();
            let body = resp
                .body_mut()
                .read_to_string()
                .unwrap_or_else(|_| "<failed to read body>".to_string());
            let _ = tx.send(Response::Error(format!("HTTP error {status}: {body}")));
            return;
        }

        let body = resp.body_mut();
        let reader = BufReader::new(body.as_reader());

        let mut stats: Stats = Default::default();
        let mut token_stream_start = None;
        let mut num_tokens = 0;
        let mut is_start = true;
        let mut is_first_reasoning = true;
        let mut is_first_content = true;

        for line_res in reader.lines() {
            let line = match line_res {
                Ok(l) => l,
                Err(e) => {
                    let _ = tx.send(Response::Error(format!("Stream read error {e}")));
                    return;
                }
            };
            //eprintln!("{line}");

            if is_start {
                // Very first message from server, usually
                // : OPENROUTER PROCESSING
                let _ = tx.send(Response::Start);
                is_start = false;
            }

            // SSE heartbeats and blank lines
            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    break;
                }

                // Each data: line is a JSON chunk in OpenAI streaming format
                match serde_json::from_str::<serde_json::Value>(data) {
                    Ok(v) => {
                        // Standard OpenAI stream delta shape
                        let Some(delta) = v
                            .get("choices")
                            .and_then(|c| c.get(0))
                            .and_then(|c0| c0.get("delta"))
                        else {
                            continue;
                        };

                        let maybe_reasoning = delta.get("reasoning").and_then(|c| c.as_str());
                        let maybe_content = delta.get("content").and_then(|c| c.as_str());

                        let has_reasoning = maybe_reasoning.map(|x| !x.is_empty()).unwrap_or(false);
                        let has_content = maybe_content.map(|x| !x.is_empty()).unwrap_or(false);
                        let has_usage = v.get("usage").is_some();

                        if !(has_reasoning || has_content || has_usage) {
                            continue;
                        }

                        // Record time to first token
                        if stats.time_to_first_token.is_none() {
                            stats.time_to_first_token = Some(Instant::now() - start);
                            token_stream_start = Some(Instant::now());
                        }

                        // Handle reasoning content
                        if let Some(reasoning_content) = maybe_reasoning
                            && !reasoning_content.is_empty()
                        {
                            num_tokens += 1;
                            if is_first_reasoning {
                                if reasoning_content.trim().is_empty() {
                                    // Don't allow starting with carriage return or blank space, that messes up the display
                                    continue;
                                }
                                let _ = tx.send(Response::Think(ThinkEvent::Start));
                                is_first_reasoning = false;
                            }
                            let _ = tx.send(Response::Think(ThinkEvent::Content(
                                reasoning_content.to_string(),
                            )));
                        }

                        // Handle regular content
                        if let Some(content) = maybe_content
                            && !content.is_empty()
                        {
                            num_tokens += 1;
                            if is_first_content && content.trim().is_empty() {
                                // Don't allow starting with carriage return or blank space, that messes up the display
                                continue;
                            }
                            // If we signaled the open (!is_first_reasoning)
                            // and we signaled the close yet (is_first_reasoning),
                            // signal the close.
                            if !is_first_reasoning && is_first_content {
                                let _ = tx.send(Response::Think(ThinkEvent::Stop));
                                is_first_content = false;
                            }
                            let _ = tx.send(Response::Content(content.to_string()));
                        }

                        // Handle last message which contains the "usage" key
                        if let Some(usage) = v.get("usage") {
                            if let Some(c) = usage.get("cost") {
                                stats.cost_in_cents = c.as_f64().unwrap() * 100.0; // convert to cents
                            }
                            stats.provider = v
                                .get("provider")
                                .map(|p| p.as_str().unwrap().to_string())
                                .unwrap();
                            stats.used_model = v
                                .get("model")
                                .map(|m| m.as_str().unwrap().to_string())
                                .unwrap();
                        }
                    }
                    Err(_err) => {
                        // Ignore malformed server-sent diagnostics; keep streaming
                    }
                }
            }
        }
        let now = Instant::now();
        stats.elapsed_time = now - start;
        let stream_elapsed_time = now - token_stream_start.unwrap();
        stats.inter_token_latency_ms = stream_elapsed_time.as_millis() / num_tokens;

        let _ = tx.send(Response::Stats(stats));
    });

    Ok(rx)
}

// Format the Duration as minutes, seconds and milliseconds.
// examples: 3m12s, 5s, 400ms, 12m, 4s
fn format_duration(d: Duration) -> String {
    let total_millis = d.as_millis();
    let minutes = total_millis / 60_000;
    let seconds = (total_millis % 60_000) / 1_000;
    let milliseconds = total_millis % 1_000;

    let mut result = String::new();

    if minutes > 0 {
        result.push_str(&format!("{minutes}m"));
    }

    if seconds > 0 {
        if seconds <= 2 {
            result.push_str(&format!(
                "{seconds}.{}s",
                (milliseconds as f64 / 100.0) as u32
            ));
        } else {
            result.push_str(&format!("{seconds}s"));
        }
    }

    if milliseconds > 0 && minutes == 0 && seconds == 0 {
        result.push_str(&format!("{milliseconds}ms"));
    }

    // Handle the case where duration is 0
    if result.is_empty() {
        result = "0ms".to_string();
    }

    result
}

fn build_body(
    model: &str,
    prompt: &str,
    system_prompt: Option<&str>,
    reasoning: Option<ReasoningConfig>,
    priority: Option<&str>,
) -> String {
    // Build messages array
    let mut messages = Vec::new();
    if let Some(sys) = system_prompt {
        messages.push(serde_json::json!({ "role": "system", "content": sys }));
    }
    messages.push(serde_json::json!({ "role": "user", "content": prompt }));

    // Base payload with streaming enabled
    let mut obj = serde_json::json!({
        "model": model,
        "stream": true,
        "usage": {"include": true},
        "messages": messages,
    });

    // Optional provider.sort
    if let Some(p) = priority {
        obj.as_object_mut()
            .unwrap()
            .insert("provider".into(), serde_json::json!({ "sort": p }));
    }
    let open_router_json = match reasoning {
        // No -r and nothing in config file
        None => serde_json::json!({"enabled": false}),
        // cli "-r off" or config file '"enabled": false'
        Some(r_cfg) if !r_cfg.enabled => serde_json::json!({"enabled": false}),
        // Reasoning on
        Some(r_cfg) => match (r_cfg.effort, r_cfg.tokens) {
            (Some(effort), _) => {
                serde_json::json!({"effort": effort.to_string(), "exclude": false, "enabled": true})
            }
            (_, Some(tokens)) => {
                serde_json::json!({"max_tokens": tokens, "exclude": false, "enabled": true})
            }
            _ => unreachable!("Reasoning effort and tokens cannot both be null"),
        },
    };
    obj.as_object_mut()
        .unwrap()
        .insert("reasoning".into(), open_router_json);
    serde_json::to_string(&obj).expect("JSON serialization failed")
}
