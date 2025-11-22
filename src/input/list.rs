//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::net::http;
use crate::{CancelToken, Context as _, OrtResult, config};

use super::args::ListOpts;

pub fn run(
    api_key: &str,
    _cancel_token: CancelToken, // TODO use CancelToken
    settings: config::Settings,
    opts: ListOpts,
    mut w: impl io::Write,
) -> OrtResult<()> {
    let models_iter = list_models(api_key, settings.dns).context("list_models")?;

    if opts.is_json {
        // The full JSON. User should use `jq` or similar to pretty it.
        for models_json in models_iter {
            // If the response has invalid UTF-8 (possibly due to BufReader reading partial
            // character?), this will error with "stream did not contain valid UTF-8"
            let models_json = models_json?;
            // TODO: `list_models` should give us bytes not String so we don't have to convert
            // back, and so that we can hand anything openrouter throws at us straight to the
            // terminal.
            let b = models_json.as_bytes();
            if b.is_empty() {
                break;
            }
            if b.len() < 5 {
                // TODO: Do these still happen? I think it was rustls.
                continue;
            }
            w.write_all(b)?;
            w.flush()?;
        }
    } else {
        // Extract and print model ids alphabetically
        let mut full = String::with_capacity(512 * 1024 * 1024);
        for models_json in models_iter {
            // If the response has invalid UTF-8 (possibly due to BufReader reading partial
            // character?), this will error with "stream did not contain valid UTF-8"
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
            let _ = writeln!(w, "{s}");
        }
    }
    Ok(())
}

/// Returns raw JSON
fn list_models(
    api_key: &str,
    dns: Vec<String>,
) -> OrtResult<impl Iterator<Item = Result<String, std::io::Error>>> {
    let reader = if dns.is_empty() {
        http::list_models(api_key, ("openrouter.ai", 443))?
    } else {
        let addrs: Vec<_> = dns
            .into_iter()
            .map(|a| {
                let ip_addr = a.parse::<Ipv4Addr>().unwrap();
                SocketAddr::new(IpAddr::V4(ip_addr), 443)
            })
            .collect();
        http::list_models(api_key, &addrs[..])?
    };
    Ok(http::skip_header(reader)?)
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
