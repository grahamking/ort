//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::fs::File;
use std::io::Write;
use std::sync::mpsc::Receiver;

use crate::{Message, PromptOpts, Response, Stats, ThinkEvent, config, data::LastData, utils};

const BOLD_START: &str = "\x1b[1m";
const BOLD_END: &str = "\x1b[0m";
const BACK_ONE: &str = "\x1b[1D";
const CURSOR_OFF: &str = "\x1b[?25l";
const CURSOR_ON: &str = "\x1b[?25h";
const CLEAR_LINE: &str = "\x1b[2K";

const SPINNER: [u8; 4] = [b'|', b'/', b'-', b'\\'];

pub trait Writer {
    fn run(&mut self, rx: Receiver<Response>) -> anyhow::Result<Stats>;
    fn inner(&mut self) -> &mut Box<dyn Write>;
}

pub struct ConsoleWriter {
    pub writer: Box<dyn Write>, // Must handle ANSI control chars
    pub show_reasoning: bool,
}

impl Writer for ConsoleWriter {
    fn inner(&mut self) -> &mut Box<dyn Write> {
        &mut self.writer
    }
    fn run(&mut self, rx: Receiver<Response>) -> anyhow::Result<Stats> {
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
                    anyhow::bail!("{err}");
                }
            }
        }

        let _ = write!(self.writer, "{CURSOR_ON}");
        let _ = self.writer.flush();

        let Some(stats) = stats_out else {
            anyhow::bail!("OpenRouter did not return usage stats");
        };
        Ok(stats)
    }
}

pub struct FileWriter {
    pub writer: Box<dyn Write>,
    pub show_reasoning: bool,
}

impl Writer for FileWriter {
    fn inner(&mut self) -> &mut Box<dyn Write> {
        &mut self.writer
    }
    fn run(&mut self, rx: Receiver<Response>) -> anyhow::Result<Stats> {
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
                    anyhow::bail!("{err}");
                }
            }
        }

        let Some(stats) = stats_out else {
            anyhow::bail!("OpenRouter did not return usage stats");
        };
        Ok(stats)
    }
}

pub struct LastWriter {
    w: Box<dyn Write>,
    data: LastData,
}

impl LastWriter {
    pub fn new(opts: PromptOpts, messages: Vec<Message>) -> anyhow::Result<Self> {
        let last_filename = format!("last-{}.json", utils::tmux_pane_id());
        let last_path = config::cache_dir()?.join(last_filename);
        let last_file = Box::new(File::create(last_path)?);
        let data = LastData { opts, messages };
        Ok(LastWriter { data, w: last_file })
    }
}

impl Writer for LastWriter {
    fn inner(&mut self) -> &mut Box<dyn Write> {
        &mut self.w
    }

    fn run(&mut self, rx: Receiver<Response>) -> anyhow::Result<Stats> {
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
                    anyhow::bail!("LastWriter: {err}");
                }
            }
        }

        let message = Message::assistant(contents.join(""));
        self.data.messages.push(message);

        self.data.to_json_writer(&mut self.w)?;
        let _ = self.w.flush();

        Ok(Stats::default()) // Stats is not used
    }
}
