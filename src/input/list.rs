//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::net::{IpAddr, Ipv4Addr, SocketAddr};

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::common::site::Site;
use crate::{
    CancelToken, Context, OrtResult, Read, Write, chunked,
    common::{buf_read, config, resolver},
    http,
    input::args,
};

// As of Feb 18 2026 output takes just over 8k
const MAX_TOTAL_SLUG_LEN: usize = 16 * 1024;

pub fn run(
    api_key: &str,
    _cancel_token: CancelToken, // TODO use CancelToken
    settings: config::Settings,
    opts: args::ListOpts,
    site: &'static Site,
    mut w: impl Write,
) -> OrtResult<()> {
    let addrs: Vec<_> = if settings.dns.is_empty() {
        let c_host = CString::new(site.host).unwrap();
        let ips = unsafe { resolver::resolve(c_host.as_ptr())? };
        ips.into_iter()
            .map(|ip| SocketAddr::new(IpAddr::V4(ip), 443))
            .collect()
    } else {
        settings
            .dns
            .into_iter()
            .map(|a| {
                let ip_addr = a.parse::<Ipv4Addr>().unwrap();
                SocketAddr::new(IpAddr::V4(ip_addr), 443)
            })
            .collect()
    };
    let reader = http::list_models(api_key, site.host, site.list_url, addrs)
        .context("list_models connect")?;
    let mut reader = buf_read::OrtBufReader::new(reader);
    let is_chunked = http::skip_header(&mut reader)?;

    if opts.is_json {
        // The full JSON. User should use `jq` or similar to pretty it.
        if is_chunked {
            // normal case
            const MAX_CHUNK_SIZE: usize = 128 * 1024;
            let mut chunked = chunked::read::<_, MAX_CHUNK_SIZE>(reader);
            while let Some(chunk) = chunked.next_chunk() {
                let chunk = chunk?;
                w.write_all(chunk.as_bytes()).context("write models JSON")?;
            }
        } else {
            // I don't think this happens right now
            let mut buf: [u8; 4096] = [0; 4096];
            loop {
                let bytes_read = reader.read(&mut buf).context("read models body")?;
                if bytes_read == 0 {
                    break;
                }
                w.write_all(&buf[..bytes_read])
                    .context("write models JSON")?;
            }
        }
        w.flush().context("flush models JSON")?;
    } else {
        // 342 models as of Feb 16th 2026
        let mut slugs = Vec::with_capacity(400);
        let mut total_slug_len = 0;
        if is_chunked {
            // normal case, it's always chunked right now
            let mut partial = String::with_capacity(2048);
            const MAX_CHUNK_SIZE: usize = 128 * 1024;
            let mut chunked = chunked::read::<_, MAX_CHUNK_SIZE>(reader);
            while let Some(chunk) = chunked.next_chunk() {
                let chunk = chunk?;
                for (pos, section) in chunk.split(r#""id":""#).enumerate() {
                    let maybe_next_id = if pos == 0 && !partial.is_empty() {
                        // We have a partial from previous iteration
                        partial.push_str(section);
                        until_quote(&partial)
                    } else if pos == 0 {
                        // `split` will return the part _before_ the first ID,
                        // which doesn't have any slugs in it.
                        continue;
                    } else {
                        // normal case, work directly on a ref into the chunk,
                        // no alloc or copy
                        until_quote(section)
                    };
                    match maybe_next_id {
                        Some(slug) => {
                            // The chunk ref is only valid for one iteration, so copy
                            let mut slug_line = String::with_capacity(slug.len() + 1);
                            slug_line.push_str(slug);
                            slug_line.push('\n');
                            total_slug_len += slug_line.len();
                            slugs.push(slug_line);
                            partial.clear();
                        }
                        None => {
                            // The chunk split a model name, save it for next chunk
                            partial.push_str(section);
                        }
                    }
                }
            }
        } else {
            // This case never happens (always chunked) so don't optimize
            let mut models = String::with_capacity(512 * 1024);
            reader
                .read(unsafe { models.as_mut_vec().as_mut_slice() })
                .context("read models body")?;
            for slug in models.split(r#""id":""#).skip(1).filter_map(until_quote) {
                let slug_line = slug.to_string() + "\n";
                total_slug_len += slug_line.len();
                slugs.push(slug_line);
            }
        };

        // Print model ids alphabetically

        slugs.sort();

        if total_slug_len > MAX_TOTAL_SLUG_LEN {
            panic!("Too many models in list. Increase MAX_TOTAL_SLUG_LEN in code.");
        }
        let mut out: [u8; _] = [0u8; MAX_TOTAL_SLUG_LEN];
        let mut ptr_out = out.as_mut_ptr();
        for s in slugs {
            let b = s.as_bytes();
            unsafe {
                core::ptr::copy_nonoverlapping(b.as_ptr(), ptr_out, b.len());
                ptr_out = ptr_out.add(b.len());
            }
        }
        let out_len = unsafe { ptr_out.offset_from(out.as_ptr()) as usize };

        let _ = w.write(&out[..out_len]); // one syscall
    }
    Ok(())
}

/// The prefix of this string until the first double quote.
/// Slugs never contain a doube quote.
fn until_quote(s: &str) -> Option<&str> {
    let mut qp = 0;
    let len = s.len();
    let b = s.as_bytes();
    while qp < len && b[qp] != b'"' {
        qp += 1;
    }
    if qp == len { None } else { Some(&s[..qp]) }
}
