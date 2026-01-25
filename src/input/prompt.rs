//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::cmp::max;
use core::ffi::c_void;
use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use core::ptr;

extern crate alloc;
use alloc::boxed::Box;
use alloc::ffi::CString;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

use crate::Context as _;

use crate::ChatCompletionsResponse;
use crate::OrtResult;
use crate::build_body;
use crate::common::config;
use crate::common::dir;
use crate::common::file;
use crate::common::io::Write;
use crate::common::queue;
use crate::common::resolver;
use crate::common::stats;
use crate::common::time;
use crate::common::utils;
use crate::libc;
use crate::ort_error;
use crate::output::writer::{CollectedWriter, ConsoleWriter, FileWriter, LastWriter};
use crate::{CancelToken, http, thread as ort_thread};
use crate::{ErrorKind, LastData};
use crate::{Message, PromptOpts};
use crate::{Response, ThinkEvent};

// The readers must never fall further behind than this many responses
const RESPONSE_BUF_LEN: usize = 256;

type ResponseQueue = queue::Queue<Response, RESPONSE_BUF_LEN>;
type ResponseConsumer = queue::Consumer<Response, RESPONSE_BUF_LEN>;

pub fn run<W: Write + Send>(
    api_key: &str,
    cancel_token: CancelToken,
    settings: config::Settings,
    opts: PromptOpts,
    messages: Vec<crate::Message>,
    is_pipe_output: bool, // Are we redirecting stdout?
    w_core: W,
) -> OrtResult<()> {
    let show_reasoning = opts.show_reasoning.unwrap();
    let is_quiet = opts.quiet.unwrap_or_default();
    //let model_name = opts.common.model.clone().unwrap();

    // Start network connection before almost anything else, this takes time
    let queue = start_prompt_thread(
        api_key,
        cancel_token,
        settings.dns,
        opts.clone(),
        messages.clone(),
        0,
    );

    let consumer_stdout = queue.consumer();
    let consumer_last = queue.consumer();

    let mut tids = vec![];
    let thread_params = Box::new(SingleRunnerThreadParams {
        w_core,
        consumer_stdout,
        is_pipe_output,
        show_reasoning,
        is_quiet,
    });
    unsafe {
        match ort_thread::spawn(
            single_runner_thread::<W>,
            Box::into_raw(thread_params) as *mut c_void,
        ) {
            Ok(tid) => {
                tids.push(tid);
            }
            Err(err) => {
                let s = CString::new(err.as_string()).unwrap();
                libc::printf(
                    c"Spawning single_runner_thread failed: %s".as_ptr(),
                    s.as_ptr(),
                );
            }
        }
    }

    if settings.save_to_file {
        let thread_params = Box::new(LastWriterThreadParams {
            opts,
            messages,
            consumer_last,
        });
        unsafe {
            match ort_thread::spawn(
                last_writer_thread,
                Box::into_raw(thread_params) as *mut c_void,
            ) {
                Ok(tid) => {
                    tids.push(tid);
                }
                Err(err) => {
                    let s = CString::new(err.as_string()).unwrap();
                    libc::printf(
                        c"Spawning last_writer_thread failed: %s".as_ptr(),
                        s.as_ptr(),
                    );
                }
            }
        }
    }

    for tid in tids {
        unsafe { libc::pthread_join(tid, ptr::null_mut()) };
    }

    Ok(())
}

/// The `-c` continue operation. Load the most recent conversation for this
/// pane to populate the context, then run with the new prompt.
pub fn run_continue(
    api_key: &str,
    cancel_token: CancelToken,
    settings: config::Settings,
    mut opts: crate::PromptOpts,
    is_pipe_output: bool,
    w: impl Write + Send,
) -> OrtResult<()> {
    let cache_dir = config::cache_dir()?;
    let mut last = cache_dir.clone();
    last.push('/');
    last.push_str(&utils::last_filename());
    let cs = CString::new(last.clone()).expect("Null bytes in config cache dir");
    let last_file = if utils::path_exists(cs.as_ref()) {
        last
    } else {
        most_recent(&cache_dir, "last-").context("most_recent")?
    };

    let mut last = match utils::filename_read_to_string(&last_file) {
        Ok(hist_str) => LastData::from_json(&hist_str)
            .map_err(|_err| ort_error(ErrorKind::HistoryParseFailed, "Failed to parse last"))?,
        Err("NOT FOUND") => {
            return Err(ort_error(
                ErrorKind::HistoryMissing,
                "No last conversation, cannot continue",
            ));
        }
        Err(_e) => {
            #[cfg(debug_assertions)]
            {
                // In debug build print the path.
                let c_last_file = CString::new(last_file).unwrap();
                unsafe { libc::write(2, c_last_file.as_ptr().cast(), c_last_file.count_bytes()) };
            }
            return Err(ort_error(
                ErrorKind::HistoryReadFailed,
                "Error reading last conversation file",
            ));
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
    settings: config::Settings,
    opts: PromptOpts,
    messages: Vec<crate::Message>,
    mut w: impl Write + Send,
) -> OrtResult<()> {
    // We'll likely never use this many, but making it the same as RESPONSE_BUF_LEN
    // makes for a smaller binary because generics.
    const MAX_MODELS: usize = 256;

    let num_models = opts.models.len();
    let mut msg = String::with_capacity(32);
    msg.push_str("Calling ");
    msg.push_str(&utils::num_to_string(num_models));
    msg.push_str(" models...\r");
    let _ = w.write(msg.as_bytes());
    let _ = w.flush();

    // Start all the queries
    let mut query_consumers = Vec::with_capacity(num_models);
    for idx in 0..num_models {
        let dns = settings.dns.clone();
        let opts_c = opts.clone();
        let messages_c = messages.clone();

        let model_name = opts_c.models.get(idx).unwrap().clone();
        let queue_single = start_prompt_thread(api_key, cancel_token, dns, opts_c, messages_c, idx);
        let consumer_collected = queue_single.consumer();
        query_consumers.push((model_name, consumer_collected));
    }

    let queue_done = queue::Queue::<String, MAX_MODELS>::new();
    let mut consumer_done = queue_done.consumer();

    // Collect the responses, printing them when ready
    let mut tids = Vec::with_capacity(num_models);
    for (model_name, consumer_collected) in query_consumers {
        let thread_params = Box::new(MultiCollectThreadParams {
            model_name,
            consumer_collected,
            full_output_queue: queue_done.clone(),
        });
        unsafe {
            match ort_thread::spawn(
                multi_collect_thread,
                Box::into_raw(thread_params) as *mut c_void,
            ) {
                Ok(tid) => {
                    tids.push(tid);
                }
                Err(err) => {
                    let s = CString::new(err.as_string()).unwrap();
                    libc::printf(
                        c"Spawning multi_collect_thread failed: %s".as_ptr(),
                        s.as_ptr(),
                    );
                }
            }
        }
    }

    for _ in 0..num_models {
        if cancel_token.is_cancelled() {
            break;
        }
        match consumer_done.get_next() {
            Some(model_output) => {
                let _ = w.write(model_output.as_bytes());
                let _ = w.write("\n\n".as_bytes());
                let _ = w.flush();
            }
            None => {
                // queue closed, which is impossible
                break;
            }
        }
    }

    // All the threads should be done
    for tid in tids {
        if cancel_token.is_cancelled() {
            break;
        }
        unsafe { libc::pthread_join(tid, ptr::null_mut()) };
    }

    Ok(())
}

struct PromptThreadParams {
    api_key: String,
    cancel_token: CancelToken,
    dns: Vec<String>,
    opts: PromptOpts,
    messages: Vec<Message>,
    model_idx: usize,
    queue: Arc<ResponseQueue>,
}

/// Start prompt in a new thread. Returns almost immediately with a queue.
/// Streams the response to the queue.
pub fn start_prompt_thread(
    api_key: &str,
    cancel_token: CancelToken,
    dns: Vec<String>,
    // Note we do not use the prompt from here, it should be in `messages` by now
    opts: PromptOpts,
    messages: Vec<Message>,
    // When running multiple models, this thread should use this one
    model_idx: usize,
) -> Arc<ResponseQueue> {
    let queue = ResponseQueue::new();
    let queue_clone = queue.clone();

    let params = Box::new(PromptThreadParams {
        api_key: api_key.to_string(),
        cancel_token,
        dns,
        opts,
        messages,
        model_idx,
        queue,
    });

    unsafe {
        if let Err(err) = ort_thread::spawn(prompt_thread, Box::into_raw(params) as *mut c_void) {
            let s = CString::new(err.as_string()).unwrap();
            libc::printf(c"Spawning prompt_thread failed: %s".as_ptr(), s.as_ptr());
        }
    }
    queue_clone
}

// extern "C" so that we can call it from `pthread_create`
extern "C" fn prompt_thread(arg: *mut c_void) -> *mut c_void {
    let params = unsafe { Box::from_raw(arg as *mut PromptThreadParams) };
    let body = match build_body(params.model_idx, &params.opts, &params.messages) {
        Ok(b) => b,
        Err(err) => {
            let s = CString::new(err.as_string()).unwrap();
            unsafe { libc::printf(c"FATAL: build_body: %s".as_ptr(), s.as_ptr()) };
            return ptr::null_mut();
        }
    };
    let start = time::Instant::now();
    let addrs: Vec<_> = if params.dns.is_empty() {
        let ips = match unsafe { resolver::resolve(c"openrouter.ai".as_ptr()) } {
            Ok(ips) => ips,
            Err(err) => {
                let s = CString::new(err.as_string()).unwrap();
                unsafe { libc::printf(c"FATAL: resolving openrouter.ai: %s".as_ptr(), s.as_ptr()) };
                return ptr::null_mut();
            }
        };
        ips.into_iter()
            .map(|ip| SocketAddr::new(IpAddr::V4(ip), 443))
            .collect()
    } else {
        params
            .dns
            .into_iter()
            .map(|a| {
                let ip_addr = a.parse::<Ipv4Addr>().unwrap();
                SocketAddr::new(IpAddr::V4(ip_addr), 443)
            })
            .collect()
    };
    let mut reader = match http::chat_completions(&params.api_key, addrs, &body) {
        Ok(r) => r,
        Err(err) => {
            let s = CString::new(err.as_string()).unwrap();
            unsafe { libc::printf(c"FATAL: running chat_completions: %s".as_ptr(), s.as_ptr()) };
            return ptr::null_mut();
        }
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
            params.queue.add(Response::Error(err.as_string()));
            return ptr::null_mut();
        }
    }

    let mut stats: stats::Stats = Default::default();
    let mut token_stream_start = None;
    let mut num_tokens = 0;
    let mut is_start = true;
    let mut is_first_reasoning = true;
    let mut is_first_content = true;

    let mut line_buf = String::with_capacity(128);
    loop {
        if params.cancel_token.is_cancelled() {
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
                params.queue.add(Response::Error(
                    "Stream read error: ".to_string() + &err.as_string(),
                ));
                break;
            }
        }
        let line = line_buf.trim();

        if is_start {
            // Very first message from server, usually
            // : OPENROUTER PROCESSING
            params.queue.add(Response::Start);
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
                    stats.time_to_first_token = Some(time::Instant::now() - start);
                    token_stream_start = Some(time::Instant::now());
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
                        params.queue.add(Response::Think(ThinkEvent::Start));
                        is_first_reasoning = false;
                    }
                    let r_event =
                        Response::Think(ThinkEvent::Content(reasoning_content.to_string()));
                    params.queue.add(r_event);
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
                        params.queue.add(Response::Think(ThinkEvent::Stop));
                        is_first_content = false;
                    }
                    let r_event = Response::Content(content.to_string());
                    params.queue.add(r_event);
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

    if params.cancel_token.is_cancelled() {
        // Probaby user did Ctrl-C
        params.queue.add(Response::Error("Interrupted".to_string()));
    } else {
        // Clean finish, send stats
        let now = time::Instant::now();
        stats.elapsed_time = now - start;
        let stream_elapsed_time = now - token_stream_start.unwrap();
        stats.inter_token_latency_ms = stream_elapsed_time.as_millis() / max(num_tokens, 1);

        params.queue.add(Response::Stats(stats));
    }
    params.queue.close();

    ptr::null_mut()
}

struct MultiCollectThreadParams {
    model_name: String,
    consumer_collected: ResponseConsumer,
    full_output_queue: Arc<queue::Queue<String, 256>>,
}

extern "C" fn multi_collect_thread(arg: *mut c_void) -> *mut c_void {
    let params = unsafe { Box::from_raw(arg as *mut MultiCollectThreadParams) };
    let mut mr = CollectedWriter {};
    match mr.run(params.consumer_collected) {
        Ok(full_output) => {
            params.full_output_queue.add(full_output);
        }
        Err(err) => {
            let err_str = err.as_string();
            if err_str.contains("429 Too Many Requests") {
                params
                    .full_output_queue
                    .add("--- ".to_string() + &params.model_name + ": Overloaded");
            } else {
                params
                    .full_output_queue
                    .add("--- ".to_string() + &params.model_name + ": " + &err_str);
            }
        }
    }
    ptr::null_mut()
}

struct SingleRunnerThreadParams<W: Write + Send> {
    w_core: W,
    consumer_stdout: ResponseConsumer,
    is_pipe_output: bool,
    show_reasoning: bool,
    is_quiet: bool,
}

extern "C" fn single_runner_thread<W: Write + Send>(arg: *mut c_void) -> *mut c_void {
    let params = unsafe { Box::from_raw(arg as *mut SingleRunnerThreadParams<W>) };

    let (stats, mut w_core) = if params.is_pipe_output {
        let mut fw = FileWriter {
            writer: params.w_core,
            show_reasoning: params.show_reasoning,
        };
        let stats = match fw.run(params.consumer_stdout) {
            Ok(stats) => stats,
            Err(err) => {
                let s = CString::new(err.as_string()).unwrap();
                unsafe {
                    libc::printf(
                        c"single_runner_thread FileWriter run: %s".as_ptr(),
                        s.as_ptr(),
                    )
                };
                return ptr::null_mut();
            }
        };
        let w_core = fw.into_inner();
        (stats, w_core)
    } else {
        let mut cw = ConsoleWriter {
            writer: params.w_core,
            show_reasoning: params.show_reasoning,
        };
        let stats = match cw.run(params.consumer_stdout) {
            Ok(stats) => stats,
            Err(err) => {
                let s = CString::new(err.as_string()).unwrap();
                unsafe {
                    libc::printf(
                        c"single_runner_thread ConsoleWriter run: %s".as_ptr(),
                        s.as_ptr(),
                    )
                };
                return ptr::null_mut();
            }
        };
        let w_core = cw.into_inner();
        (stats, w_core)
    };
    let _ = w_core.write(b"\n");
    if !params.is_quiet {
        let _ = w_core.write("\nStats: ".as_bytes());
        let _ = w_core.write(stats.as_string().as_bytes());
        let _ = w_core.write_char('\n');
    }

    ptr::null_mut()
}

struct LastWriterThreadParams {
    opts: PromptOpts,
    messages: Vec<Message>,
    consumer_last: ResponseConsumer,
}

extern "C" fn last_writer_thread(arg: *mut c_void) -> *mut c_void {
    let params = unsafe { Box::from_raw(arg as *mut LastWriterThreadParams) };
    let mut last_writer = match LastWriter::new(params.opts, params.messages) {
        Ok(lw) => lw,
        Err(err) => {
            let s = CString::new(err.as_string()).unwrap();
            unsafe {
                libc::printf(
                    c"last_writer_thread LastWriter::new: %s".as_ptr(),
                    s.as_ptr(),
                )
            };
            return ptr::null_mut();
        }
    };
    if let Err(err) = last_writer.run(params.consumer_last) {
        let s = CString::new(err.as_string()).unwrap();
        unsafe {
            libc::printf(
                c"last_writer_thread last_writer.run: %s".as_ptr(),
                s.as_ptr(),
            )
        };
    }
    ptr::null_mut()
}

/// Find the most recent file in `dir` that starts with `filename_prefix`.
/// Uses the minimal amount of disk access to go as fast as possible.
fn most_recent(dir: &str, filename_prefix: &str) -> OrtResult<String> {
    let c_dir = CString::new(dir)
        .map_err(|_| ort_error(ErrorKind::FileReadFailed, "Null byte in most_recent dir"))?;
    let dir_files = dir::DirFiles::new(c_dir.as_c_str())?;

    let mut most_recent_file: Option<(String, time::Instant)> = None;
    for name in dir_files {
        if !name.starts_with(filename_prefix) {
            continue;
        }
        let path = dir.to_string() + "/" + &name;
        let c_name = CString::new(path.clone()).map_err(|_| {
            ort_error(
                ErrorKind::FileReadFailed,
                "Null byte in most_recent_file name",
            )
        })?;
        let modified_time = file::last_modified(c_name.as_c_str())?;

        if let Some((_, prev_time)) = &most_recent_file {
            if modified_time > *prev_time {
                most_recent_file = Some((path, modified_time));
            }
        } else {
            most_recent_file = Some((path, modified_time));
        }
    }

    most_recent_file
        .map(|(path, _)| Ok(path))
        .unwrap_or_else(|| {
            Err(ort_error(
                ErrorKind::HistoryLookupFailed,
                "No files found starting with prefix",
            ))
        })
}
