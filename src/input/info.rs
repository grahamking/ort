//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

use core::net::{IpAddr, Ipv4Addr, SocketAddr};

extern crate alloc;
use alloc::string::String;

use crate::{
    Context, OrtResult, Read, Write, chunked,
    common::{buf_read, config, resolver},
    http,
    input::args,
    json_parser,
};
use crate::{ErrorKind, ort_error};

pub fn run<W: Write + Send>(
    api_key: &str,
    cfg: &config::Cfg,
    opts: args::InfoOpts,
    w: &mut W,
) -> OrtResult<()> {
    let (host, port, base_path) = http::split_url(&cfg.base_url);
    let addrs = if cfg.dns.is_empty() {
        let ips = unsafe { resolver::resolve(host).context("resolver::resolve")? };
        ips.into_iter()
            .map(|ip| SocketAddr::new(IpAddr::V4(ip), port))
            .collect()
    } else {
        cfg.dns
            .iter()
            .map(|a| {
                let ip_addr = a.parse::<Ipv4Addr>().unwrap();
                SocketAddr::new(IpAddr::V4(ip_addr), port)
            })
            .collect()
    };
    let reader =
        http::info(api_key, host, base_path, &opts.model_id, addrs).context("list_models")?;
    let mut reader = buf_read::OrtBufReader::new(reader);
    let response_body = http::skip_header(&mut reader)?;

    if opts.is_json {
        // The full JSON. User should use `jq` or similar to pretty it.
        match response_body {
            http::ResponseBody::Chunked => {
                // normal case
                const MAX_CHUNK_SIZE: usize = 128 * 1024;
                let mut chunked = chunked::read::<_, MAX_CHUNK_SIZE>(reader);
                while let Some(chunk) = chunked.next_chunk() {
                    let chunk = chunk?;
                    w.write_all(chunk.as_bytes()).context("write info JSON")?;
                }
            }
            http::ResponseBody::ContentLength(len) => {
                let mut remaining = len;
                let mut buf: [u8; 4096] = [0; 4096];
                while remaining > 0 {
                    let n = remaining.min(buf.len());
                    reader.read_exact(&mut buf[..n]).context("read info body")?;
                    w.write_all(&buf[..n]).context("write info JSON")?;
                    remaining -= n;
                }
            }
            http::ResponseBody::UntilEof => {
                let mut buf: [u8; 4096] = [0; 4096];
                loop {
                    let bytes_read = reader.read(&mut buf).context("read info body")?;
                    if bytes_read == 0 {
                        break;
                    }
                    w.write_all(&buf[..bytes_read])
                        .context("write models JSON")?;
                }
            }
        }
        w.flush().context("flush info JSON")?;
    } else {
        match response_body {
            http::ResponseBody::Chunked => {
                // Normal case, it's always chunked right now

                // Full output should be about 2K, so read it all into memory
                let mut output = String::with_capacity(4096);
                const MAX_CHUNK_SIZE: usize = 128 * 1024;
                let mut chunked = chunked::read::<_, MAX_CHUNK_SIZE>(reader);
                while let Some(chunk) = chunked.next_chunk() {
                    let chunk = chunk?;
                    output.push_str(chunk);
                }

                // Check we have expected format
                if !(output.starts_with(r#"{"data":"#) && output.ends_with("}}}")) {
                    return Err(ort_error(ErrorKind::FormatError, "unexpected info format"));
                }
                // The whole thing is wrapped in a "data" field, drop that to
                // simplify parsing
                let end_pos = output.len() - 1;
                let info = &output[8..end_pos];

                // crate::print_string(c"", info);

                let mut modality = None;
                let mut default_effort = None;
                let mut supported_efforts_raw = None;

                // Pull out the objects
                let mut objects = [
                    json_parser::JsonField::new_raw("architecture"),
                    json_parser::JsonField::new_raw("reasoning"),
                    json_parser::JsonField::new_raw("links"),
                ];
                json_parser::autoparser(info, &mut objects)?;

                // Now the fields
                if let Some(arch) = objects[0].get_raw() {
                    let mut fields = [json_parser::JsonField::new_simple_string("modality")];
                    json_parser::autoparser(&arch, &mut fields)?;
                    modality = fields[0].get_string();
                }
                if let Some(reasoning) = objects[1].get_raw() {
                    let mut fields = [
                        json_parser::JsonField::new_simple_string("default_effort"),
                        json_parser::JsonField::new_vec_raw("supported_efforts"),
                    ];
                    json_parser::autoparser(&reasoning, &mut fields)?;
                    default_effort = fields[0].get_string();
                    supported_efforts_raw = fields[1].get_vec_raw();
                }

                // TODO build string, issue one syscall

                w.write(opts.model_id.as_bytes())?;
                w.write_str("\n\n")?;
                if let Some(se) = supported_efforts_raw {
                    w.write(se.join(", ").replace('"', "").as_bytes())?;
                    w.write_str(" (")?;
                    if let Some(de) = default_effort {
                        w.write(de.as_bytes())?;
                    } else {
                        w.write_str("No default effort")?;
                    }
                    w.write_str(")\n")?;
                }
                if let Some(m) = modality {
                    w.write(m.as_bytes())?;
                }
                w.write_char('\n')?;
            }
            http::ResponseBody::ContentLength(_len) => {
                crate::print_string(c"ContentLength not supported for 'info'", "");
                /*
                let mut models = vec![0u8; len];
                reader.read_exact(&mut models).context("read models body")?;
                let models = core::str::from_utf8(&models)
                    .map_err(|_| ort_error(ErrorKind::FormatError, "models JSON utf8"))?;
                push_slugs(models, &mut slugs, &mut total_slug_len);
                */
            }
            http::ResponseBody::UntilEof => {
                crate::print_string(c"UntilEof not supported for 'info'", "");
                /*
                let mut models = Vec::with_capacity(512 * 1024);
                let mut buf: [u8; 4096] = [0; 4096];
                loop {
                    let bytes_read = reader.read(&mut buf).context("read models body")?;
                    if bytes_read == 0 {
                        break;
                    }
                    models.extend_from_slice(&buf[..bytes_read]);
                }
                let models = core::str::from_utf8(&models)
                    .map_err(|_| ort_error(ErrorKind::FormatError, "models JSON utf8"))?;
                push_slugs(models, &mut slugs, &mut total_slug_len);
                */
            }
        };
    }
    Ok(())
}
