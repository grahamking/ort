//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use crate::{ArgParseError, Cmd, ListOpts};

pub fn parse_args(args: &[String]) -> Result<Cmd, ArgParseError> {
    let mut is_json = false;

    let mut i = 2;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-json" => is_json = true,
            x => {
                return Err(ArgParseError::new(format!("Invalid list argument: {x}")));
            }
        }
        i += 1;
    }

    Ok(Cmd::List(ListOpts { is_json }))
}

pub fn run(api_key: &str, opts: ListOpts) -> anyhow::Result<()> {
    let models_json = ort::list_models(api_key)?;

    if opts.is_json {
        // The full JSON. User should use `jq` or similar to pretty it.
        println!("{models_json}");
    } else {
        // Extract and print canonical_slug fields alphabetically
        let mut slugs: Vec<&str> = models_json
            .split(r#""canonical_slug":""#)
            .skip(1)
            .map(until_quote)
            .collect();
        slugs.sort();
        slugs.dedup();
        for s in slugs {
            println!("{s}");
        }
    }
    Ok(())
}

/// The prefix of this string until the first double quote.
/// Slugs never contain a doube quote.
fn until_quote(s: &str) -> &str {
    let mut qp = 0;
    let len = s.len();
    let b = s.as_bytes();
    while b[qp] != b'"' && qp < len {
        qp += 1;
    }
    &s[..qp]
}
