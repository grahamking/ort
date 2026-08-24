//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025, 2026 Graham King

use core::cmp::max;
use core::net::{IpAddr, Ipv4Addr, SocketAddr};

extern crate alloc;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::cli::Env;
use crate::common::buf_read::OrtBufReader;
use crate::common::data::{Tool, ToolCall};
use crate::common::error::Context;
use crate::net::AsFd;
use crate::output::logger::Logger;
use crate::{OrtError, chunked};

use crate::ChatCompletionsResponse;
use crate::Message;
use crate::OrtResult;
use crate::build_body;
use crate::common::config::Cfg;
use crate::common::error::{ort_err, ort_error};
use crate::common::io::{ReadLine, Write};
use crate::common::resolver;
use crate::common::stats::{self, Stats};
use crate::common::time;
use crate::common::utils;
use crate::http::{self, ContentLengthReader};
use crate::input::args::PromptOpts;
use crate::output::last_writer::LastWriter;
use crate::output::writer::{CollectedWriter, ConsoleWriter, FileWriter};
use crate::output::{OutputWriter, last_writer};
use crate::syscall::{self, F_SETFL, O_NONBLOCK, SOCK_CLOEXEC, SOCK_STREAM};
use crate::{ErrorKind, LastData};
use crate::{Response, ThinkEvent};

const EPOLL_WAIT_TIMEOUT_MS: i32 = 100;

/// Same size as input/list.rs but likely could be much smaller
/// Same size means the generic is shared, smaller code.
const MAX_CHUNK_SIZE: usize = 128 * 1024;

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
    cfg: &Cfg,
    env: &Env,
    messages: Vec<Message>,
    is_pipe_output: bool, // Are we redirecting stdout?
    w_core: &mut W,
) -> OrtResult<()> {
    let mut output_writer: Box<dyn OutputWriter> = if is_pipe_output {
        Box::new(FileWriter::new(w_core, cfg.show_reasoning, cfg.quiet))
    } else {
        Box::new(ConsoleWriter::new(w_core, cfg.show_reasoning, cfg.quiet))
    };

    let mut last_writer = if !cfg.is_private {
        // Save the config so we use the same next time
        Some(LastWriter::new(messages.clone(), env, cfg)?)
    } else {
        None
    };

    let mut active_prompt = ActivePrompt::new(api_key.to_string(), cfg, messages, vec![], 0, env)?;
    active_prompt.start()?;

    loop {
        match active_prompt.next() {
            Ok(None) => {
                break;
            }
            Ok(Some(out)) => {
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
    output_writer.write(Response::Stats(stats.clone()))?;
    output_writer.stop(true)?; // prints stats
    // Finalize JSON
    if let Some(lw) = last_writer.as_mut() {
        // Last writer needs the provider from Stats so we can use the same one next time
        lw.write(Response::Stats(stats))?;
        lw.stop(true)?;
    }

    Ok(())
}

pub(in crate::input) fn load_last_data(env: &Env) -> OrtResult<LastData> {
    let last_file_path = last_writer::last_data_file(env)?;
    match utils::filename_read_to_string(&last_file_path) {
        Ok(hist_str) => LastData::from_json(&hist_str).map_err(|err| {
            let msg = last_file_path + " - " + &err.as_string();
            ort_err(ErrorKind::HistoryParseFailed, msg.into())
        }),
        Err("NOT FOUND") => Err(ort_err(
            ErrorKind::HistoryMissing,
            (last_file_path + " not found. No last conversation, cannot continue").into(),
        )),
        Err(err) => {
            let msg = last_file_path + " read error on last conversation file -" + err;
            Err(ort_err(ErrorKind::HistoryReadFailed, msg.into()))
        }
    }
}

/// The `-c` continue operation. Load the most recent conversation for this
/// pane to populate the context, then run with the new prompt.
pub fn run_continue<W: Write + Send>(
    api_key: &str,
    cfg: &Cfg,
    env: &Env,
    new_prompt: String,
    is_pipe_output: bool,
    w: &mut W,
) -> OrtResult<()> {
    let mut last = load_last_data(env)?;
    last.messages.push(crate::Message::user(new_prompt));

    run(api_key, cfg, env, last.messages, is_pipe_output, w)
}

pub fn run_multi<W: Write + Send>(
    api_key: &str,
    cfg: &Cfg,
    env: &Env,
    opts: PromptOpts,
    messages: Vec<crate::Message>,
    w: &mut W,
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
        let model_id = opts.models.get(idx).unwrap().clone();
        names.push(model_id);

        let mut active_prompt =
            ActivePrompt::new(api_key.to_string(), cfg, messages.clone(), vec![], idx, env)?;
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

    let mut is_active = vec![true; num_models];
    let mut remaining = num_models;
    let mut ready_events = vec![syscall::epoll_event { events: 0, data: 0 }; num_models];
    while remaining > 0 {
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

        for evt in ready_events[..num_ready as usize].iter() {
            let idx = evt.data as usize;
            if !is_active[idx] {
                continue;
            }

            let active_prompt = &mut active_prompts[idx];
            let output_writer = &mut active_writers[idx];
            //let name = &names[evt.data as usize];

            // TODO: loop until WouldBlock?

            match active_prompt.next() {
                Ok(None) => {
                    is_active[idx] = false;
                    remaining -= 1;
                    let socket_fd = active_prompt.as_fd();
                    let mut event = syscall::epoll_event { events: 0, data: 0 };
                    let _ = syscall::epoll_ctl(
                        epoll_fd.raw(),
                        syscall::EPOLL_CTL_DEL,
                        socket_fd,
                        &mut event,
                    );

                    let stats = active_prompt.stop();
                    output_writer.write(Response::Stats(stats))?;
                    output_writer.stop(true)?;

                    let _ = w.write(output_writer.output.as_ref().unwrap().as_bytes());
                    let _ = w.write("\n\n".as_bytes());
                    let _ = w.flush();
                }
                Ok(Some(out)) => {
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
    }
    Ok(())
}

pub trait PromptReader: ReadLine + AsFd {}

pub struct ActivePrompt {
    api_key: String,
    cfg: Cfg,
    messages: Vec<Message>,
    tools: Vec<&'static Tool>,
    session_id: String,

    // When running multiple models, this thread should use this one
    model_idx: usize,

    reader: Option<Box<dyn PromptReader>>,

    stats: stats::Stats,
    tsc_calibration: Option<time::TscCalibration>,
    token_stream_start: Option<time::Ticks>,
    start: Option<time::Ticks>,
    num_tokens: usize,
    is_start: bool,
    is_first_reasoning: bool,
    is_first_content: bool,
    line_buf: String,

    pending_tool_calls: Vec<ToolCall>,
    logger: Option<Logger>,
}

impl ActivePrompt {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        api_key: String,
        cfg: &Cfg,
        messages: Vec<Message>,
        tools: Vec<&'static Tool>,
        model_idx: usize,
        env: &Env,
    ) -> OrtResult<Self> {
        let session_id = if model_idx == 0 {
            cfg.session_id.clone()
        } else {
            alloc::format!("{}-{model_idx}", cfg.session_id)
        };
        let model_id = cfg.models.get(model_idx).expect("Missing model name");
        let logger = if cfg.is_private {
            None
        } else {
            Some(Logger::new(env, &utils::slug(model_id.as_str()))?)
        };
        Ok(ActivePrompt {
            api_key,
            cfg: cfg.clone(),
            messages,
            tools,
            session_id,
            model_idx,
            reader: None,
            stats: Stats {
                // Default the model to the passed one, in case provider stats don't include it
                used_model: model_id.clone(),
                // Provider doesn't make sense for build.nvidia.com
                provider: "".to_string(),
                ..Default::default()
            },
            // TODO: Should we warn this CPU doesn't have TSC calibration, so no timing?
            //print_string(c"FATAL running tsc_calibration: ", &err.as_string());
            tsc_calibration: time::tsc_calibration().ok(),
            token_stream_start: None,
            start: None,
            num_tokens: 0,
            is_start: true,
            is_first_reasoning: true,
            is_first_content: true,
            line_buf: String::with_capacity(1024),
            pending_tool_calls: vec![],
            logger,
        })
    }

    /// Start the HTTP request
    pub fn start(&mut self) -> OrtResult<()> {
        let body = build_body(self.model_idx, &self.cfg, &self.messages, &self.tools)
            .context("build_body")?;
        if let Some(l) = self.logger.as_mut() {
            l.log(&body);
        }
        let (host, port, base_path) = http::split_url(&self.cfg.base_url);
        self.start = Some(time::Ticks::now());
        let addrs = if self.cfg.dns.is_empty() {
            let ips = unsafe { resolver::resolve(host).context("resolver::resolve")? };
            ips.into_iter()
                .map(|ip| SocketAddr::new(IpAddr::V4(ip), port))
                .collect()
        } else {
            self.cfg
                .dns
                .iter()
                .map(|a| {
                    let ip_addr = a.parse::<Ipv4Addr>().unwrap();
                    SocketAddr::new(IpAddr::V4(ip_addr), port)
                })
                .collect()
        };
        let mut buf_reader = http::chat_completions(
            &self.api_key,
            host,
            base_path,
            &self.session_id,
            addrs,
            &body,
        )
        .context("http::chat_completions")?;

        match http::skip_header(&mut buf_reader).context("http::skip_header")? {
            http::ResponseBody::Chunked => {
                // Transfer encoding chunked, this is what OpenRouter does.
                let chunk_reader = chunked::read::<_, MAX_CHUNK_SIZE>(buf_reader);
                self.reader = Some(Box::new(chunk_reader));
            }
            http::ResponseBody::ContentLength(len) => {
                // Content-Length with keep-alive. Stop at the body length.
                // Rare except for upstream errors which are non-streaming.
                let content_reader = ContentLengthReader::new(buf_reader, len);
                self.reader = Some(Box::new(OrtBufReader::new(content_reader)));
            }
            http::ResponseBody::UntilEof => {
                // OpenRouter does chunked. Only seen this on local dev server.
                self.reader = Some(Box::new(buf_reader));
            }
        }

        Ok(())
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> OrtResult<Option<Vec<Response>>> {
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
                    return Ok(None);
                }
                _ => {
                    // success
                }
            }
            let line = self.line_buf.trim();
            // utils::print_string(c"LEN: ", &crate::utils::num_to_string(line.len()));
            // utils::print_string(c"LINE: ", line);

            if self.is_start {
                // Very first message from server, often
                // : OPENROUTER PROCESSING
                queue.push(Response::Start);
                self.is_start = false;
                // It might be a real data line, so continue processing
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
                return Ok(None);
            }

            // Log now that it's interesting
            if let Some(l) = self.logger.as_mut() {
                l.log(data);
            }

            // Each data: line is a JSON chunk in OpenAI streaming format
            match ChatCompletionsResponse::from_json(data) {
                Ok(mut v) => {
                    // Handle last message which contains the "usage" key
                    // Do this before getting choices because it's empty on last message.
                    if let Some(usage) = v.usage {
                        self.stats.cost_in_cents = Some(usage.cost as f64 * 100.0); // convert to cents
                        self.stats.web_search_requests = usage.web_search_requests;
                        if let Some(provider) = v.provider {
                            self.stats.provider = provider;
                        }
                        if let Some(model) = v.model {
                            self.stats.used_model = model;
                        }
                    }

                    if let Some(error) = v.error {
                        queue.push(Response::Error(error.message().to_string()));
                    };

                    // Standard OpenAI stream delta shape
                    let Some(choice) = v.choices.pop() else {
                        if queue.is_empty() {
                            continue;
                        } else {
                            return Ok(Some(queue));
                        }
                    };

                    let has_reasoning = choice
                        .delta
                        .reasoning
                        .as_ref()
                        .map(|x| !x.is_empty())
                        .unwrap_or(false);
                    let content = choice.delta.text();
                    let has_content = content.map(|x| !x.is_empty()).unwrap_or(false);
                    let has_tool_calls = !choice.delta.tool_calls.is_empty();
                    let has_annotations = !choice.delta.annotations.is_empty();
                    let is_finished = choice.finish_reason.is_some();

                    if !(has_reasoning
                        || has_content
                        || has_tool_calls
                        || has_annotations
                        || is_finished)
                    {
                        continue;
                    }

                    // Record time to first token
                    if self.stats.time_to_first_token.is_none() {
                        let first_token = time::Ticks::now();
                        self.stats.time_to_first_token = self
                            .tsc_calibration
                            .map(|tc| time::elapsed_duration(self.start.unwrap(), first_token, tc));
                        self.token_stream_start = Some(time::Ticks::now());
                    }

                    // Handle tool calls
                    if has_tool_calls {
                        // TODO: Think about ownership, reduce copying
                        for tool_call in &choice.delta.tool_calls {
                            match self.pending_tool_calls.get_mut(tool_call.index as usize) {
                                Some(pending) => {
                                    pending.update_from(tool_call);
                                }
                                None => {
                                    self.pending_tool_calls.push(tool_call.clone());
                                }
                            }
                        }
                    }

                    // Handle reasoning content
                    if let Some(reasoning_content) = choice.delta.reasoning.as_ref()
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

                    // Handle annotations
                    for a in &choice.delta.annotations {
                        queue.push(Response::Annotation(a.clone()));
                    }

                    // Handle regular content
                    if let Some(content) = content
                        && !content.is_empty()
                    {
                        self.num_tokens += 1;
                        if self.is_first_content && content.trim().is_empty() {
                            // Don't allow starting with carriage return or blank space, that messes up the display
                            if queue.is_empty() {
                                continue;
                            } else {
                                return Ok(Some(queue));
                            }
                        }
                        // If we signaled the open (!is_first_reasoning)
                        // and we haven't signaled the close yet (is_first_reasoning),
                        // signal the close.
                        if !self.is_first_reasoning && self.is_first_content {
                            queue.push(Response::Think(ThinkEvent::Stop));
                            self.is_first_content = false;
                        }
                        let r_event = Response::Content(content.to_string());
                        queue.push(r_event);
                    }

                    if choice.is_tool_call_finish() {
                        let event =
                            Response::ToolCalls(core::mem::take(&mut self.pending_tool_calls));
                        queue.push(event);
                    }
                }
                Err(err) => {
                    if let Some(l) = self.logger.as_mut() {
                        l.log(&err.as_string());
                    }
                    #[cfg(debug_assertions)]
                    {
                        utils::print_string(c"Malformed: ", &err.as_string());
                        utils::print_string(c"DATA: ", data);
                    }
                }
            }

            if queue.is_empty() && !self.pending_tool_calls.is_empty() {
                // Tool calls are sometimes spread over several messages
                continue;
            }

            return Ok(Some(queue));
        }
    }

    pub fn stop(&mut self) -> Stats {
        if let Some(tc) = self.tsc_calibration {
            let now = time::Ticks::now();
            if let Some(start) = self.start.take() {
                self.stats.elapsed_time = time::elapsed_duration(start, now, tc);
            }
            if let Some(token_stream_start) = self.token_stream_start {
                let stream_elapsed_time = time::elapsed_duration(token_stream_start, now, tc);
                self.stats.inter_token_latency_ms =
                    stream_elapsed_time.as_millis() / max(self.num_tokens, 1) as u128;
            }
        };
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

#[cfg(test)]
mod test {
    use core::assert_matches;

    extern crate alloc;
    use alloc::boxed::Box;
    use alloc::string::ToString;
    use alloc::vec;

    use crate::cli::Env;
    use crate::common::buf_read::OrtBufReader;
    use crate::common::{time, utils};
    use crate::config::Cfg;
    use crate::input::prompt::PromptReader;
    use crate::{Response, ThinkEvent};

    #[test]
    fn test_annotation_realistic() {
        let test_file = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/fixtures/parse_test.jsonl";
        let test_data = utils::filename_read_to_string(&test_file).unwrap();
        let mut active_prompt = super::ActivePrompt::new(
            "api_key".to_string(),
            &Cfg {
                models: vec!["test/test".to_string()],
                is_private: true,
                ..Default::default()
            },
            vec![], // messages
            vec![], // tools
            0,
            &Env::default(),
        )
        .unwrap();
        let string_reader = crate::common::buf_read::StringReader {
            data: test_data,
            pos: 0,
        };
        let buf_reader: Box<dyn PromptReader> = Box::new(OrtBufReader::new(string_reader));
        active_prompt.reader = Some(buf_reader);
        active_prompt.start = Some(time::Ticks::now());

        // First the Start event
        let Ok(Some(events)) = active_prompt.next() else {
            panic!("Missing Start event");
        };
        assert_matches!(events.first(), Some(Response::Start));
        // The first annotation comes out here, not 100% sure why
        assert_matches!(events[1], Response::Annotation(_));

        // Then some Annotation from the web_search
        let mut num_annotations = 0;
        let mut has_seen_the_bug = false;
        'events: while let Ok(Some(events)) = active_prompt.next() {
            if events.is_empty() && !has_seen_the_bug {
                // OpenRouter web_search seems to truncte citations sometimes
                // JSON parser probably needs to handle it.
                // Currently returns an error and we get empty event vec
                has_seen_the_bug = true;
                continue;
            }
            for event in events {
                match event {
                    Response::Annotation(_) => {
                        num_annotations += 1;
                    }
                    Response::Think(ThinkEvent::Start) => {
                        // Once we reach Think Start annotations are over
                        break 'events;
                    }
                    other => {
                        panic!("Unexpected event: {other:?}");
                    }
                }
            }
        }
        assert_eq!(num_annotations, 47);
        if has_seen_the_bug {
            // TODO
            crate::eprint_string(c"The citation truncation bug is not fixed", "");
        }

        // Then some Think(Content)
        let mut num_think = 0;
        while let Ok(Some(events)) = active_prompt.next() {
            match events.first().unwrap() {
                Response::Think(ThinkEvent::Content(_)) => num_think += 1,
                other => {
                    panic!("Unexpected event: {other:?}");
                }
            }
            assert_eq!(events.len(), 1);
        }
        assert_eq!(num_think, 296);

        // At the point I interrupted the stream because of the citation truncated
        // bug, so no other events.
    }
}
