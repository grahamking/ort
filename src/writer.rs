//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::io::Write;
use std::sync::mpsc::Receiver;

use ort::{Response, ThinkEvent};

const BOLD_START: &str = "\x1b[1m";
const BOLD_END: &str = "\x1b[0m";
const BACK_ONE: &str = "\x1b[1D";
const CURSOR_OFF: &str = "\x1b[?25l";
const CURSOR_ON: &str = "\x1b[?25h";
const CLEAR_LINE: &str = "\x1b[2K";

const SPINNER: [u8; 4] = [b'|', b'/', b'-', b'\\'];

pub trait Writer {
    fn run(&mut self, rx: Receiver<Response>) -> anyhow::Result<()>;
}

pub struct ConsoleWriter {
    pub writer: Box<dyn Write>, // Must handle ANSI control chars
    pub is_quiet: bool,
    pub show_reasoning: bool,
}

impl Writer for ConsoleWriter {
    fn run(&mut self, rx: Receiver<Response>) -> anyhow::Result<()> {
        let _ = write!(self.writer, "\n{CURSOR_OFF}Connecting...\r");
        let _ = self.writer.flush();

        let mut spindx = 0;
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
                                let _ = write!(self.writer, "{BOLD_START}</think>{BOLD_END}\n\n");
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
                            ThinkEvent::Stop => {
                                // Erase the Thinking line
                                let _ = write!(self.writer, "{CLEAR_LINE}\r");
                                let _ = self.writer.flush();
                            }
                        }
                    }
                }
                Response::Content(content) => {
                    let _ = write!(self.writer, "{content}");
                    let _ = self.writer.flush();
                }
                Response::Stats(stats) => {
                    let _ = writeln!(self.writer);
                    if !self.is_quiet {
                        let _ = write!(self.writer, "\nStats: {stats}\n");
                        /* TODO print where file was stored, can only use single line.
                        match &self.filepath {
                            Some(fp) => println!("Stats: {stats}. Saved to {}", fp.display()),
                            None => println!("Stats: {stats}"),
                        };
                        */
                    }
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

        Ok(())
    }
}

pub struct FileWriter {
    pub writer: Box<dyn Write>,
    pub is_quiet: bool,
    pub show_reasoning: bool,
}

impl Writer for FileWriter {
    fn run(&mut self, rx: Receiver<Response>) -> anyhow::Result<()> {
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
                    let _ = writeln!(self.writer);
                    if !self.is_quiet {
                        let _ = write!(self.writer, "\nStats: {stats}\n");
                    }
                }
                Response::Error(err) => {
                    anyhow::bail!("{err}");
                }
            }
        }

        Ok(())
    }
}
