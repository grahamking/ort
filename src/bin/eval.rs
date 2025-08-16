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

use anyhow::Context as _;
use std::io::Write as _;

use std::env;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ort::Response;

/// Write output to here, one directory per prompt
const EVALS_ROOT: &str = "/home/graham/evals/results/";

/// Model IDs one per line. To enable reasoning put anything else on the line after a space.
/// Max 15 models! That's how many cat names we have. Add names if you have more models.
const MODELS_FILE: &str = "/home/graham/evals/models-final.txt";

/// Prompt to use, one per line
const PROMPTS_FILE: &str = "/home/graham/evals/prompts-final.txt";

/// Secret alises for the models so you can blind compare them
const CAT_NAMES: [&str; 15] = [
    "Luna", "Milo", "Oliver", "Bella", "Chloe", "Simba", "Nala", "Kitty", "Shadow", "Gizmo",
    "Coco", "Misty", "Tiger", "Salem", "Pumpkin",
];

/// System prompt, change at will
const SYSTEM_PROMPT: &str =
    "Make your answer concise but complete. No yapping. Direct professional tone. No emoji.";

fn main() -> anyhow::Result<()> {
    let api_key = match env::var("OPENROUTER_API_KEY") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            eprintln!("OPENROUTER_API_KEY is not set.");
            std::process::exit(1);
        }
    };

    let models: Vec<String> = fs::read_to_string(MODELS_FILE)
        .context(MODELS_FILE)?
        .lines()
        .map(str::to_string)
        .collect();
    let prompts: Vec<String> = fs::read_to_string(PROMPTS_FILE)
        .context(PROMPTS_FILE)?
        .lines()
        .map(str::to_string)
        .collect();

    for (eval_num, prompt) in prompts.into_iter().enumerate() {
        run_prompt(&api_key, eval_num, &prompt, &models)?;
    }
    Ok(())
}

fn run_prompt(
    api_key: &str,
    eval_num: usize,
    prompt: &str,
    models: &[String],
) -> anyhow::Result<()> {
    println!("\n-- {prompt}");

    // Randomize so their names are not predictable
    let mut names = CAT_NAMES.iter().map(|n| n.to_string()).collect();
    shuffle_strings(&mut names);

    // Make the eval directory
    let dir_name = PathBuf::from(EVALS_ROOT).join(format!("eval{eval_num}"));
    fs::create_dir_all(&dir_name)?;

    // Save the prompt
    let prompt_path = Path::new(&dir_name).join("prompt");
    fs::write(prompt_path, format!("{prompt}\n"))?;

    let mut key_file = File::create(Path::new(&dir_name).join("key"))?;
    for (model_num, model) in models.into_iter().enumerate() {
        let parts: Vec<_> = model.split(' ').collect();
        let enable_reasoning = parts.len() > 1;
        println!(
            "{} {}",
            parts[0],
            if enable_reasoning { "reasoning" } else { "" }
        );

        let prompt_opts = ort::PromptOpts {
            // We clone the model name because the struct takes ownership of the String.
            model: parts[0].to_string(),
            system: Some(SYSTEM_PROMPT.to_string()),
            prompt: prompt.to_string(),
            priority: None,
            quiet: false,
            show_reasoning: true,
            enable_reasoning,
        };

        let cat_name = &names[model_num];
        let mut out = File::create(Path::new(&dir_name).join(format!("{cat_name}.txt")))?;

        let rx = ort::prompt(api_key, prompt_opts)?;
        while let Ok(data) = rx.recv() {
            match data {
                Response::Error(err) => {
                    anyhow::bail!("{err}");
                }
                Response::Stats(stats) => {
                    let _ = write!(key_file, "{cat_name}: {stats}\n");
                }
                Response::Content(content) => {
                    let _ = write!(out, "{content}");
                }
            }
        }
        let _ = write!(out, "\n");
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

fn shuffle_strings(vec: &mut Vec<String>) {
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
