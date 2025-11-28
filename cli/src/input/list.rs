//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::io::{self, Read as _};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::net::{chunked, http};
use crate::{CancelToken, Context as _, OrtResult, config, ort_from_err};

use super::args::ListOpts;

pub fn run(
    api_key: &str,
    _cancel_token: CancelToken, // TODO use CancelToken
    settings: config::Settings,
    opts: ListOpts,
    mut w: impl io::Write,
) -> OrtResult<()> {
    let models = list_models(api_key, settings.dns).context("list_models")?;

    if opts.is_json {
        // The full JSON. User should use `jq` or similar to pretty it.
        w.write_all(models.as_bytes()).map_err(ort_from_err)?;
        w.flush().map_err(ort_from_err)?;
    } else {
        // Extract and print model ids alphabetically
        let mut slugs: Vec<&str> = models.split(r#""id":""#).skip(1).map(until_quote).collect();
        slugs.sort();
        for s in slugs {
            let _ = writeln!(w, "{s}");
        }
    }
    Ok(())
}

/// Returns raw JSON
fn list_models(api_key: &str, dns: Vec<String>) -> OrtResult<String> {
    let mut reader = if dns.is_empty() {
        http::list_models(api_key, ("openrouter.ai", 443)).map_err(ort_from_err)?
    } else {
        let addrs: Vec<_> = dns
            .into_iter()
            .map(|a| {
                let ip_addr = a.parse::<Ipv4Addr>().unwrap();
                SocketAddr::new(IpAddr::V4(ip_addr), 443)
            })
            .collect();
        http::list_models(api_key, &addrs[..]).map_err(ort_from_err)?
    };
    let is_chunked = http::skip_header(&mut reader)?;
    let mut full = String::with_capacity(512 * 1024);
    if is_chunked {
        chunked::read_to_string(reader, unsafe { full.as_mut_vec() })?;
    } else {
        reader.read_to_string(&mut full).map_err(ort_from_err)?;
    };
    Ok(full)
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
