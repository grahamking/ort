//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

/*
 * TODO: Get the error from the thread
#[test]
fn test_invalid_model_name() {
    let mut out = Cursor::new(vec![]);

    let args = ["ort", "-m", "testxyz", "Hello"]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let ret = ort::cli::main(args, true, &mut out);
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
    const MODEL: &str = "openai/gpt-oss-20b:free";
    let mut out = Vec::new();

    let args = ["ort", "-m", MODEL, "-r", "low", "Hello"]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let ret = ort::cli::main(args, false, &mut out).unwrap();
    assert_eq!(ret, 0);

    let contents = String::from_utf8_lossy(&out);
    let mut lines = contents.lines();

    let first_line = lines.next().unwrap();
    assert!(
        first_line.contains("assist") || first_line.contains("help"),
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

    let args = ["ort", "list"].into_iter().map(|s| s.to_string()).collect();
    let ret = ort::cli::main(args, false, &mut out).unwrap();
    assert_eq!(ret, 0);

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
