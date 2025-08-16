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

const DEFAULT_MODEL: &str = "openai/gpt-oss-20b:free";

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
        DEFAULT_MODEL
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
    let mut model = DEFAULT_MODEL.to_string();
    let mut system: Option<String> = None;
    let mut priority: Option<String> = None;
    let mut i = 1usize;
    let mut prompt_parts: Vec<String> = Vec::new();
    let mut quiet = false;
    let mut enable_reasoning = false;
    let mut show_reasoning = false;

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
                model = args[i].clone();
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
                quiet = true;
                i += 1;
            }
            "-r" => {
                enable_reasoning = true;
                i += 1;
            }
            "-rr" => {
                show_reasoning = true;
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

    let is_pipe = unsafe { libc::isatty(libc::STDIN_FILENO) == 0 };
    let prompt = if !prompt_parts.is_empty() {
        prompt_parts.join(" ")
    } else if is_pipe {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer).unwrap();
        buffer
    } else {
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
        prompt,
    })
}

fn main() -> ExitCode {
    // Fail fast if key missing
    let api_key = match env::var("OPENROUTER_API_KEY") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("OPENROUTER_API_KEY is not set.");
            std::process::exit(1);
        }
    };

    let cmd = parse_args(); // handles pipe input
    let cmd_result = match cmd {
        Cmd::Prompt(args) => run_prompt(&api_key, args),
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
    let is_quiet = opts.quiet;
    let rx = ort::prompt(api_key, opts)?;
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    while let Ok(data) = rx.recv() {
        match data {
            Response::Error(err) => {
                anyhow::bail!("{err}");
            }
            Response::Stats(stats) => {
                println!();
                if !is_quiet {
                    println!();
                    println!("Stats: {stats}");
                }
            }
            Response::Content(content) => {
                let _ = write!(handle, "{content}");
                let _ = handle.flush();
            }
        }
    }

    Ok(())
}
