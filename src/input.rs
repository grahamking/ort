//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use crate::common::error::Context;
use crate::{OrtResult, Read, Write, chunked, common::buf_read, http};

pub mod args;
pub mod cli;
pub mod info;
pub mod list;
pub mod prompt;
pub mod to_json;

fn stream_json<R: Read, W: Write + Send>(
    response_body: http::ResponseBody,
    mut reader: buf_read::OrtBufReader<R>,
    w: &mut W,
) -> OrtResult<()> {
    // The full JSON. User should use `jq` or similar to pretty it.
    match response_body {
        http::ResponseBody::Chunked => {
            // normal case
            const MAX_CHUNK_SIZE: usize = 128 * 1024;
            let mut chunked = chunked::read::<_, MAX_CHUNK_SIZE>(reader);
            while let Some(chunk) = chunked.next_chunk() {
                let chunk = chunk?;
                w.write_all(chunk.as_bytes())
                    .context("stream_json chunked write_all")?;
            }
        }
        http::ResponseBody::ContentLength(len) => {
            let mut remaining = len;
            let mut buf: [u8; 4096] = [0; 4096];
            while remaining > 0 {
                let n = remaining.min(buf.len());
                reader
                    .read_exact(&mut buf[..n])
                    .context("stream_json contentlength read_exact")?;
                w.write_all(&buf[..n])
                    .context("stream_json contentlength write_all")?;
                remaining -= n;
            }
        }
        http::ResponseBody::UntilEof => {
            let mut buf: [u8; 4096] = [0; 4096];
            loop {
                let bytes_read = reader.read(&mut buf).context("stream_json untileof read")?;
                if bytes_read == 0 {
                    break;
                }
                w.write_all(&buf[..bytes_read])
                    .context("stream_json untileof write_all")?;
            }
        }
    }
    w.flush().context("stream_json flush")
}
