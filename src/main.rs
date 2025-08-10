use std::env;
use std::io::{self, BufRead, BufReader, Write};
use std::process::ExitCode;
use std::time::Duration;

const DEFAULT_MODEL: &str = "openrouter/auto:price";
const API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

#[derive(Debug)]
struct Opts {
    model: String,
    system: Option<String>,
    priority: Option<String>,
    prompt: String,
}

fn print_usage_and_exit() -> ! {
    eprintln!(
        "Usage: ort [-m <model>] [-s \"<system prompt>\"] [-p <price|throughput|latency>] <prompt>\n\
         Defaults: -m {} ; -s omitted ; -p omitted\n\
         Example:\n  ort -p price -m moonshotai/kimi-k2 -s \"Respond like a pirate\" \"Write a limerick about AI\"",
        DEFAULT_MODEL
    );
    std::process::exit(2);
}

fn parse_args() -> Opts {
    let args: Vec<String> = env::args().collect();
    // args[0] is program name
    if args.len() == 1 {
        print_usage_and_exit();
    }

    // Simple, fast, no external parser
    let mut model = DEFAULT_MODEL.to_string();
    let mut system: Option<String> = None;
    let mut priority: Option<String> = None;
    let mut i = 1usize;
    let mut prompt_parts: Vec<String> = Vec::new();

    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => print_usage_and_exit(),
            "-m" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Missing value for -m");
                    print_usage_and_exit();
                }
                model = args[i].clone();
                i += 1;
            }
            "-s" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Missing value for -s");
                    print_usage_and_exit();
                }
                system = Some(args[i].clone());
                i += 1;
            }
            "-p" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Missing value for -p");
                    print_usage_and_exit();
                }
                let val = args[i].clone();
                match val.as_str() {
                    "price" | "throughput" | "latency" => priority = Some(val),
                    _ => {
                        eprintln!("Invalid -p value: must be one of price|throughput|latency");
                        print_usage_and_exit();
                    }
                }
                i += 1;
            }
            s if s.starts_with('-') => {
                eprintln!("Unknown flag: {}", s);
                print_usage_and_exit();
            }
            _ => {
                // First positional marks the start of the prompt; join the rest verbatim
                prompt_parts.extend_from_slice(&args[i..]);
                break;
            }
        }
    }

    if prompt_parts.is_empty() {
        eprintln!("Missing prompt.");
        print_usage_and_exit();
    }

    Opts {
        model,
        system,
        priority,
        prompt: prompt_parts.join(" "),
    }
}

fn build_body(cfg: &Opts) -> String {
    // Build messages array
    let mut messages = Vec::new();
    if let Some(sys) = &cfg.system {
        messages.push(serde_json::json!({ "role": "system", "content": sys }));
    }
    messages.push(serde_json::json!({ "role": "user", "content": cfg.prompt }));

    // Base payload with streaming enabled
    let mut obj = serde_json::json!({
        "model": cfg.model,
        "stream": true,
        "messages": messages
    });

    // Optional provider.sort
    if let Some(p) = &cfg.priority {
        obj.as_object_mut()
            .unwrap()
            .insert("provider".into(), serde_json::json!({ "sort": p }));
    }

    serde_json::to_string(&obj).expect("JSON serialization failed")
}

fn main() -> ExitCode {
    // Fail fast if key missing
    let api_key = match env::var("OPENROUTER_API_KEY") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("OPENROUTER_API_KEY is not set.");
            std::process::exit(1);
        }
    };

    let opts = parse_args();
    let body = build_body(&opts);

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_recv_response(None)
        .build()
        .into();

    let req = agent
        .post(API_URL)
        .header("Authorization", &format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream");

    let mut resp = match req.send(&body) {
        Ok(r) => r,
        Err(ureq::Error::StatusCode(code)) => {
            eprintln!("HTTP error {code}");
            return ExitCode::from(1);
        }
        Err(e) => {
            eprintln!("Request error: {e}");
            return ExitCode::from(1);
        }
    };

    if resp.status() != 200 {
        let status = resp.status();
        let body = resp
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|_| "<failed to read body>".to_string());
        eprintln!("HTTP error {}: {}", status, body);
        std::process::exit(1);
    }

    // Stream SSE lines and print content deltas as they arrive
    let body = resp.body_mut();
    let reader = BufReader::new(body.as_reader());
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    for line_res in reader.lines() {
        let line = match line_res {
            Ok(l) => l,
            Err(e) => {
                let _ = writeln!(io::stderr(), "Stream read error: {}", e);
                break;
            }
        };

        // SSE heartbeats and blank lines
        if line.is_empty() || line.starts_with(':') {
            continue;
        }

        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                // Finish with a newline if last chunk didn't include one
                let _ = handle.flush();
                break;
            }

            // Each data: line is a JSON chunk in OpenAI streaming format
            match serde_json::from_str::<serde_json::Value>(data) {
                Ok(v) => {
                    // Standard OpenAI stream delta shape
                    if let Some(content) = v
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c0| c0.get("delta"))
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        // Write chunk and flush to keep it live
                        let _ = write!(handle, "{}", content);
                        let _ = handle.flush();
                    }
                }
                Err(_e) => {
                    // Ignore malformed server-sent diagnostics; keep streaming
                }
            }
        }
    }
    println!();
    ExitCode::SUCCESS
}
