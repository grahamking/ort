//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::fmt;
use std::fs::File;
use std::sync::mpsc::Receiver;

use crate::{
    LastData, Message, OrtResult, PromptOpts, Response, ThinkEvent, common::utils, config, ort_err,
    ort_from_err, stats::Stats,
};

const BOLD_START: &str = "\x1b[1m";
const BOLD_END: &str = "\x1b[0m";
const BACK_ONE: &str = "\x1b[1D";
const CURSOR_OFF: &str = "\x1b[?25l";
const CURSOR_ON: &str = "\x1b[?25h";
const CLEAR_LINE: &str = "\x1b[2K";

const SPINNER: [u8; 4] = [b'|', b'/', b'-', b'\\'];

/// Adapter that lets us use any `std::io::Write` as a UTF-8-only `fmt::Write`.
pub struct IoFmtWriter<W: std::io::Write> {
    inner: W,
}

impl<W: std::io::Write> IoFmtWriter<W> {
    pub fn new(inner: W) -> Self {
        IoFmtWriter { inner }
    }

    pub fn into_inner(self) -> W {
        self.inner
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl<W: std::io::Write> fmt::Write for IoFmtWriter<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        std::io::Write::write_all(&mut self.inner, s.as_bytes()).map_err(|_| fmt::Error)
    }

    fn write_char(&mut self, c: char) -> fmt::Result {
        let mut buf = [0u8; 4];
        let slice = c.encode_utf8(&mut buf);
        std::io::Write::write_all(&mut self.inner, slice.as_bytes()).map_err(|_| fmt::Error)
    }
}

pub trait Flushable {
    fn flush(&mut self) -> OrtResult<()>;
}

impl<W: std::io::Write> Flushable for IoFmtWriter<W> {
    fn flush(&mut self) -> OrtResult<()> {
        std::io::Write::flush(&mut self.inner).map_err(ort_from_err)
    }
}

impl Flushable for String {
    fn flush(&mut self) -> OrtResult<()> {
        Ok(())
    }
}

impl<T: Flushable + ?Sized> Flushable for &mut T {
    fn flush(&mut self) -> OrtResult<()> {
        (**self).flush()
    }
}

pub struct ConsoleWriter<W: fmt::Write + Flushable> {
    pub writer: W, // Must handle ANSI control chars
    pub show_reasoning: bool,
}

impl<W: fmt::Write + Flushable> ConsoleWriter<W> {
    pub fn into_inner(self) -> W {
        self.writer
    }
    pub fn run(&mut self, rx: Receiver<Response>) -> OrtResult<Stats> {
        let _ = write!(self.writer, "{CURSOR_OFF}Connecting...\r");
        let _ = self.writer.flush();

        let mut is_first_content = true;
        let mut spindx = 0;
        let mut stats_out = None;
        while let Ok(data) = rx.recv() {
            match data {
                Response::Start => {
                    let _ = write!(self.writer, "{BOLD_START}Processing...{BOLD_END} \r");
                    let _ = self.writer.flush();
                }
                Response::Think(think) => {
                    if self.show_reasoning {
                        match think {
                            ThinkEvent::Start => {
                                let _ = write!(self.writer, "{BOLD_START}<think>{BOLD_END}");
                            }
                            ThinkEvent::Content(s) => {
                                let _ = write!(self.writer, "{s}");
                                let _ = self.writer.flush();
                            }
                            ThinkEvent::Stop => {
                                let _ = writeln!(self.writer, "{BOLD_START}</think>{BOLD_END}");
                            }
                        }
                    } else {
                        match think {
                            ThinkEvent::Start => {
                                let _ = write!(self.writer, "{BOLD_START}Thinking...{BOLD_END}  ");
                                let _ = self.writer.flush();
                            }
                            ThinkEvent::Content(_) => {
                                let _ = write!(
                                    self.writer,
                                    "{}{BACK_ONE}",
                                    SPINNER[spindx % SPINNER.len()] as char
                                );
                                let _ = self.writer.flush();
                                spindx += 1;
                            }
                            ThinkEvent::Stop => {}
                        }
                    }
                }
                Response::Content(content) => {
                    if is_first_content {
                        // Erase the Processing or Thinking line
                        let _ = write!(self.writer, "\r{CLEAR_LINE}\n");
                        is_first_content = false;
                    }
                    let _ = write!(self.writer, "{content}");
                    let _ = self.writer.flush();
                }
                Response::Stats(stats) => {
                    stats_out = Some(stats);
                }
                Response::Error(err) => {
                    let _ = write!(self.writer, "{CURSOR_ON}");
                    let _ = self.writer.flush();
                    return ort_err(err.to_string());
                }
            }
        }

        let _ = write!(self.writer, "{CURSOR_ON}");
        let _ = self.writer.flush();

        let Some(stats) = stats_out else {
            return ort_err("OpenRouter did not return usage stats");
        };
        Ok(stats)
    }
}

pub struct FileWriter<W: fmt::Write> {
    pub writer: W,
    pub show_reasoning: bool,
}

impl<W: fmt::Write> FileWriter<W> {
    pub fn into_inner(self) -> W {
        self.writer
    }
    pub fn run(&mut self, rx: Receiver<Response>) -> OrtResult<Stats> {
        let mut stats_out = None;
        while let Ok(data) = rx.recv() {
            match data {
                Response::Start => {}
                Response::Think(think) => {
                    if self.show_reasoning {
                        match think {
                            ThinkEvent::Start => {
                                let _ = write!(self.writer, "<think>");
                            }
                            ThinkEvent::Content(s) => {
                                let _ = write!(self.writer, "{s}");
                            }
                            ThinkEvent::Stop => {
                                let _ = write!(self.writer, "</think>\n\n");
                            }
                        }
                    }
                }
                Response::Content(content) => {
                    let _ = write!(self.writer, "{content}");
                }
                Response::Stats(stats) => {
                    stats_out = Some(stats);
                }
                Response::Error(err) => {
                    return ort_err(err.to_string());
                }
            }
        }

        let Some(stats) = stats_out else {
            return ort_err("OpenRouter did not return usage stats");
        };
        Ok(stats)
    }
}

pub struct LastWriter {
    w: IoFmtWriter<std::fs::File>,
    data: LastData,
}

impl LastWriter {
    pub fn new(opts: PromptOpts, messages: Vec<Message>) -> OrtResult<Self> {
        let last_filename = format!("last-{}.json", utils::tmux_pane_id());
        let last_path = config::cache_dir()?.join(last_filename);
        let last_file = File::create(last_path).map_err(ort_from_err)?;
        let data = LastData { opts, messages };
        Ok(LastWriter {
            data,
            w: IoFmtWriter::new(last_file),
        })
    }
    pub fn into_inner(self) -> std::fs::File {
        self.w.into_inner()
    }

    pub fn run(&mut self, rx: Receiver<Response>) -> OrtResult<Stats> {
        let mut contents = Vec::with_capacity(1024);
        while let Ok(data) = rx.recv() {
            match data {
                Response::Start => {}
                Response::Think(_) => {}
                Response::Content(content) => {
                    contents.push(content);
                }
                Response::Stats(stats) => {
                    self.data.opts.provider = Some(utils::slug(stats.provider()));
                }
                Response::Error(err) => {
                    return ort_err(format!("LastWriter: {err}"));
                }
            }
        }

        let message = Message::assistant(contents.join(""));
        self.data.messages.push(message);

        // TODO: output should not import input!
        crate::input::to_json::last_data_to_json_writer(&self.data, &mut self.w)?;
        let _ = self.w.flush();

        Ok(Stats::default()) // Stats is not used
    }
}

pub struct CollectedWriter {}

impl CollectedWriter {
    pub fn run(&mut self, rx: Receiver<Response>) -> OrtResult<String> {
        let mut got_stats = None;
        let mut contents = Vec::with_capacity(1024);
        while let Ok(data) = rx.recv() {
            match data {
                Response::Start => {}
                Response::Think(_) => {}
                Response::Content(content) => {
                    contents.push(content);
                }
                Response::Stats(stats) => {
                    got_stats = Some(stats);
                }
                Response::Error(err) => {
                    return ort_err(format!("CollectedWriter: {err}"));
                }
            }
        }

        let out = format!("--- {} ---\n{}", got_stats.unwrap(), contents.join(""));
        Ok(out)
    }
}
