//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::env;
use std::io;
use std::io::Read as _;
use std::io::Write as _;
use std::process::ExitCode;

use ort::Response;
use ort::ThinkEvent;

mod config;

const BOLD_START: &str = "\x1b[1m";
const BOLD_END: &str = "\x1b[0m";
const BACK_ONE: &str = "\x1b[1D";
const CURSOR_OFF: &str = "\x1b[?25l";
const CURSOR_ON: &str = "\x1b[?25h";

const SPINNER: [u8; 4] = [b'|', b'/', b'-', b'\\'];

#[derive(Debug)]
enum Cmd {
    List(ListOpts),
    Prompt(ort::PromptOpts),
}

#[derive(Debug)]
struct ListOpts {
    is_json: bool,
}

fn print_usage_and_exit() -> ! {
    eprintln!(
        "Usage: ort [-m <model>] [-s \"<system prompt>\"] [-p <price|throughput|latency>] [-r] [-rr] [-q] <prompt>\n\
Defaults: -m {} ; -s omitted ; -p omitted\n\
Example:\n  ort -p price -m moonshotai/kimi-k2 -s \"Respond like a pirate\" \"Write a limerick about AI\"

See https://github.com/grahamking/ort for full docs.
",
        ort::DEFAULT_MODEL
    );
    std::process::exit(2);
}

fn parse_args() -> Cmd {
    let args: Vec<String> = env::args().collect();
    // args[0] is program name
    if args.len() == 1 {
        print_usage_and_exit();
    }

    if args[1].as_str() == "list" {
        parse_list(args)
    } else {
        parse_prompt(args)
    }
}

fn parse_list(args: Vec<String>) -> Cmd {
    let mut is_json = false;

    let mut i = 2;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-json" => is_json = true,
            x => {
                eprintln!("Invalid list argument: {x}");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    Cmd::List(ListOpts { is_json })
}

fn parse_prompt(args: Vec<String>) -> Cmd {
    // Only the prompt is required. Everything else can come from config file
    // or default.
    let mut prompt_parts: Vec<String> = Vec::new();

    let mut model: Option<String> = None;
    let mut system: Option<String> = None;
    let mut priority: Option<String> = None;
    let mut quiet: Option<bool> = None;
    let mut enable_reasoning: Option<bool> = None;
    let mut show_reasoning: Option<bool> = None;

    let mut i = 1usize;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => print_usage_and_exit(),
            "-m" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Missing value for -m");
                    print_usage_and_exit();
                }
                model = Some(args[i].clone());
                i += 1;
            }
            "-s" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Missing value for -s");
                    print_usage_and_exit();
                }
                system = Some(args[i].clone());
                i += 1;
            }
            "-p" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Missing value for -p");
                    print_usage_and_exit();
                }
                let val = args[i].clone();
                match val.as_str() {
                    "price" | "throughput" | "latency" => priority = Some(val),
                    _ => {
                        eprintln!("Invalid -p value: must be one of price|throughput|latency");
                        print_usage_and_exit();
                    }
                }
                i += 1;
            }
            "-q" => {
                quiet = Some(true);
                i += 1;
            }
            "-r" => {
                enable_reasoning = Some(true);
                i += 1;
            }
            "-rr" => {
                show_reasoning = Some(true);
                i += 1;
            }
            s if s.starts_with('-') => {
                eprintln!("Unknown flag: {s}");
                print_usage_and_exit();
            }
            _ => {
                // First positional marks the start of the prompt; join the rest verbatim
                prompt_parts.extend_from_slice(&args[i..]);
                break;
            }
        }
    }

    let mut prompt = "".to_string();
    if !prompt_parts.is_empty() {
        prompt = prompt_parts.join(" ");
    };

    let is_pipe_input = unsafe { libc::isatty(libc::STDIN_FILENO) == 0 };
    if is_pipe_input {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer).unwrap();
        prompt.push_str("\n\n");
        prompt.push_str(&buffer);
    }

    if prompt.is_empty() {
        eprintln!("Missing prompt.");
        print_usage_and_exit();
    };

    Cmd::Prompt(ort::PromptOpts {
        model,
        system,
        priority,
        quiet,
        enable_reasoning,
        show_reasoning,
        prompt: Some(prompt),
    })
}

fn main() -> ExitCode {
    // Load ~/.config/ort.json
    let cfg = match config::load() {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("Failed loading config file: {err:#}");
            std::process::exit(1);
        }
    };

    // Fail fast if key missing
    let api_key = match env::var("OPENROUTER_API_KEY") {
        Ok(v) if !v.is_empty() => v,
        _ => match cfg.get_openrouter_key() {
            Some(k) => k,
            None => {
                eprintln!("OPENROUTER_API_KEY is not set.");
                std::process::exit(1);
            }
        },
    };

    let cmd = parse_args(); // handles pipe input

    let cmd_result = match cmd {
        Cmd::Prompt(mut cli_opts) => {
            cli_opts.merge(cfg.prompt_opts.unwrap_or_default());
            run_prompt(&api_key, cli_opts)
        }
        Cmd::List(args) => run_list(&api_key, args),
    };
    match cmd_result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

fn run_list(api_key: &str, opts: ListOpts) -> anyhow::Result<()> {
    let models = ort::list_models(api_key)?;

    if opts.is_json {
        // pretty-print the full JSON
        for m in models {
            println!("{}", serde_json::to_string_pretty(&m).unwrap());
        }
    } else {
        // extract and print canonical_slug fields alphabetically
        let mut slugs: Vec<_> = models
            .into_iter()
            .map(|mut m| m["canonical_slug"].take().as_str().unwrap().to_string())
            .collect();

        slugs.sort();
        slugs.dedup();
        for s in slugs {
            println!("{s}");
        }
    }
    Ok(())
}

fn run_prompt(api_key: &str, opts: ort::PromptOpts) -> anyhow::Result<()> {
    let is_quiet = opts.quiet.unwrap();
    let show_reasoning = opts.show_reasoning.unwrap();

    let rx = ort::prompt(api_key, opts)?;

    let is_pipe_output = unsafe { libc::isatty(libc::STDOUT_FILENO) == 0 };

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    //let mut s = String::new();
    //let mut handle = std::io::Cursor::new(unsafe { s.as_bytes_mut() });

    if !is_pipe_output {
        let _ = write!(handle, "{CURSOR_OFF}Connecting...\r");
        let _ = handle.flush();
    }

    let mut spindx = 0;
    while let Ok(data) = rx.recv() {
        match data {
            Response::Start => {
                if !is_pipe_output {
                    let _ = write!(handle, "{BOLD_START}Processing...{BOLD_END} \r");
                    let _ = handle.flush();
                }
            }
            Response::Think(think) => {
                if show_reasoning {
                    match think {
                        ThinkEvent::Start => {
                            let _ = write!(handle, "<think>");
                        }
                        ThinkEvent::Content(s) => {
                            let _ = write!(handle, "{s}");
                            let _ = handle.flush();
                        }
                        ThinkEvent::Stop => {
                            let _ = write!(handle, "</think>\n\n");
                        }
                    }
                } else if !is_pipe_output {
                    match think {
                        ThinkEvent::Start => {
                            let _ = write!(handle, "{BOLD_START}Thinking...{BOLD_END}  ");
                            let _ = handle.flush();
                        }
                        ThinkEvent::Content(_) => {
                            let _ = write!(handle, "{}{BACK_ONE}", SPINNER[spindx % 4] as char);
                            let _ = handle.flush();
                            spindx += 1;
                        }
                        ThinkEvent::Stop => {
                            // Erase the Thinking line
                            let _ = write!(handle, "\r");
                            let _ = handle.flush();
                        }
                    }
                }
            }
            Response::Content(content) => {
                let _ = write!(handle, "{content}");
                let _ = handle.flush();
            }
            Response::Stats(stats) => {
                println!();
                if !is_quiet {
                    println!();
                    println!("Stats: {stats}");
                }
            }
            Response::Error(err) => {
                if !is_pipe_output {
                    let _ = write!(handle, "{CURSOR_ON}");
                    let _ = handle.flush();
                }
                anyhow::bail!("{err}");
            }
        }
    }

    if !is_pipe_output {
        let _ = write!(handle, "{CURSOR_ON}");
        let _ = handle.flush();
    }

    Ok(())
}
