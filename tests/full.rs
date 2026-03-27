//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use ort_openrouter_cli::cli::{self, Env};

/*
 * TODO: Get the error from the thread
#[test]
fn test_invalid_model_name() {
    let mut out = Cursor::new(vec![]);

    let args = ["ort", "-m", "testxyz", "Hello"]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let ret = cli::main(args, true, &mut out);
    match ret {
        Ok(_) => panic!("Invalid model ID should have produced an error"),
        Err(err) => {
            assert!(err.to_string().contains("testxyz is not a valid model ID"));
        }
    }
}
*/

#[test]
fn test_hello() {
    const MODEL: &str = "openai/gpt-oss-20b";
    let mut out = Vec::new();

    // Need "-p latency" to avoid Chutes which can be very slow
    let args: Vec<String> = ["ort", "-m", MODEL, "-p", "latency", "-r", "low", "Hello"]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let ret = cli::main(&args, env(), false, &mut out);
    assert!(matches!(ret, Ok(0)));

    let contents = String::from_utf8_lossy(&out);
    if contents.is_empty() {
        panic!("No output from 'ort'. Try it at the command line.");
    }
    let mut lines = contents.lines();

    let first_line = lines.next().unwrap();
    assert!(
        first_line.contains("assist") || first_line.contains("help") || first_line.contains("ello"),
        "Invalid line: '{first_line}'"
    );

    let last_line = lines.last().unwrap();
    assert!(
        last_line.starts_with(&format!("Stats: {MODEL}")),
        "Invalid last line: '{last_line}'",
    );
}

#[test]
fn test_list() {
    let mut out = Vec::new();

    let args: Vec<String> = ["ort", "list"].into_iter().map(|s| s.to_string()).collect();
    match cli::main(&args, env(), false, &mut out) {
        // success
        Ok(0) => {}
        Ok(x) => {
            panic!("cli::main exit code: {x}");
        }
        Err(err) => {
            panic!("cli::main err: {}", err.as_string());
        }
    }

    let contents = String::from_utf8_lossy(&out);
    let mut count = 0;
    for line in contents.lines() {
        count += 1;
        // One of the most popular and entrenched models
        if line == "meta-llama/llama-3-70b-instruct" {
            return;
        }
    }
    // List is ordered alphabetically, "m" should have many before
    assert!(count > 20, "Too few lines: {count}");
    panic!("List did not include Llama 3 70B");
}

fn env() -> Env {
    macro_rules! env_str {
        ($name:literal) => {
            std::env::var($name).ok().map(|v| {
                let s: &'static str = v.leak();
                s
            })
        };
    }
    cli::Env {
        HOME: env_str!("HOME"),
        TMUX_PANE: env_str!("TMUX_PANE"),
        XDG_CONFIG_HOME: env_str!("XDG_CONFIG_HOME"),
        XDG_CACHE_HOME: env_str!("XDG_CACHE_HOME"),
        OPENROUTER_API_KEY: env_str!("OPENROUTER_API_KEY"),
        NVIDIA_API_KEY: env_str!("NVIDIA_API_KEY"),
    }
}
