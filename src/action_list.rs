//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use crate::{CancelToken, OrtResult, config};

use crate::cli::{ArgParseError, Cmd, ListOpts};
use std::io::Write as _;

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

pub fn run(
    api_key: &str,
    _cancel_token: CancelToken, // TODO use CancelToken
    settings: config::Settings,
    opts: ListOpts,
) -> OrtResult<()> {
    let models_iter = crate::list_models(api_key, settings.dns)?;

    if opts.is_json {
        // The full JSON. User should use `jq` or similar to pretty it.
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        for models_json in models_iter {
            let models_json = models_json?;
            let b = models_json.as_bytes();
            if b.is_empty() {
                break;
            }
            if b.len() < 5 {
                // TODO: Do these still happen? I think it was rustls.
                continue;
            }
            handle.write_all(b)?;
            handle.flush()?;
        }
        drop(handle);
    } else {
        // Extract and print model ids alphabetically
        let mut full = String::with_capacity(512 * 1024 * 1024);
        for models_json in models_iter {
            let models_json = models_json?;
            if models_json.is_empty() {
                break;
            }
            if models_json.len() < 5 {
                continue;
            }
            full += &models_json;
        }
        let mut slugs: Vec<&str> = full.split(r#""id":""#).skip(1).map(until_quote).collect();
        slugs.sort();
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
