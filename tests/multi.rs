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
    // ibm-granite/granite-4.1-8b
    // google/gemma-3-4b-it
    const MODEL1: &str = "mistralai/mistral-nemo";
    const MODEL2: &str = "nvidia/nemotron-nano-9b-v2:free";
    const MODEL3: &str = "meta-llama/llama-3.1-8b-instruct";
    const MODELS: [&str; 3] = [MODEL1, MODEL2, MODEL3];

    let mut out = Vec::new();

    // Need "-p latency" to avoid Chutes which can be very slow
    let args: Vec<String> = [
        "ort", "-m", MODEL1, "-m", MODEL2, "-m", MODEL3, "-p", "latency", "Hello",
    ]
    .into_iter()
    .map(|s| s.to_string())
    .collect();
    let ret = cli::main(&args, shared::env(), false, &mut out);
    assert!(matches!(ret, Ok(0)));

    let contents = String::from_utf8_lossy(&out);
    if contents.is_empty() {
        panic!("No output from 'ort'. Try it at the command line.");
    }
    let mut seen_hello = 0;
    let mut seen_model = [false; 3];
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

    assert_eq!(seen_hello, 3, "Did not see hello response three times");
    assert!(
        seen_model.iter().all(|&b| b),
        "Did not see all the model names"
    );
}
