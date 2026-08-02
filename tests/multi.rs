//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

use ort_openrouter_cli::cli;

mod shared;

#[test]
fn test_multi() {
    // Pick cheap or reliably free ones
    // google/gemma-3-4b-it
    // mistralai/mistral  # hitting rate limit
    const MODEL1: &str = "nvidia/nemotron-nano-9b-v2:free";
    const MODEL2: &str = "meta-llama/llama-3.1-8b-instruct";
    const MODELS: [&str; 2] = [MODEL1, MODEL2];

    let mut out = Vec::new();

    // Need "-p latency" to avoid Chutes which can be very slow
    let args: Vec<String> = ["ort", "-m", MODEL1, "-m", MODEL2, "-p", "latency", "Hello"]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let ret = cli::main(&args, shared::env(), false, &mut out);
    match ret {
        Ok(0) => {} // success
        Ok(x) => {
            panic!("cli::main exit code {x} expected 0");
        }
        Err(err) => {
            panic!("cli::main error: {}", err.as_string());
        }
    }

    let contents = String::from_utf8_lossy(&out);
    if contents.is_empty() {
        panic!("No output from 'ort'. Try it at the command line.");
    }
    let mut seen_hello = 0;
    let mut seen_model = [false; 2];
    for line in contents.lines() {
        if shared::HELLO.iter().any(|hello| line.contains(hello)) {
            seen_hello += 1;
        }
        for (idx, model) in MODELS.iter().enumerate() {
            if line.contains(model) {
                seen_model[idx] = true;
                break;
            }
        }
    }

    assert_eq!(seen_hello, 2, "Did not see hello response twice");
    assert!(
        seen_model.iter().all(|&b| b),
        "Did not see all the model names"
    );
}
