//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use core::fmt;
use std::fs::File;

use crate::{
    Consumer, Flushable, LastData, Message, OrtResult, PromptOpts, Response, Stats, cache_dir,
    ort_err, ort_from_err, slug, tmux_pane_id,
};

/// Adapter that lets us use any `std::io::Write` as a UTF-8-only `fmt::Write`.
pub struct IoFmtWriter<W: std::io::Write> {
    inner: W,
}

impl<W: std::io::Write> IoFmtWriter<W> {
    pub fn new(inner: W) -> Self {
        IoFmtWriter { inner }
    }

    pub fn into_inner(self) -> W {
        self.inner
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl<W: std::io::Write> fmt::Write for IoFmtWriter<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        std::io::Write::write_all(&mut self.inner, s.as_bytes()).map_err(|_| fmt::Error)
    }

    fn write_char(&mut self, c: char) -> fmt::Result {
        let mut buf = [0u8; 4];
        let slice = c.encode_utf8(&mut buf);
        std::io::Write::write_all(&mut self.inner, slice.as_bytes()).map_err(|_| fmt::Error)
    }
}

impl<W: std::io::Write> Flushable for IoFmtWriter<W> {
    fn flush(&mut self) -> OrtResult<()> {
        std::io::Write::flush(&mut self.inner).map_err(ort_from_err)
    }
}

pub struct LastWriter {
    w: IoFmtWriter<std::fs::File>,
    data: LastData,
}

impl LastWriter {
    pub fn new(opts: PromptOpts, messages: Vec<Message>) -> OrtResult<Self> {
        let last_filename = format!("last-{}.json", tmux_pane_id());
        let mut last_path = cache_dir()?;
        last_path.push('/');
        last_path.push_str(&last_filename);
        let last_file = File::create(last_path).map_err(ort_from_err)?;
        let data = LastData { opts, messages };
        Ok(LastWriter {
            data,
            w: IoFmtWriter::new(last_file),
        })
    }
    pub fn into_inner(self) -> std::fs::File {
        self.w.into_inner()
    }

    pub fn run<const N: usize>(&mut self, mut rx: Consumer<Response, N>) -> OrtResult<Stats> {
        let mut contents = Vec::with_capacity(1024);
        while let Some(data) = rx.get_next() {
            match data {
                Response::Start => {}
                Response::Think(_) => {}
                Response::Content(content) => {
                    contents.push(content);
                }
                Response::Stats(stats) => {
                    self.data.opts.provider = Some(slug(stats.provider()));
                }
                Response::Error(err) => {
                    return ort_err(format!("LastWriter: {err}"));
                }
                Response::None => {
                    eprintln!("Response::None means we read the wrong Queue position");
                }
            }
        }

        let message = Message::assistant(contents.join(""));
        self.data.messages.push(message);

        self.data.to_json_writer(&mut self.w)?;
        let _ = self.w.flush();

        Ok(Stats::default()) // Stats is not used
    }
}
