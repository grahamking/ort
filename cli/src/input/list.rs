//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};

use crate::{
    CancelToken, Context as _, ListOpts, OrtBufReader, OrtResult, Read, Settings, Write, chunked,
    http, ort_from_err,
};

pub fn run(
    api_key: &str,
    _cancel_token: CancelToken, // TODO use CancelToken
    settings: Settings,
    opts: ListOpts,
    mut w: impl Write,
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
            let _ = w.write(s.as_bytes());
            let _ = w.write(b"\n");
        }
    }
    Ok(())
}

/// Returns raw JSON
fn list_models(api_key: &str, dns: Vec<String>) -> OrtResult<String> {
    let addrs: Vec<_> = if dns.is_empty() {
        ("openrouter.ai", 443).to_socket_addrs().unwrap().collect()
    } else {
        dns.into_iter()
            .map(|a| {
                let ip_addr = a.parse::<Ipv4Addr>().unwrap();
                SocketAddr::new(IpAddr::V4(ip_addr), 443)
            })
            .collect()
    };
    let reader = http::list_models(api_key, addrs).map_err(ort_from_err)?;
    let mut reader = OrtBufReader::new(reader);
    let is_chunked = http::skip_header(&mut reader)?;
    let mut full = String::with_capacity(512 * 1024);
    if is_chunked {
        chunked::read_to_string(reader, unsafe { full.as_mut_vec() })?;
    } else {
        reader
            .read(unsafe { full.as_mut_vec().as_mut_slice() })
            .map_err(ort_from_err)?;
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
