//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::ffi::CString;
use std::io::{self};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::thread;
use std::time::{Instant, SystemTime};
use std::{fs, path::PathBuf};

use ort_openrouter_core::Context as _;

use crate::Queue;
use crate::build_body;
use crate::ort_error;
use crate::tmux_pane_id;
use crate::writer::{self, IoFmtWriter};
use crate::{CancelToken, http};
use crate::{ChatCompletionsResponse, Settings};
use crate::{CollectedWriter, ConsoleWriter, FileWriter};
use crate::{LastData, OrtError, cache_dir, ort_err, ort_from_err, path_exists, read_to_string};
use crate::{Message, PromptOpts};
use crate::{OrtResult, Stats};
use crate::{Response, ThinkEvent};

pub fn run(
    api_key: &str,
    cancel_token: CancelToken,
    settings: Settings,
    opts: PromptOpts,
    messages: Vec<crate::Message>,
    is_pipe_output: bool, // Are we redirecting stdout?
    w: impl io::Write + Send,
) -> OrtResult<()> {
    // The readers must never fall further behind than this many responses
    const RESPONSE_BUF_LEN: usize = 256;

    let show_reasoning = opts.show_reasoning.unwrap();
    let is_quiet = opts.quiet.unwrap_or_default();
    //let model_name = opts.common.model.clone().unwrap();

    // Start network connection before almost anything else, this takes time
    let queue = start_prompt_thread::<RESPONSE_BUF_LEN>(
        api_key,
        cancel_token,
        settings.dns,
        opts.clone(),
        messages.clone(),
        0,
    );
    std::thread::yield_now();

    let consumer_stdout = queue.consumer();
    let consumer_last = queue.consumer();

    //let cache_dir = config::cache_dir()?;
    //let path = cache_dir.join(format!("{}.txt", slug(&model_name)));
    //let path_display = path.display().to_string();

    // Convert std::io::Write (Stdout) into core::fmt::Write for no_std
    let w_core = IoFmtWriter::new(w);

    let scope_err = thread::scope(|scope| {
        let mut handles = vec![];
        let jh_stdout = scope.spawn(move || -> OrtResult<()> {
            let (stats, w_core) = if is_pipe_output {
                let mut fw = FileWriter {
                    writer: w_core,
                    show_reasoning,
                };
                let stats = fw.run(consumer_stdout)?;
                let w_core = fw.into_inner();
                (stats, w_core)
            } else {
                let mut cw = ConsoleWriter {
                    writer: w_core,
                    show_reasoning,
                };
                let stats = cw.run(consumer_stdout)?;
                let w_core = cw.into_inner();
                (stats, w_core)
            };
            let mut w = w_core.into_inner();
            let _ = writeln!(w);
            if !is_quiet {
                //if settings.save_to_file {
                //    let _ = write!(handle, "\nStats: {stats}. Saved to {path_display}\n");
                //} else {
                let _ = write!(w, "\nStats: {stats}\n");
                //}
            }

            Ok(())
        });
        handles.push(jh_stdout);

        if settings.save_to_file {
            /*
            let jh_file = thread::spawn(move || -> OrtResult<()> {
                let f = File::create(&path)?;
                let mut file_writer = FileWriter {
                    writer: Box::new(f),
                    show_reasoning,
                };
                let stats = file_writer.run(rx_file)?;
                let f = file_writer.inner();
                let _ = writeln!(f);
                if !is_quiet {
                    let _ = write!(f, "\nStats: {stats}\n");
                }
                Ok(())
            });
            handles.push(jh_file);
            */

            let jh_last = scope.spawn(move || -> OrtResult<()> {
                let mut last_writer = writer::LastWriter::new(opts, messages)?;
                last_writer.run(consumer_last)?;
                Ok(())
            });
            handles.push(jh_last);
        }

        for h in handles {
            if let Err(err) = h.join().unwrap() {
                let mut oe = ort_error(err.to_string());
                oe.context("Internal thread error");
                // The errors are all the same so only print the first
                return Err(oe);
            }
        }
        Ok(())
    });
    scope_err?;

    Ok(())
}

/// The `-c` continue operation. Load the most recent conversation for this
/// pane to populate the context, then run with the new prompt.
pub fn run_continue(
    api_key: &str,
    cancel_token: CancelToken,
    settings: Settings,
    mut opts: crate::PromptOpts,
    is_pipe_output: bool,
    w: impl io::Write + Send,
) -> OrtResult<()> {
    let cache_dir = cache_dir()?;
    let mut last = cache_dir.clone();
    last.push('/');
    last.push_str(&format!("last-{}.json", tmux_pane_id()));
    let cs = CString::new(last.clone()).expect("Null bytes in config cache dir");
    let last_file = if !path_exists(cs.as_ref()) {
        last
    } else {
        most_recent(&cache_dir, "last-").context("most_recent")?
    };

    let mut last = match read_to_string(&last_file) {
        Ok(hist_str) => LastData::from_json(&hist_str)
            .map_err(|err| ort_error(format!("Failed to parse last: {err}")))?,
        Err("NOT FOUND") => {
            return ort_err("No last conversation, cannot continue");
        }
        Err(e) => {
            let mut err: OrtError = ort_from_err(e);
            err.context(last_file);
            return Err(err);
        }
    };

    opts.merge(last.opts);
    last.messages
        .push(crate::Message::user(opts.prompt.take().unwrap()));

    run(
        api_key,
        cancel_token,
        settings,
        opts,
        last.messages,
        is_pipe_output,
        w,
    )
}

pub fn run_multi(
    api_key: &str,
    cancel_token: CancelToken,
    settings: Settings,
    opts: PromptOpts,
    messages: Vec<crate::Message>,
    mut w: impl io::Write + Send,
) -> OrtResult<()> {
    // We'll likely never use this many, but making it the same as RESPONSE_BUF_LEN
    // makes for a smaller binary because generics.
    const MAX_MODELS: usize = 256;

    let num_models = opts.models.len();
    let _ = write!(w, "Calling {num_models} models...\r");
    let _ = w.flush();

    let _scope_err = thread::scope(|scope| {
        let queue_done = Queue::<String, MAX_MODELS>::new();
        let mut consumer_done = queue_done.consumer();
        let mut handles = Vec::with_capacity(num_models);

        // Start all the queries
        for idx in 0..num_models {
            let dns = settings.dns.clone();
            let opts_c = opts.clone();
            let messages_c = messages.clone();
            let qq = queue_done.clone();
            let handle = scope.spawn(move || -> OrtResult<()> {
                let model_name = opts_c.models.get(idx).unwrap().clone();
                let queue_single = start_prompt_thread::<MAX_MODELS>(
                    api_key,
                    cancel_token,
                    dns,
                    opts_c,
                    messages_c,
                    idx,
                );
                let mut mr = CollectedWriter {};
                let consumer_collected = queue_single.consumer();
                match mr.run(consumer_collected) {
                    Ok(full_output) => {
                        qq.add(full_output);
                    }
                    Err(err) => {
                        let err_str = err.to_string();
                        if err_str.contains("429 Too Many Requests") {
                            qq.add(format!("--- {model_name}: Overloaded ---"));
                        } else {
                            qq.add(format!("--- {model_name}: {err_str} ---"));
                        }
                    }
                }
                Ok(())
            });
            handles.push(handle);
        }

        for _ in 0..num_models {
            if cancel_token.is_cancelled() {
                break;
            }
            match consumer_done.get_next() {
                Some(model_output) => {
                    write!(w, "{}\n\n", model_output).map_err(ort_from_err)?;
                    let _ = w.flush();
                }
                None => {
                    // queue closed, which is impossible
                    break;
                }
            }
        }

        // All the threads should be done
        for h in handles {
            if cancel_token.is_cancelled() {
                break;
            }
            if let Err(err) = h.join().unwrap() {
                let mut oe = ort_error(err.to_string());
                oe.context("Internal thread error");
                // The errors are all the same so only print the first
                return Err(oe);
            }
        }
        Ok(())
    });

    Ok(())
}

/// Start prompt in a new thread. Returns almost immediately with a queue.
/// Streams the response to the queue.
pub fn start_prompt_thread<const N: usize>(
    api_key: &str,
    cancel_token: CancelToken,
    dns: Vec<String>,
    // Note we do not use the prompt from here, it should be in `messages` by now
    opts: PromptOpts,
    messages: Vec<Message>,
    // When running multiple models, this thread should use this one
    model_idx: usize,
) -> Arc<Queue<Response, N>> {
    let api_key = api_key.to_string();
    let queue = Queue::new();
    let queue_clone = queue.clone();

    std::thread::spawn(move || {
        let body = build_body(model_idx, &opts, &messages).unwrap(); // TODO unwrap
        let start = Instant::now();
        let mut reader = if dns.is_empty() {
            let addr = ("openrouter.ai", 443);
            http::chat_completions(&api_key, addr, &body).unwrap() // TODO unwrap
        } else {
            let addrs: Vec<_> = dns
                .into_iter()
                .map(|a| {
                    let ip_addr = a.parse::<Ipv4Addr>().unwrap();
                    SocketAddr::new(IpAddr::V4(ip_addr), 443)
                })
                .collect();
            http::chat_completions(&api_key, &addrs[..], &body).unwrap() // TODO unwrap
        };

        match http::skip_header(&mut reader) {
            Ok(true) => {
                // TODO it's transfer encoding chunked, this is the common case
                // which we don't handle correctly yet
            }
            Ok(false) => {
                // Not chunked encoding. I don't think we ever get here.
                // But the rest of this function assumes we do.
            }
            Err(err) => {
                queue.add(Response::Error(err.to_string()));
                return;
            }
        }

        let mut stats: Stats = Default::default();
        let mut token_stream_start = None;
        let mut num_tokens = 0;
        let mut is_start = true;
        let mut is_first_reasoning = true;
        let mut is_first_content = true;

        let mut line_buf = String::with_capacity(128);
        loop {
            if cancel_token.is_cancelled() {
                break;
            }

            line_buf.clear();
            match reader.read_line(&mut line_buf) {
                Ok(0) => {
                    // EOF
                    break;
                }
                Ok(_) => {
                    // success
                }
                Err(err) => {
                    queue.add(Response::Error(
                        "Stream read error: ".to_string() + &err.to_string(),
                    ));
                    break;
                }
            }
            let line = line_buf.trim();

            if is_start {
                // Very first message from server, usually
                // : OPENROUTER PROCESSING
                queue.add(Response::Start);
                is_start = false;
            }

            // SSE heartbeats and blank lines
            // TODO: and this skips chunked transfer encoding size lines
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
                            queue.add(Response::Think(ThinkEvent::Start));
                            is_first_reasoning = false;
                        }
                        let r_event =
                            Response::Think(ThinkEvent::Content(reasoning_content.to_string()));
                        queue.add(r_event);
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
                            queue.add(Response::Think(ThinkEvent::Stop));
                            is_first_content = false;
                        }
                        let r_event = Response::Content(content.to_string());
                        queue.add(r_event);
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

        if cancel_token.is_cancelled() {
            // Probaby user did Ctrl-C
            queue.add(Response::Error("Interrupted".to_string()));
        } else {
            // Clean finish, send stats
            let now = Instant::now();
            stats.elapsed_time = now - start;
            let stream_elapsed_time = now - token_stream_start.unwrap();
            stats.inter_token_latency_ms = stream_elapsed_time.as_millis() / num_tokens;

            queue.add(Response::Stats(stats));
        }
        queue.close();
    });

    queue_clone
}

/// Find the most recent file in `dir` that starts with `filename_prefix`.
/// Uses the minimal amount of disk access to go as fast as possible.
fn most_recent(dir: &str, filename_prefix: &str) -> OrtResult<String> {
    let mut most_recent_file: Option<(PathBuf, SystemTime)> = None;

    for entry in fs::read_dir(dir).map_err(ort_from_err)? {
        let entry = entry.map_err(ort_from_err)?;
        let path = entry.path();

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(filename_prefix) {
            continue;
        }
        // Only then get metadata (1 disk read)
        let metadata = entry.metadata().map_err(ort_from_err)?;
        if !metadata.is_file() {
            continue;
        }
        let modified_time = metadata.modified().map_err(ort_from_err)?;

        if let Some((_, prev_time)) = &most_recent_file {
            if modified_time > *prev_time {
                most_recent_file = Some((path, modified_time));
            }
        } else {
            most_recent_file = Some((path, modified_time));
        }
    }

    most_recent_file
        .map(|(path, _)| Ok(path.display().to_string()))
        .unwrap_or_else(|| {
            ort_err(format!(
                "No files found starting with prefix: {filename_prefix}"
            ))
        })
}
