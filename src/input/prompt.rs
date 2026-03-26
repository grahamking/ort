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

use crate::net::AsFd;
use crate::{Context as _, TcpSocket, TlsStream};

use crate::ChatCompletionsResponse;
use crate::OrtResult;
use crate::build_body;
use crate::common::dir;
use crate::common::file;
use crate::common::io::Write;
use crate::common::queue;
use crate::common::resolver;
use crate::common::site::Site;
use crate::common::stats::{self, Stats};
use crate::common::time;
use crate::common::utils;
use crate::common::{buf_read, config};
use crate::libc::{self, F_SETFL, O_NONBLOCK, SOCK_CLOEXEC, SOCK_STREAM};
use crate::ort_error;
use crate::output::last_writer::LastWriter;
use crate::output::writer::{CollectedWriter, ConsoleWriter, FileWriter, OutputWriter};
use crate::utils::print_string;
use crate::{CancelToken, http, thread as ort_thread};
use crate::{ErrorKind, LastData};
use crate::{Message, PromptOpts};
use crate::{Response, ThinkEvent};

// The readers must never fall further behind than this many responses
const RESPONSE_BUF_LEN: usize = 256;

type ResponseQueue = queue::Queue<Response, RESPONSE_BUF_LEN>;
type ResponseConsumer = queue::Consumer<Response, RESPONSE_BUF_LEN>;

#[allow(clippy::too_many_arguments)]
pub fn run<W: Write + Send>(
    api_key: &str,
    cancel_token: CancelToken,
    settings: config::Settings,
    opts: PromptOpts,
    site: &'static Site,
    messages: Vec<crate::Message>,
    is_pipe_output: bool, // Are we redirecting stdout?
    w_core: W,
) -> OrtResult<()> {
    let show_reasoning = opts.show_reasoning.unwrap();
    let is_quiet = opts.quiet.unwrap_or_default();
    //let model_name = opts.common.model.clone().unwrap();

    let mut output_writer: Box<dyn OutputWriter> = if is_pipe_output {
        Box::new(FileWriter::new(w_core, show_reasoning, is_quiet))
    } else {
        Box::new(ConsoleWriter::new(w_core, show_reasoning, is_quiet))
    };

    let mut last_writer = if settings.save_to_file {
        Some(LastWriter::new(opts.clone(), messages.clone())?)
    } else {
        None
    };

    let prompt_cancel_token = cancel_token;
    let params = PromptThreadParams {
        api_key: api_key.to_string(),
        cancel_token: prompt_cancel_token,
        dns: settings.dns,
        opts,
        messages,
        model_idx: 0,
        queue: ResponseQueue::new(),
        site,
    };
    let mut active_prompt = ActivePrompt::new(params);
    active_prompt.start()?;

    // now set it non-blocking so we can epoll
    let _socket_fd = active_prompt.as_fd();
    // technically we should F_GETFL first to get the flags, but we know what they are
    //libc::fcntl(socket_fd, F_SETFL, SOCK_STREAM | SOCK_CLOEXEC | O_NONBLOCK);

    loop {
        match active_prompt.next() {
            Ok(out) if out.is_empty() => {
                break;
            }
            Ok(out) => {
                for event in out {
                    output_writer.write(event.clone())?;
                    if let Some(lw) = last_writer.as_mut() {
                        lw.write(event)?;
                    }
                }
            }
            Err(err) => {
                utils::print_string(c"active_prompt.next: ", &err.as_string());
            }
        }
    }

    if cancel_token.is_cancelled() {
        // Probaby user did Ctrl-C
        output_writer.write(Response::Error("Interrupted".to_string()))?;
    } else {
        // Clean finish, send stats
        let stats = active_prompt.stop();
        output_writer.write(Response::Stats(stats))?;
        output_writer.stop()?; // prints stats
        // Finalize JSON
        if let Some(lw) = last_writer.as_mut() {
            lw.stop()?;
        }
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
    site: &'static Site,
    is_pipe_output: bool,
    w: impl Write + Send,
) -> OrtResult<()> {
    let mut last_path = [0u8; 128];
    let cache_dir_end = config::cache_dir(&mut last_path)?;
    last_path[cache_dir_end] = b'/';
    let last_filename = utils::last_filename();
    let start = cache_dir_end + 1;
    let end = start + last_filename.len();
    last_path[start..end].copy_from_slice(last_filename.as_bytes());

    let cs = CString::new(&last_path[..end]).expect("Null bytes in config cache dir");
    let last_file = if utils::path_exists(cs.as_ref()) {
        unsafe { String::from_utf8_unchecked(last_path[..end].into()) }
    } else {
        let cache_dir = unsafe { str::from_utf8_unchecked(&last_path[..cache_dir_end]) };
        most_recent(cache_dir, "last-").context("most_recent")?
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
                libc::write(2, c_last_file.as_ptr().cast(), c_last_file.count_bytes());
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
        site,
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
    site: &'static Site,
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
        let queue_single =
            start_prompt_thread(api_key, cancel_token, dns, opts_c, site, messages_c, idx);
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
                    print_string(c"Spawning multi_collect_thread failed: ", &err.as_string());
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
    site: &'static Site,
}

/// Start prompt in a new thread. Returns almost immediately with a queue.
/// Streams the response to the queue.
pub fn start_prompt_thread(
    api_key: &str,
    cancel_token: CancelToken,
    dns: Vec<String>,
    // Note we do not use the prompt from here, it should be in `messages` by now
    opts: PromptOpts,
    site: &'static Site,
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
        site,
    });

    unsafe {
        if let Err(err) = ort_thread::spawn(prompt_thread, Box::into_raw(params) as *mut c_void) {
            print_string(c"Spawning prompt_thread failed: ", &err.as_string());
        }
    }
    queue_clone
}

struct ActivePrompt {
    api_key: String,
    cancel_token: CancelToken,
    dns: Vec<String>,
    // Note we do not use the prompt from here, it should be in `messages` by now
    opts: PromptOpts,
    site: &'static Site,
    messages: Vec<Message>,
    // When running multiple models, this thread should use this one
    model_idx: usize,

    reader: Option<buf_read::OrtBufReader<TlsStream<TcpSocket>>>,

    stats: stats::Stats,
    tsc_calibration: time::TscCalibration,
    token_stream_start: Option<time::Ticks>,
    start: Option<time::Ticks>,
    num_tokens: usize,
    is_start: bool,
    is_first_reasoning: bool,
    is_first_content: bool,
    line_buf: String,
}

impl ActivePrompt {
    pub fn new(params: PromptThreadParams) -> Self {
        ActivePrompt {
            api_key: params.api_key,
            cancel_token: params.cancel_token,
            dns: params.dns,
            site: params.site,
            messages: params.messages,
            model_idx: params.model_idx,
            reader: None,
            stats: Stats {
                // Default the model to the passed one, in case provider stats don't include it
                used_model: params
                    .opts
                    .models
                    .get(params.model_idx)
                    .cloned()
                    .expect("Missing model name"),
                // Provider doesn't make sense for build.nvidia.com
                provider: params.site.name.to_string(),
                ..Default::default()
            },
            tsc_calibration: match time::tsc_calibration() {
                Ok(tc) => tc,
                Err(err) => {
                    print_string(c"FATAL running tsc_calibration: ", &err.as_string());
                    panic!("FATAL running tsc_calibration: {}", err.as_string());
                }
            },
            opts: params.opts,
            token_stream_start: None,
            start: None,
            num_tokens: 0,
            is_start: true,
            is_first_reasoning: true,
            is_first_content: true,
            line_buf: String::with_capacity(1024),
        }
    }

    /// Start the HTTP request
    pub fn start(&mut self) -> OrtResult<()> {
        let body = match build_body(self.model_idx, &self.opts, &self.messages) {
            Ok(b) => b,
            Err(err) => {
                print_string(c"FATAL: build_body: ", &err.as_string());
                return Err(ort_error(ErrorKind::Other, "build body"));
            }
        };
        self.start = Some(time::Ticks::now());
        let addr = if self.dns.is_empty() {
            let ip = match unsafe { resolver::resolve(self.site.dns_label) } {
                Ok(ip) => ip,
                Err(err) => {
                    print_string(c"FATAL: resolving host: ", &err.as_string());
                    return Err(ort_error(ErrorKind::DnsResolveFailed, ""));
                }
            };
            SocketAddr::new(IpAddr::V4(ip), self.site.port)
        } else {
            self.dns
                .first()
                .map(|a| {
                    let ip_addr = a.parse::<Ipv4Addr>().unwrap();
                    SocketAddr::new(IpAddr::V4(ip_addr), self.site.port)
                })
                .unwrap()
        };
        self.reader = match http::chat_completions(
            &self.api_key,
            self.site.host,
            self.site.chat_completions_url,
            vec![addr],
            &body,
        ) {
            Ok(r) => Some(r),
            Err(err) => {
                print_string(c"FATAL running chat_completions: ", &err.as_string());
                return Err(ort_error(ErrorKind::Other, "running chat_completions"));
            }
        };

        match http::skip_header(self.reader.as_mut().unwrap()) {
            Ok(true) => {
                // TODO it's transfer encoding chunked, this is the common case
                // which we don't handle correctly yet
            }
            Ok(false) => {
                // Not chunked encoding. I don't think we ever get here.
                // But the rest of this function assumes we do.
            }
            Err(err) => {
                print_string(c"FATAL running skip_header: ", &err.as_string());
                return Err(ort_error(ErrorKind::HttpStatusError, "running skip_header"));
            }
        }

        Ok(())
    }

    pub fn next(&mut self) -> OrtResult<Vec<Response>> {
        let mut queue = vec![];

        loop {
            if self.cancel_token.is_cancelled() {
                return Ok(queue);
            }

            self.line_buf.clear();
            match self.reader.as_mut().unwrap().read_line(&mut self.line_buf) {
                Ok(0) => {
                    // EOF
                    return Ok(queue);
                }
                Ok(_) => {
                    // success
                }
                Err(err) => {
                    queue.push(Response::Error(
                        "Stream read error: ".to_string() + &err.as_string(),
                    ));
                    // TODO: Should we return Err instead?
                    return Ok(queue);
                }
            }
            let line = self.line_buf.trim();
            //utils::print_string(c"LINE: ", &line_buf);

            if self.is_start {
                // Very first message from server, usually
                // : OPENROUTER PROCESSING
                queue.push(Response::Start);
                self.is_start = false;
                return Ok(queue);
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
                return Ok(queue);
            }

            // Each data: line is a JSON chunk in OpenAI streaming format
            match ChatCompletionsResponse::from_json(data) {
                Ok(mut v) => {
                    // Handle last message which contains the "usage" key
                    // Do this before getting choices because it's empty on last message.
                    if let Some(usage) = v.usage {
                        self.stats.cost_in_cents = Some(usage.cost as f64 * 100.0); // convert to cents
                        self.stats.provider =
                            v.provider.expect("Last message was missing provider");
                        self.stats.used_model = v.model.expect("Last message was missing model");
                    }

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

                    if !(has_reasoning || has_content) {
                        continue;
                    }

                    // Record time to first token
                    if self.stats.time_to_first_token.is_none() {
                        let first_token = time::Ticks::now();
                        self.stats.time_to_first_token = Some(time::elapsed_duration(
                            self.start.unwrap(),
                            first_token,
                            self.tsc_calibration,
                        ));
                        self.token_stream_start = Some(time::Ticks::now());
                    }

                    // Handle reasoning content
                    if let Some(reasoning_content) = delta.reasoning.as_ref()
                        && !reasoning_content.is_empty()
                    {
                        self.num_tokens += 1;
                        if self.is_first_reasoning {
                            if reasoning_content.trim().is_empty() {
                                // Don't allow starting with carriage return or blank space, that messes up the display
                                continue;
                            }
                            queue.push(Response::Think(ThinkEvent::Start));
                            self.is_first_reasoning = false;
                        }
                        let r_event =
                            Response::Think(ThinkEvent::Content(reasoning_content.to_string()));
                        queue.push(r_event);
                    }

                    // Handle regular content
                    if let Some(content) = delta.content.as_ref()
                        && !content.is_empty()
                    {
                        self.num_tokens += 1;
                        if self.is_first_content && content.trim().is_empty() {
                            // Don't allow starting with carriage return or blank space, that messes up the display
                            if queue.is_empty() {
                                continue;
                            } else {
                                return Ok(queue);
                            }
                        }
                        // If we signaled the open (!is_first_reasoning)
                        // and we signaled the close yet (is_first_reasoning),
                        // signal the close.
                        if !self.is_first_reasoning && self.is_first_content {
                            queue.push(Response::Think(ThinkEvent::Stop));
                            self.is_first_content = false;
                        }
                        let r_event = Response::Content(content.to_string());
                        queue.push(r_event);
                    }
                }
                Err(err) => {
                    utils::print_string(c"Malformed: ", &err);
                }
            }

            return Ok(queue);
        }
    }

    pub fn stop(mut self) -> Stats {
        let now = time::Ticks::now();
        self.stats.elapsed_time =
            time::elapsed_duration(self.start.take().unwrap(), now, self.tsc_calibration);
        let stream_elapsed_time =
            time::elapsed_duration(self.token_stream_start.unwrap(), now, self.tsc_calibration);
        self.stats.inter_token_latency_ms =
            stream_elapsed_time.as_millis() / max(self.num_tokens, 1) as u128;

        self.stats
    }
}

impl AsFd for ActivePrompt {
    fn as_fd(&self) -> i32 {
        self.reader.as_ref().unwrap().as_fd()
    }
}

// extern "C" so that we can call it from `pthread_create`
extern "C" fn prompt_thread(arg: *mut c_void) -> *mut c_void {
    let params = unsafe { Box::from_raw(arg as *mut PromptThreadParams) };
    let queue = params.queue.clone();
    let cancel_token = params.cancel_token;

    let mut active_prompt = ActivePrompt::new(*params);
    if active_prompt.start().is_err() {
        // "start()" already logged the error
        return ptr::null_mut();
    };

    loop {
        match active_prompt.next() {
            Ok(out) if out.is_empty() => {
                break;
            }
            Ok(out) => {
                for resp in out {
                    queue.add(resp);
                }
            }
            Err(err) => {
                utils::print_string(c"active_prompt.next: ", &err.as_string());
            }
        }
    }

    if cancel_token.is_cancelled() {
        // Probaby user did Ctrl-C
        queue.add(Response::Error("Interrupted".to_string()));
    } else {
        // Clean finish, send stats
        let stats = active_prompt.stop();
        queue.add(Response::Stats(stats));
    }
    queue.close();

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
