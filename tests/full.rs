//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::io::BufRead;
use std::io::Cursor;

#[test]
fn test_invalid_model_name() {
    let mut out = Cursor::new(vec![]);

    let args = ["ort", "-m", "testxyz", "Hello"]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let ret = ort::cli::main(args, true, &mut out);
    assert_eq!(ret, std::process::ExitCode::SUCCESS);

    //out.set_position(0);
    //let line = out.lines().map(|l| l.unwrap()).next().unwrap();
    //assert!(line.contains("testxyz is not a valid model ID"));
}

#[test]
fn test_hello() {
    const MODEL: &str = "meta-llama/llama-3.3-8b-instruct:free";
    let mut out = Cursor::new(vec![]);

    let args = ["ort", "-m", MODEL, "Hello"]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let ret = ort::cli::main(args, false, &mut out);
    assert_eq!(ret, std::process::ExitCode::SUCCESS);

    out.set_position(0);
    let mut lines = out.lines().map(|l| l.unwrap());

    let first_line = lines.next().unwrap();
    assert!(
        first_line.contains("can I assist"),
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
    let mut out = Cursor::new(vec![]);

    let args = ["ort", "list"].into_iter().map(|s| s.to_string()).collect();
    let ret = ort::cli::main(args, false, &mut out);
    assert_eq!(ret, std::process::ExitCode::SUCCESS);

    out.set_position(0);
    let mut count = 0;
    for line in out.lines() {
        count += 1;
        // One of the most popular and entrenched models
        if line.unwrap() == "meta-llama/llama-3-70b-instruct" {
            return;
        }
    }
    // List is ordered alphabetically, "m" should have many before
    assert!(count > 20, "Too few lines: {count}");
    panic!("List did not include Llama 3 70B");
}
