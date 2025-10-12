//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King
//!
//! Blind model evaluations. How else you gonna choose which models to use?
//!
//! Reads model IDs and prompts. Runs every prompt against every model, one at a time.
//! Writes the results in a directory hierarchy.
//! Make a MODELS_FILE and PROMPTS_FILE each with only two entries and try it, you'll see.

use ort::CancelToken;
use ort::Context;
use ort::OrtResult;
use ort::PromptOpts;
use ort::ReasoningConfig;
use ort::ReasoningEffort;
use ort::ThinkEvent;
use ort::ort_err;
use std::io::Write as _;

use std::env;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ort::Response;

/// Secret alises for the models so you can blind compare them
const CAT_NAMES: [&str; 15] = [
    "Luna", "Milo", "Oliver", "Bella", "Chloe", "Simba", "Nala", "Kitty", "Shadow", "Gizmo",
    "Coco", "Misty", "Tiger", "Salem", "Pumpkin",
];

/// System prompt, change at will
const SYSTEM_PROMPT: &str =
    "Make your answer concise but complete. No yapping. Direct professional tone. No emoji.";

fn print_usage_and_exit() -> ! {
    eprintln!(
        "eval --models <models-file> --prompts <prompts-file> --out <dir>\n\
- models-file is a list of model IDs (e.g. 'moonshotai/kimi-k2') one per line.\n\
- prompts-file is a list of prompts one per line\n\
- out dir is a directory to write the results to\n\
See https://github.com/grahamking/ort for full docs.
"
    );
    std::process::exit(2);
}

struct Args {
    /// Model IDs one per line. To enable reasoning put anything else on the line after a space.
    /// Max 15 models! That's how many cat names we have. Add names if you have more models.
    models_file: PathBuf,
    /// Prompt to use, one per line
    prompts_file: PathBuf,
    /// Write output to here, one directory per prompt
    out_dir: PathBuf,
}

fn main() -> OrtResult<()> {
    let api_key = match env::var("OPENROUTER_API_KEY") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("OPENROUTER_API_KEY is not set.");
            std::process::exit(1);
        }
    };
    // This is how we would stop all the running evals on ctrl-c, see main.rs
    let cancel_token = CancelToken::init();

    let args = parse_args();

    let models: Vec<String> = fs::read_to_string(&args.models_file)
        .context("Reading models file")?
        .lines()
        .map(str::to_string)
        .collect();
    let prompts: Vec<String> = fs::read_to_string(&args.prompts_file)
        .context("Reading prompts file")?
        .lines()
        .map(str::to_string)
        .collect();

    for (eval_num, prompt) in prompts.into_iter().enumerate() {
        run_prompt(
            &api_key,
            cancel_token,
            eval_num,
            &prompt,
            &models,
            &args.out_dir,
        )?;
    }
    Ok(())
}

fn parse_args() -> Args {
    let args: Vec<String> = env::args().collect();
    if args.len() != 7 {
        print_usage_and_exit();
    }

    let mut models_file: PathBuf = Default::default();
    let mut prompts_file: PathBuf = Default::default();
    let mut out_dir: PathBuf = Default::default();

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => print_usage_and_exit(),
            "--models" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Missing value for --models");
                    print_usage_and_exit();
                }
                models_file = args[i].clone().into();
                if !models_file.exists() || !models_file.is_file() {
                    eprintln!("File not found: {}", models_file.display());
                    std::process::exit(3);
                }
                i += 1;
            }
            "--prompts" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Missing value for --prompts");
                    print_usage_and_exit();
                }
                prompts_file = args[i].clone().into();
                if !prompts_file.exists() || !prompts_file.is_file() {
                    eprintln!("File not found: {}", prompts_file.display());
                    std::process::exit(3);
                }
                i += 1;
            }
            "--out" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Missing value for --out");
                    print_usage_and_exit();
                }
                out_dir = args[i].clone().into();
                if !out_dir.exists() || !out_dir.is_dir() {
                    eprintln!("Directory does not exist: {}", out_dir.display());
                    std::process::exit(3);
                }
                i += 1;
            }
            s if s.starts_with('-') => {
                eprintln!("Unknown flag: {s}");
                print_usage_and_exit();
            }
            _ => {
                print_usage_and_exit();
            }
        }
    }

    Args {
        models_file,
        prompts_file,
        out_dir,
    }
}

fn run_prompt(
    api_key: &str,
    cancel_token: CancelToken,
    eval_num: usize,
    prompt: &str,
    models: &[String],
    out_dir: &Path,
) -> OrtResult<()> {
    println!("\n-- {prompt}");

    // Randomize so their names are not predictable
    let mut names: Vec<String> = CAT_NAMES.iter().map(|n| n.to_string()).collect();
    shuffle_strings(&mut names);

    // Make the eval directory
    let dir_name = PathBuf::from(out_dir).join(format!("eval{eval_num}"));
    fs::create_dir_all(&dir_name)?;

    // Save the prompt
    let prompt_path = Path::new(&dir_name).join("prompt");
    fs::write(prompt_path, format!("{prompt}\n"))?;

    let mut key_file = File::create(Path::new(&dir_name).join("key"))?;
    for (model_num, model) in models.iter().enumerate() {
        let parts: Vec<_> = model.split(' ').collect();
        let enable_reasoning = parts.len() > 1;
        println!(
            "{} {}",
            parts[0],
            if enable_reasoning { "reasoning" } else { "" }
        );

        let common = PromptOpts {
            prompt: None,
            // We clone the model name because the struct takes ownership of the String.
            model: Some(parts[0].to_string()),
            system: Some(SYSTEM_PROMPT.to_string()),
            priority: None,
            provider: None,
            show_reasoning: Some(true),
            reasoning: Some(ReasoningConfig {
                enabled: true,
                effort: Some(ReasoningEffort::Medium),
                ..Default::default()
            }),
            quiet: Some(false),
            merge_config: true,
        };

        let cat_name = &names[model_num];
        let mut out = File::create(Path::new(&dir_name).join(format!("{cat_name}.txt")))?;

        let messages = vec![ort::Message::user(prompt.to_string())];
        let rx = ort::prompt(api_key, cancel_token, vec![], common, messages);
        while let Ok(data) = rx.recv() {
            if cancel_token.is_cancelled() {
                break;
            }
            match data {
                Response::Start => {}
                Response::Think(think) => match think {
                    ThinkEvent::Start => {
                        let _ = write!(out, "<think>");
                    }
                    ThinkEvent::Content(s) => {
                        let _ = write!(out, "{s}");
                    }
                    ThinkEvent::Stop => {
                        let _ = write!(out, "</think>\n\n");
                    }
                },
                Response::Content(content) => {
                    let _ = write!(out, "{content}");
                }
                Response::Stats(stats) => {
                    let _ = writeln!(key_file, "{cat_name}: {stats}");
                }
                Response::Error(err) => {
                    return ort_err(err.to_string());
                }
            }
        }
        let _ = writeln!(out);
        let _ = out.flush();
        let _ = key_file.flush();
    }

    Ok(())
}

fn xorshift(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn shuffle_strings(vec: &mut [String]) {
    let mut seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let len = vec.len();
    for i in (1..len).rev() {
        let j = (xorshift(&mut seed) % (i as u64 + 1)) as usize;
        vec.swap(i, j);
    }
}
