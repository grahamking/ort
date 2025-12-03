//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::fmt;

extern crate alloc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::{Consumer, Flushable, OrtResult, Response, Stats, ThinkEvent, ort_err};

const BOLD_START: &str = "\x1b[1m";
const BOLD_END: &str = "\x1b[0m";
const BACK_ONE: &str = "\x1b[1D";
const CURSOR_OFF: &str = "\x1b[?25l";
const CURSOR_ON: &str = "\x1b[?25h";
const CLEAR_LINE: &str = "\x1b[2K";

const SPINNER: [u8; 4] = [b'|', b'/', b'-', b'\\'];

pub struct ConsoleWriter<W: fmt::Write + Flushable> {
    pub writer: W, // Must handle ANSI control chars
    pub show_reasoning: bool,
}

impl<W: fmt::Write + Flushable> ConsoleWriter<W> {
    pub fn into_inner(self) -> W {
        self.writer
    }
    pub fn run<const N: usize>(&mut self, mut rx: Consumer<Response, N>) -> OrtResult<Stats> {
        let _ = write!(self.writer, "{CURSOR_OFF}Connecting...\r");
        let _ = self.writer.flush();

        let mut is_first_content = true;
        let mut spindx = 0;
        let mut stats_out = None;
        while let Some(data) = rx.get_next() {
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
                Response::None => {
                    panic!("Response::None means we read the wrong Queue position");
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
    pub fn run<const N: usize>(&mut self, mut rx: Consumer<Response, N>) -> OrtResult<Stats> {
        let mut stats_out = None;
        while let Some(data) = rx.get_next() {
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
                Response::None => {
                    return ort_err("Response::None means we read the wrong Queue position");
                }
            }
        }

        let Some(stats) = stats_out else {
            return ort_err("OpenRouter did not return usage stats");
        };
        Ok(stats)
    }
}

pub struct CollectedWriter {}

impl CollectedWriter {
    pub fn run<const N: usize>(&mut self, mut rx: Consumer<Response, N>) -> OrtResult<String> {
        let mut got_stats = None;
        let mut contents = Vec::with_capacity(1024);
        while let Some(data) = rx.get_next() {
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
                    return ort_err("CollectedWriter".to_string() + &err.to_string());
                }
                Response::None => {
                    return ort_err("Response::None means we read the wrong Queue position");
                }
            }
        }

        let out =
            "--- ".to_string() + &got_stats.unwrap().to_string() + " ---\n" + &contents.join("");
        //let out = format!("--- {} ---\n{}", got_stats.unwrap(), contents.join(""));
        Ok(out)
    }
}
