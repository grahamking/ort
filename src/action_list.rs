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
    let models = ort::list_models(api_key)?;

    if opts.is_json {
        // pretty-print the full JSON
        for m in models {
            println!("{}", serde_json::to_string_pretty(&m).unwrap());
        }
    } else {
        // extract and print canonical_slug fields alphabetically
        let mut slugs: Vec<_> = models
            .into_iter()
            .map(|mut m| m["canonical_slug"].take().as_str().unwrap().to_string())
            .collect();

        slugs.sort();
        slugs.dedup();
        for s in slugs {
            println!("{s}");
        }
    }
    Ok(())
}
