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

    // TODO: have action_prompt use the passed impl Write,
    // and uncomment below to check it

    //out.set_position(0);
    //let line = out.lines().map(|l| l.unwrap()).next().unwrap();
    //assert!(line.contains("testxyz is not a valid model ID"));
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
