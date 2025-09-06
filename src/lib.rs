//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::fmt;
use std::io::BufRead;
use std::net::Ipv4Addr;
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub mod config;
mod data;
mod net;
pub mod utils;
pub use data::{
    ChatCompletionsResponse, Choice, DEFAULT_MODEL, LastData, Message, Priority, PromptOpts,
    ReasoningConfig, ReasoningEffort, Role, Usage,
};
pub mod parser;
pub mod serializer;
pub mod writer;

const MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

const EXPECTED_HTTP_200: &str = "HTTP/1.1 200 OK";

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

/// Returns raw JSON
pub fn list_models(api_key: &str) -> anyhow::Result<String> {
    let mut req = ureq::get(MODELS_URL);
    req = req.header("Authorization", &format!("Bearer {api_key}",));
    let mut resp = req.call()?;
    let body = resp.body_mut().read_to_string()?;
    Ok(body)
}

/// Start prompt in a new thread. Returns almost immediately with a channel. Streams the response to the channel.
pub fn prompt(
    api_key: &str,
    _verify_certs: bool,
    dns: Vec<String>,
    // Note we do not use the prompt from here, it should be in `messages` by now
    opts: PromptOpts,
    messages: Vec<Message>,
) -> anyhow::Result<mpsc::Receiver<Response>> {
    let (tx, rx) = mpsc::channel();
    let api_key = api_key.to_string();

    std::thread::spawn(move || {
        let body = serializer::build_body(&opts, &messages).unwrap(); // TODO
        let start = Instant::now();
        let reader = if dns.is_empty() {
            let addr = ("openrouter.ai", 443);
            net::chat_completions(&api_key, addr, &body).unwrap() // TODO unwrap
        } else {
            let addr = (dns[0].parse::<Ipv4Addr>().unwrap(), 443);
            net::chat_completions(&api_key, addr, &body).unwrap() // TODO unwrap
        };
        let mut response_lines = reader.lines();

        let Some(Ok(status)) = response_lines.next() else {
            let _ = tx.send(Response::Error(
                "Request failed. Missing initial status line".to_string(),
            ));
            return;
        };
        if status.trim() != EXPECTED_HTTP_200 {
            // Usually the body explains the error so gather that.

            let mut response_lines = response_lines
                // Skip the rest of the headers
                .skip_while(|line| line.as_ref().map(|l| l.trim().len()).unwrap_or(0) > 0)
                // Then skip until the actual error
                .skip_while(|line| line.as_ref().map(|l| l.trim().len()).unwrap_or(0) < 5);
            match response_lines.next() {
                Some(Ok(err)) => {
                    // TODO parse JSON. It looks like this:
                    // {"error":{"message":"openai/gpt-oss-90b is not a valid model ID","code":400},"user_id":"user_30mJ0GpP57Kj9wLQ4mDCfMS5nk0"}
                    let _ = tx.send(Response::Error(format!("HTTP error {status}: {err}")));
                }
                _ => {
                    let _ = tx.send(Response::Error(format!("HTTP error {status}")));
                }
            }
            return;
        }

        let mut stats: Stats = Default::default();
        let mut token_stream_start = None;
        let mut num_tokens = 0;
        let mut is_start = true;
        let mut is_first_reasoning = true;
        let mut is_first_content = true;

        for line_res in response_lines {
            let line = match line_res {
                Ok(l) => l,
                Err(e) => {
                    let _ = tx.send(Response::Error(format!("Stream read error {e}")));
                    return;
                }
            };

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
            // Skip HTTP headers
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data == "[DONE]" {
                break;
            }

            // Each data: line is a JSON chunk in OpenAI streaming format
            match ChatCompletionsResponse::from_json(data) {
                Ok(mut v) => {
                    // Standard OpenAI stream delta shape
                    let Some(delta) = v.choices.pop().map(|c| c.delta) else {
                        continue;
                    };

                    let has_reasoning = delta
                        .reasoning
                        .as_ref()
                        .map(|x| !x.is_empty())
                        .unwrap_or(false);
                    let has_content = delta
                        .content
                        .as_ref()
                        .map(|x| !x.is_empty())
                        .unwrap_or(false);
                    let has_usage = v.usage.is_some();

                    if !(has_reasoning || has_content || has_usage) {
                        continue;
                    }

                    // Record time to first token
                    if stats.time_to_first_token.is_none() {
                        stats.time_to_first_token = Some(Instant::now() - start);
                        token_stream_start = Some(Instant::now());
                    }

                    // Handle reasoning content
                    if let Some(reasoning_content) = delta.reasoning.as_ref()
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
                    if let Some(content) = delta.content.as_ref()
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
                    if let Some(usage) = v.usage {
                        stats.cost_in_cents = usage.cost as f64 * 100.0; // convert to cents
                        stats.provider = v.provider.expect("Last message was missing provider");
                        stats.used_model = v.model.expect("Last message was missing mode");
                    }
                }
                Err(_err) => {
                    // Ignore malformed server-sent diagnostics; keep streaming
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
