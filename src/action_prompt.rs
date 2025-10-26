//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::io;
use std::io::Read as _;
use std::io::Write as _;
use std::sync::mpsc;
use std::thread;

use crate::CancelToken;
use crate::OrtResult;
use crate::Priority;
use crate::PromptOpts;
use crate::ReasoningConfig;
use crate::ReasoningEffort;
use crate::config;
use crate::utils;
use crate::writer::Writer as _;

use crate::cli::ArgParseError;
use crate::cli::Cmd;
use crate::cli::print_usage_and_exit;
use crate::multi_channel;
use crate::writer;

const STDIN_FILENO: i32 = 0;

pub fn parse_args(args: &[String]) -> Result<Cmd, ArgParseError> {
    // Only the prompt is required. Everything else can come from config file
    // or default.
    let mut prompt_parts: Vec<String> = Vec::new();

    let mut model: Option<String> = None;
    let mut system: Option<String> = None;
    let mut priority: Option<Priority> = None;
    let mut quiet: Option<bool> = None;
    let mut reasoning: Option<ReasoningConfig> = None;
    let mut show_reasoning: Option<bool> = None;
    let mut provider: Option<String> = None;
    let mut continue_conversation = false;
    let mut merge_config = true;

    let mut i = 1usize;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => print_usage_and_exit(),
            "-m" => {
                i += 1;
                if i >= args.len() {
                    return Err(ArgParseError::new_str("Missing value for -m"));
                }
                model = Some(args[i].clone());
                i += 1;
            }
            "-s" => {
                i += 1;
                if i >= args.len() {
                    return Err(ArgParseError::new_str("Missing value for -s"));
                }
                system = Some(args[i].clone());
                i += 1;
            }
            "-p" => {
                i += 1;
                if i >= args.len() {
                    return Err(ArgParseError::new_str("Missing value for -p"));
                }
                let val = args[i].clone();
                match val.as_str() {
                    // Safety: The 'parse' can handle exactly the three strings we match on
                    "price" | "throughput" | "latency" => priority = Some(val.parse().unwrap()),
                    _ => {
                        return Err(ArgParseError::new_str(
                            "Invalid -p value: must be one of price|throughput|latency",
                        ));
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
                            return Err(ArgParseError::new_str(
                                "Invalid -r value. Must be off|low|medium|high|<num-tokens>",
                            ));
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
            "-pr" => {
                i += 1;
                if i >= args.len() {
                    return Err(ArgParseError::new_str("Missing value for -pr"));
                }
                provider = Some(utils::slug(args[i].as_ref()));
                i += 1;
            }
            "-c" => {
                continue_conversation = true;
                i += 1;
            }
            "-nc" => {
                merge_config = false;
                i += 1;
            }
            s if s.starts_with('-') => {
                return Err(ArgParseError::new(format!("Unknown flag: {s}")));
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

    let is_pipe_input = unsafe { isatty(STDIN_FILENO) == 0 };
    if is_pipe_input {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer).unwrap();
        prompt.push_str("\n\n");
        prompt.push_str(&buffer);
    }

    if prompt.is_empty() {
        return Err(ArgParseError::new_str("Missing prompt."));
    };
    let prompt_opts = PromptOpts {
        prompt: Some(prompt),
        model,
        provider,
        system,
        priority,
        reasoning,
        show_reasoning,
        quiet,
        merge_config,
    };
    if !continue_conversation {
        Ok(Cmd::Prompt(prompt_opts))
    } else {
        Ok(Cmd::ContinueConversation(prompt_opts))
    }
}

pub fn run(
    api_key: &str,
    cancel_token: CancelToken,
    settings: config::Settings,
    opts: PromptOpts,
    messages: Vec<crate::Message>,
    is_pipe_output: bool, // Are we redirecting stdout?
    //w: impl io::Write + Send,
    w: std::io::Stdout,
) -> OrtResult<()> {
    let show_reasoning = opts.show_reasoning.unwrap();
    let is_quiet = opts.quiet.unwrap_or_default();
    //let model_name = opts.common.model.clone().unwrap();

    // Start network connection before almost anything else, this takes time
    let rx_main = crate::prompt(
        api_key,
        cancel_token,
        settings.dns,
        opts.clone(),
        messages.clone(),
    );
    std::thread::yield_now();

    let (tx_stdout, rx_stdout) = mpsc::channel();
    //let (tx_file, rx_file) = mpsc::channel();
    let (tx_last, rx_last) = mpsc::channel();

    let jh_broadcast = multi_channel::broadcast(rx_main, vec![tx_stdout, tx_last]);

    //let cache_dir = config::cache_dir()?;
    //let path = cache_dir.join(format!("{}.txt", utils::slug(&model_name)));
    //let path_display = path.display().to_string();

    let writer = Box::new(w);
    thread::scope(|scope| {
        let mut handles = vec![];
        let jh_stdout = scope.spawn(move || -> OrtResult<()> {
            let (stats, mut w) = if is_pipe_output {
                let mut fw = writer::FileWriter {
                    writer,
                    show_reasoning,
                };
                let stats = fw.run(rx_stdout)?;
                let w = fw.into_inner();
                (stats, w)
            } else {
                let mut cw = writer::ConsoleWriter {
                    writer,
                    show_reasoning,
                };
                let stats = cw.run(rx_stdout)?;
                let w = cw.into_inner();
                (stats, w)
            };
            let _ = writeln!(w);
            if !is_quiet {
                //if settings.save_to_file {
                //    let _ = write!(handle, "\nStats: {stats}. Saved to {path_display}\n");
                //} else {
                let _ = write!(w, "\nStats: {stats}\n");
                //}
            }

            Ok(())
        });
        handles.push(jh_stdout);

        if settings.save_to_file {
            /*
            let jh_file = thread::spawn(move || -> OrtResult<()> {
                let f = File::create(&path)?;
                let mut file_writer = writer::FileWriter {
                    writer: Box::new(f),
                    show_reasoning,
                };
                let stats = file_writer.run(rx_file)?;
                let f = file_writer.inner();
                let _ = writeln!(f);
                if !is_quiet {
                    let _ = write!(f, "\nStats: {stats}\n");
                }
                Ok(())
            });
            handles.push(jh_file);
            */

            let jh_last = scope.spawn(move || -> OrtResult<()> {
                let mut last_writer = writer::LastWriter::new(opts, messages)?;
                last_writer.run(rx_last)?;
                Ok(())
            });
            handles.push(jh_last);
        }

        for h in handles {
            if let Err(err) = h.join().unwrap() {
                eprintln!("\nThread error: {err}");
                // The errors are all the same so only print the first
                break;
            }
        }
    });
    if let Err(err) = jh_broadcast.join().unwrap() {
        eprintln!("\nThread error: {err}");
    }

    Ok(())
}

unsafe extern "C" {
    pub fn isatty(fd: i32) -> i32;
}
