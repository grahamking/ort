//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::env;
use std::fs::File;
use std::io;
use std::io::Read as _;
use std::process::ExitCode;
use std::sync::mpsc;
use std::thread;

use ort::ReasoningConfig;
use ort::ReasoningEffort;

mod config;
mod multi_channel;
mod writer;
use writer::Writer as _;

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
    let mut reasoning: Option<ReasoningConfig> = None;
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
                i += 1;
                let r_cfg = match args[i].as_str() {
                    "off" => ReasoningConfig {
                        enabled: false,
                        ..Default::default()
                    },
                    "low" => ReasoningConfig {
                        enabled: true,
                        effort: Some(ReasoningEffort::Low),
                        ..Default::default()
                    },
                    "medium" | "med" => ReasoningConfig {
                        enabled: true,
                        effort: Some(ReasoningEffort::Medium),
                        ..Default::default()
                    },
                    "high" => ReasoningConfig {
                        enabled: true,
                        effort: Some(ReasoningEffort::High),
                        ..Default::default()
                    },
                    n_str => match n_str.parse::<u32>() {
                        Ok(n) => ReasoningConfig {
                            enabled: true,
                            tokens: Some(n),
                            ..Default::default()
                        },
                        Err(_) => {
                            eprintln!("Invalid -r value. Must be off|low|medium|high|<num-tokens>");
                            print_usage_and_exit();
                        }
                    },
                };
                reasoning = Some(r_cfg);
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
        reasoning,
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
    let save_to_file = true;

    let cmd_result = match cmd {
        Cmd::Prompt(mut cli_opts) => {
            cli_opts.merge(cfg.prompt_opts.unwrap_or_default());
            run_prompt(&api_key, save_to_file, cli_opts)
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

fn run_prompt(api_key: &str, save_to_file: bool, opts: ort::PromptOpts) -> anyhow::Result<()> {
    let is_quiet = opts.quiet.unwrap();
    let show_reasoning = opts.show_reasoning.unwrap();
    let is_pipe_output = unsafe { libc::isatty(libc::STDOUT_FILENO) == 0 };
    let model_name = opts.model.clone().unwrap();

    // Start network connection before almost anything else, this takes time
    let rx_main = ort::prompt(api_key, opts)?;

    let (tx_stdout, rx_stdout) = mpsc::channel();
    let (tx_file, rx_file) = mpsc::channel();
    let jh_broadcast = multi_channel::broadcast(rx_main, vec![tx_stdout, tx_file]);

    let jh_stdout = thread::spawn(move || -> anyhow::Result<()> {
        let stdout = std::io::stdout();
        let handle = stdout.lock();
        let mut stdout_writer: Box<dyn writer::Writer> = if is_pipe_output {
            Box::new(writer::FileWriter {
                writer: Box::new(handle),
                is_quiet,
                show_reasoning,
            })
        } else {
            Box::new(writer::ConsoleWriter {
                writer: Box::new(handle),
                is_quiet,
                show_reasoning,
            })
        };
        stdout_writer.run(rx_stdout)
    });

    let mut handles = vec![jh_broadcast, jh_stdout];

    if save_to_file {
        let jh_file = thread::spawn(move || -> anyhow::Result<()> {
            let cache_dir = config::cache_dir()?;
            let path = cache_dir.join(format!("{}.txt", slug(&model_name)));
            let f = File::create(&path)?;
            let mut file_writer = writer::FileWriter {
                writer: Box::new(f),
                is_quiet,
                show_reasoning,
            };
            file_writer.run(rx_file)
        });
        handles.push(jh_file);
    }

    for h in handles {
        if let Err(err) = h.join().unwrap() {
            eprintln!("Thread error: {err}");
        }
    }

    Ok(())
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
