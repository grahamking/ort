//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::sync::mpsc::Receiver;
use std::{fs::File, io::Write};

use ort::{Response, ThinkEvent};

use crate::config;

const BOLD_START: &str = "\x1b[1m";
const BOLD_END: &str = "\x1b[0m";
const BACK_ONE: &str = "\x1b[1D";
const CURSOR_OFF: &str = "\x1b[?25l";
const CURSOR_ON: &str = "\x1b[?25h";
const CLEAR_LINE: &str = "\x1b[2K";

const SPINNER: [u8; 4] = [b'|', b'/', b'-', b'\\'];

pub struct Writer {
    pub model_name: String,
    pub save_to_file: bool,
    pub is_pipe_output: bool,
    pub is_quiet: bool,
    pub show_reasoning: bool,
}

impl Writer {
    pub fn run(&self, rx: Receiver<Response>) -> anyhow::Result<()> {
        let filepath;
        let mut file: Box<dyn Write> = if self.save_to_file {
            let cache_dir = config::cache_dir()?;
            let path = cache_dir.join(format!("{}.txt", slug(&self.model_name)));
            let f = File::create(&path)?;
            filepath = Some(path);
            Box::new(f)
        } else {
            filepath = None;
            Box::new(std::io::sink()) // /dev/null
        };

        let stdout = std::io::stdout();
        let mut handle = stdout.lock();

        // For debug
        //let mut s = String::new();
        //let mut handle = std::io::Cursor::new(unsafe { s.as_bytes_mut() });

        if !self.is_pipe_output {
            let _ = write!(handle, "\n{CURSOR_OFF}Connecting...\r");
            let _ = handle.flush();
        }

        let mut spindx = 0;
        while let Ok(data) = rx.recv() {
            match data {
                Response::Start => {
                    if !self.is_pipe_output {
                        let _ = write!(handle, "{BOLD_START}Processing...{BOLD_END} \r");
                        let _ = handle.flush();
                    }
                }
                Response::Think(think) => {
                    if self.show_reasoning {
                        match think {
                            ThinkEvent::Start => {
                                if self.is_pipe_output {
                                    let _ = write!(handle, "<think>");
                                } else {
                                    let _ = write!(handle, "{BOLD_START}<think>{BOLD_END}");
                                }
                                let _ = write!(file, "<think>");
                            }
                            ThinkEvent::Content(s) => {
                                let _ = write!(handle, "{s}");
                                let _ = handle.flush();
                                let _ = write!(file, "{s}");
                            }
                            ThinkEvent::Stop => {
                                if self.is_pipe_output {
                                    let _ = write!(handle, "</think>\n\n");
                                } else {
                                    let _ = write!(handle, "{BOLD_START}</think>{BOLD_END}\n\n");
                                }
                                let _ = write!(file, "</think>");
                            }
                        }
                    } else if !self.is_pipe_output {
                        match think {
                            ThinkEvent::Start => {
                                let _ = write!(handle, "{BOLD_START}Thinking...{BOLD_END}  ");
                                let _ = handle.flush();
                            }
                            ThinkEvent::Content(_) => {
                                let _ = write!(
                                    handle,
                                    "{}{BACK_ONE}",
                                    SPINNER[spindx % SPINNER.len()] as char
                                );
                                let _ = handle.flush();
                                spindx += 1;
                            }
                            ThinkEvent::Stop => {
                                // Erase the Thinking line
                                let _ = write!(handle, "{CLEAR_LINE}\r");
                                let _ = handle.flush();
                            }
                        }
                    }
                }
                Response::Content(content) => {
                    let _ = write!(handle, "{content}");
                    let _ = handle.flush();
                    let _ = write!(file, "{content}");
                }
                Response::Stats(stats) => {
                    println!();
                    if !self.is_quiet {
                        println!();
                        match &filepath {
                            Some(fp) => println!("Stats: {stats}. Saved to {}", fp.display()),
                            None => println!("Stats: {stats}"),
                        };
                    }
                }
                Response::Error(err) => {
                    if !self.is_pipe_output {
                        let _ = write!(handle, "{CURSOR_ON}");
                        let _ = handle.flush();
                    }
                    anyhow::bail!("{err}");
                }
            }
        }

        if !self.is_pipe_output {
            let _ = write!(handle, "{CURSOR_ON}");
            let _ = handle.flush();
        }
        let _ = writeln!(file);

        Ok(())
    }
}

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or('-')
            } else {
                '-'
            }
        })
        .collect()
}
