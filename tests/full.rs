#[test]
fn test_invalid_model_name() {
    let args = ["ort", "-m", "testxyz", "Hello"]
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    let ret = ort::cli::main(args);
    assert_eq!(ret, std::process::ExitCode::SUCCESS);
}
