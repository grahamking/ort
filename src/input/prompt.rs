//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::cmp::max;
use core::net::{IpAddr, Ipv4Addr, SocketAddr};

extern crate alloc;
use alloc::boxed::Box;
use alloc::ffi::CString;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::cli::Env;
use crate::net::AsFd;
use crate::{Context as _, OrtError, TcpSocket, TlsStream};

use crate::ChatCompletionsResponse;
use crate::OrtResult;
use crate::build_body;
use crate::common::dir;
use crate::common::file;
use crate::common::io::Write;
use crate::common::resolver;
use crate::common::site::Site;
use crate::common::stats::{self, Stats};
use crate::common::time;
use crate::common::utils;
use crate::common::{buf_read, config};
use crate::http;
use crate::ort_error;
use crate::output::last_writer::LastWriter;
use crate::output::writer::{CollectedWriter, ConsoleWriter, FileWriter, OutputWriter};
use crate::syscall::{self, F_SETFL, O_NONBLOCK, SOCK_CLOEXEC, SOCK_STREAM};
use crate::utils::print_string;
use crate::{ErrorKind, LastData};
use crate::{Message, PromptOpts};
use crate::{Response, ThinkEvent};

const EPOLL_WAIT_TIMEOUT_MS: i32 = 100;

struct EpollFd(i32);

impl EpollFd {
    fn raw(&self) -> i32 {
        self.0
    }
}

impl Drop for EpollFd {
    fn drop(&mut self) {
        let _ = syscall::close(self.0);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run<W: Write + Send>(
    api_key: &str,
    settings: config::Settings,
    env: &Env,
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
        Some(LastWriter::new(opts.clone(), messages.clone(), env)?)
    } else {
        None
    };

    let params = PromptThreadParams {
        api_key: api_key.to_string(),
        dns: settings.dns,
        opts,
        messages,
        model_idx: 0,
        site,
    };
    let mut active_prompt = ActivePrompt::new(params);
    active_prompt.start()?;

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
                // TODO? 429 is useful to know about
                // let err_str = err.as_string();
                // if err_str.contains("429 Too Many Requests") {
                utils::print_string(c"active_prompt.next: ", &err.as_string());
            }
        }
    }

    // Clean finish, send stats
    let stats = active_prompt.stop();
    output_writer.write(Response::Stats(stats))?;
    output_writer.stop(true)?; // prints stats
    // Finalize JSON
    if let Some(lw) = last_writer.as_mut() {
        lw.stop(true)?;
    }

    Ok(())
}

/// The `-c` continue operation. Load the most recent conversation for this
/// pane to populate the context, then run with the new prompt.
pub fn run_continue(
    api_key: &str,
    settings: config::Settings,
    env: &Env,
    mut opts: crate::PromptOpts,
    site: &'static Site,
    is_pipe_output: bool,
    w: impl Write + Send,
) -> OrtResult<()> {
    let mut last_path = [0u8; 128];
    let cache_dir_end = config::cache_dir(env, &mut last_path)?;
    last_path[cache_dir_end] = b'/';
    let last_filename = utils::last_filename(env);
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
                syscall::write(2, c_last_file.as_ptr().cast(), c_last_file.count_bytes());
            }
            return Err(ort_error(
                ErrorKind::HistoryReadFailed,
                "Error reading last conversation file",
            ));
        }
    };

    opts.merge(last.opts);
    last.messages
        .push(crate::Message::user(opts.prompt.take().unwrap(), vec![]));

    run(
        api_key,
        settings,
        env,
        opts,
        site,
        last.messages,
        is_pipe_output,
        w,
    )
}

pub fn run_multi(
    api_key: &str,
    settings: config::Settings,
    opts: PromptOpts,
    site: &'static Site,
    messages: Vec<crate::Message>,
    mut w: impl Write + Send,
) -> OrtResult<()> {
    let num_models = opts.models.len();
    let mut msg = String::with_capacity(32);
    msg.push_str("Calling ");
    msg.push_str(&utils::num_to_string(num_models));
    msg.push_str(" models...\r");
    let _ = w.write(msg.as_bytes());
    let _ = w.flush();

    let epoll_fd = syscall::epoll_create(num_models as i32);
    if epoll_fd < 0 {
        return Err(ort_error(ErrorKind::Other, "epoll_create"));
    }
    let epoll_fd = EpollFd(epoll_fd);
    let mut names = Vec::with_capacity(num_models); // debug
    let mut active_prompts = Vec::with_capacity(num_models);
    let mut active_writers = Vec::with_capacity(num_models);

    // Start all the queries.
    // We negotiate TLS one at a time, should start epoll earlier to do all at once.
    for idx in 0..num_models {
        let model_name = opts.models.get(idx).unwrap().clone();
        names.push(model_name);

        let params = PromptThreadParams {
            api_key: api_key.to_string(),
            dns: settings.dns.clone(),
            site,
            opts: opts.clone(),
            messages: messages.clone(),
            model_idx: idx,
        };
        let mut active_prompt = ActivePrompt::new(params);
        active_prompt.start()?;
        let socket_fd = active_prompt.as_fd();

        active_prompts.push(active_prompt);
        active_writers.push(CollectedWriter::new());

        syscall::fcntl(socket_fd, F_SETFL, SOCK_STREAM | SOCK_CLOEXEC | O_NONBLOCK);
        let mut event = syscall::epoll_event {
            events: syscall::EPOLLIN,
            data: active_prompts.len() as u64 - 1,
        };
        if syscall::epoll_ctl(
            epoll_fd.raw(),
            syscall::EPOLL_CTL_ADD,
            socket_fd,
            &mut event,
        ) < 0
        {
            return Err(ort_error(ErrorKind::Other, "epoll_ctl"));
        }
    }

    let mut ready_events = vec![syscall::epoll_event { events: 0, data: 0 }; active_prompts.len()];
    while !ready_events.is_empty() {
        let num_ready = syscall::epoll_wait(
            epoll_fd.raw(),
            ready_events.as_mut_ptr(),
            ready_events.len() as i32,
            EPOLL_WAIT_TIMEOUT_MS,
        );
        if num_ready < 0 {
            // Ctrl-C
            break;
        }
        if num_ready == 0 {
            continue;
        }

        let mut num_done = 0;
        for evt in ready_events[..num_ready as usize].iter() {
            let active_prompt = &mut active_prompts[evt.data as usize];
            let output_writer = &mut active_writers[evt.data as usize];
            //let name = &names[evt.data as usize];

            // TODO: loop until WouldBlock?

            match active_prompt.next() {
                Ok(out) if out.is_empty() => {
                    num_done += 1;

                    let stats = active_prompt.stop();
                    output_writer.write(Response::Stats(stats))?;
                    output_writer.stop(true)?;

                    let _ = w.write(output_writer.output.as_ref().unwrap().as_bytes());
                    let _ = w.write("\n\n".as_bytes());
                    let _ = w.flush();
                }
                Ok(out) => {
                    for event in out {
                        output_writer.write(event.clone())?;
                    }
                }
                Err(OrtError {
                    kind: ErrorKind::WouldBlock,
                    ..
                }) => {
                    // we read all the data, back to epoll_wait
                }
                Err(err) => {
                    utils::print_string(c"active_prompt.next: ", &err.as_string());
                }
            }
        }

        for _ in 0..num_done {
            let _ = ready_events.pop();
        }
    }
    Ok(())
}

struct PromptThreadParams {
    api_key: String,
    dns: Vec<String>,
    opts: PromptOpts,
    messages: Vec<Message>,
    model_idx: usize,
    site: &'static Site,
}

struct ActivePrompt {
    api_key: String,
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
            self.line_buf.clear();
            match self
                .reader
                .as_mut()
                .unwrap()
                .read_line(&mut self.line_buf)?
            {
                0 => {
                    // EOF
                    return Ok(queue);
                }
                _ => {
                    // success
                }
            }
            let line = self.line_buf.trim();
            //utils::print_string(c"LINE: ", line);

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
                    let has_content = !delta.content.is_empty();
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
                    if let Some(content) = delta.content.first()
                        && content.len() != 0
                    {
                        self.num_tokens += 1;
                        let content = content.content();
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

    pub fn stop(&mut self) -> Stats {
        let now = time::Ticks::now();
        self.stats.elapsed_time =
            time::elapsed_duration(self.start.take().unwrap(), now, self.tsc_calibration);
        let stream_elapsed_time =
            time::elapsed_duration(self.token_stream_start.unwrap(), now, self.tsc_calibration);
        self.stats.inter_token_latency_ms =
            stream_elapsed_time.as_millis() / max(self.num_tokens, 1) as u128;

        self.stats.clone()
    }

    /*
    fn has_pending_data(&self) -> bool {
        self.reader
            .as_ref()
            .map(|reader| reader.has_pending_data())
            .unwrap_or(false)
    }
    */
}

impl AsFd for ActivePrompt {
    fn as_fd(&self) -> i32 {
        self.reader.as_ref().unwrap().as_fd()
    }
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
