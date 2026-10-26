//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

use core::net::{IpAddr, Ipv4Addr, SocketAddr};

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

use crate::{
    Context, OrtResult, Write, chunked,
    common::{buf_read, config, resolver},
    http,
    input::args,
    json_parser, utils,
};
use crate::{ErrorKind, ort_error};

pub fn run<W: Write + Send>(
    api_key: &str,
    cfg: &config::Cfg,
    opts: args::InfoOpts,
    w: &mut W,
) -> OrtResult<()> {
    let (host, port, base_path) = http::split_url(&cfg.base_url);
    let addrs: Vec<SocketAddr> = if cfg.dns.is_empty() {
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
    let reader = http::model_info(api_key, host, base_path, &opts.model_id, &addrs)
        .context("list_models")?;
    let mut reader = buf_read::OrtBufReader::new(reader);
    let response_body = http::skip_header(&mut reader)?;

    if opts.is_json {
        super::stream_json(response_body, reader, w)?;
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
                let mut details_url = None;

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
                if let Some(links) = objects[2].get_raw() {
                    let mut fields = [json_parser::JsonField::new_simple_string("details")];
                    json_parser::autoparser(&links, &mut fields)?;
                    details_url = fields[0].get_string();
                }

                // TODO build string, issue one syscall

                w.write(opts.model_id.as_bytes())?;
                w.write_str("\n")?;
                if let Some(se) = supported_efforts_raw {
                    w.write_str("Thinking: ")?;
                    w.write(se.join(", ").replace('"', "").as_bytes())?;
                    w.write_str(" (default: ")?;
                    if let Some(de) = default_effort {
                        w.write(de.as_bytes())?;
                    } else {
                        w.write_str("No default effort")?;
                    }
                    w.write_str(")\n")?;
                }
                if let Some(m) = modality {
                    w.write_str("Modality: ")?;
                    w.write(m.as_bytes())?;
                }
                w.write_char('\n')?;

                if let Some(details_url) = details_url {
                    w.write_str("\n# Providers\n")?;
                    print_providers(api_key, host, &details_url, &addrs, opts.is_json, w)?;
                };
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

fn print_providers<W: Write + Send>(
    api_key: &str,
    host: &str,
    path: &str,
    addrs: &[SocketAddr],
    is_json: bool,
    w: &mut W,
) -> OrtResult<()> {
    let reader = http::model_details(api_key, host, path, addrs).context("info/details")?;
    let mut reader = buf_read::OrtBufReader::new(reader);
    let response_body = http::skip_header(&mut reader)?;

    if is_json {
        // TODO: Currently we never call print_providers in the is_json case
        // because we don't parse out the endpoints URL
        super::stream_json(response_body, reader, w)?;
    } else {
        match response_body {
            http::ResponseBody::Chunked => {
                // Normal case, it's always chunked right now

                // Full output for a private model is < 16K.
                // openai/gpt-oss-120b is 30K
                // deepseek/deepseek-v4.1-flash is 55K
                let mut output = String::with_capacity(64 * 1024);
                const MAX_CHUNK_SIZE: usize = 128 * 1024;
                let mut chunked = chunked::read::<_, MAX_CHUNK_SIZE>(reader);
                while let Some(chunk) = chunked.next_chunk() {
                    let chunk = chunk?;
                    output.push_str(chunk);
                }

                if !(output.starts_with(r#"{"data":"#) && output.ends_with("}}")) {
                    return Err(ort_error(ErrorKind::FormatError, "unexpected info format"));
                }
                // The whole thing is wrapped in a "data" field, drop that to
                // simplify parsing
                let end_pos = output.len() - 1;
                let details = &output[8..end_pos];

                //crate::print_string(c"", details);

                let mut objects = [json_parser::JsonField::new_vec_raw("endpoints")];
                json_parser::autoparser(details, &mut objects)?;
                if let Some(endpoints) = objects[0].get_vec_raw() {
                    for endpoint in endpoints {
                        let mut fields = [
                            json_parser::JsonField::new_simple_string("tag"),
                            json_parser::JsonField::new_int("context_length"),
                            json_parser::JsonField::new_raw("pricing"),
                        ];
                        json_parser::autoparser(&endpoint, &mut fields)?;
                        if let Some(ctx) = fields[1].get_int() {
                            let ctx_str = utils::num_to_human_string(ctx as usize);
                            w.write_str("ctx=")?;
                            w.write(ctx_str.as_bytes())?;
                            w.write_char(' ')?;
                        }
                        if let Some(pricing) = fields[2].get_raw() {
                            // These should be floats but OpenRouter sends as string
                            let mut p_fields = [
                                json_parser::JsonField::new_simple_string("prompt"),
                                json_parser::JsonField::new_simple_string("completion"),
                                json_parser::JsonField::new_simple_string("input_cache_read"),
                            ];
                            json_parser::autoparser(&pricing, &mut p_fields)?;
                            if let Some(p_in_s) = p_fields[0].get_string() {
                                let (mut p_in, _) = utils::parse_f64(p_in_s.as_bytes())?;
                                p_in *= 1_000_000.0;
                                let s = utils::float_to_string(p_in, 2);
                                w.write_str("in=$")?;
                                w.write(s.as_bytes())?;
                                w.write_char(' ')?;
                            }
                            if let Some(p_out_s) = p_fields[1].get_string() {
                                //let (mut p_out, _) = utils::parse_f32(p_out_s.as_bytes()).map(|(a, b)| (a as f64, b))?;
                                let (mut p_out, _) = utils::parse_f64(p_out_s.as_bytes())?;
                                p_out *= 1_000_000.0;
                                let s = utils::float_to_string(p_out, 2);
                                w.write_str("out=$")?;
                                w.write(s.as_bytes())?;
                                w.write_char(' ')?;
                            }
                            if let Some(p_cache_s) = p_fields[2].get_string() {
                                let (mut p_cache, _) = utils::parse_f64(p_cache_s.as_bytes())?;
                                p_cache *= 1_000_000.0;
                                let s = utils::float_to_string(p_cache, 2);
                                w.write_str("cache=$")?;
                                w.write(s.as_bytes())?;
                                w.write_char(' ')?;
                            } else {
                                w.write_str("            ")?; // 12 spaces, preserve formatting
                            }
                        }
                        if let Some(tag) = fields[0].get_string() {
                            w.write(tag.as_bytes())?;
                        }
                        w.write_char('\n')?;
                    }
                }
            }
            _ => {
                crate::print_string(c"not chunked", "");
            }
        }
    }
    Ok(())
}
