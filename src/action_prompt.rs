//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

use std::io;
use std::sync::mpsc;
use std::thread;

use crate::CancelToken;
use crate::OrtResult;
use crate::PromptOpts;
use crate::config;
use crate::ort_error;

use crate::multi_channel;
use crate::writer;

pub fn run(
    api_key: &str,
    cancel_token: CancelToken,
    settings: config::Settings,
    opts: PromptOpts,
    messages: Vec<crate::Message>,
    is_pipe_output: bool, // Are we redirecting stdout?
    w: impl io::Write + Send,
) -> OrtResult<()> {
    let show_reasoning = opts.show_reasoning.unwrap();
    let is_quiet = opts.quiet.unwrap_or_default();
    //let model_name = opts.common.model.clone().unwrap();

    // Start network connection before almost anything else, this takes time
    let rx_main = crate::prompt(
        api_key,
        cancel_token,
        settings.dns,
        opts.clone(),
        messages.clone(),
    );
    std::thread::yield_now();

    let (tx_stdout, rx_stdout) = mpsc::channel();
    //let (tx_file, rx_file) = mpsc::channel();
    let (tx_last, rx_last) = mpsc::channel();

    let jh_broadcast = multi_channel::broadcast(rx_main, vec![tx_stdout, tx_last]);

    //let cache_dir = config::cache_dir()?;
    //let path = cache_dir.join(format!("{}.txt", utils::slug(&model_name)));
    //let path_display = path.display().to_string();

    let scope_err = thread::scope(|scope| {
        let mut handles = vec![];
        let jh_stdout = scope.spawn(move || -> OrtResult<()> {
            let (stats, mut w) = if is_pipe_output {
                let mut fw = writer::FileWriter {
                    writer: w,
                    show_reasoning,
                };
                let stats = fw.run(rx_stdout)?;
                let w = fw.into_inner();
                (stats, w)
            } else {
                let mut cw = writer::ConsoleWriter {
                    writer: w,
                    show_reasoning,
                };
                let stats = cw.run(rx_stdout)?;
                let w = cw.into_inner();
                (stats, w)
            };
            let _ = writeln!(w);
            if !is_quiet {
                //if settings.save_to_file {
                //    let _ = write!(handle, "\nStats: {stats}. Saved to {path_display}\n");
                //} else {
                let _ = write!(w, "\nStats: {stats}\n");
                //}
            }

            Ok(())
        });
        handles.push(jh_stdout);

        if settings.save_to_file {
            /*
            let jh_file = thread::spawn(move || -> OrtResult<()> {
                let f = File::create(&path)?;
                let mut file_writer = writer::FileWriter {
                    writer: Box::new(f),
                    show_reasoning,
                };
                let stats = file_writer.run(rx_file)?;
                let f = file_writer.inner();
                let _ = writeln!(f);
                if !is_quiet {
                    let _ = write!(f, "\nStats: {stats}\n");
                }
                Ok(())
            });
            handles.push(jh_file);
            */

            let jh_last = scope.spawn(move || -> OrtResult<()> {
                let mut last_writer = writer::LastWriter::new(opts, messages)?;
                last_writer.run(rx_last)?;
                Ok(())
            });
            handles.push(jh_last);
        }

        for h in handles {
            if let Err(err) = h.join().unwrap() {
                let mut oe = ort_error(err.to_string());
                oe.context("Internal thread error");
                // The errors are all the same so only print the first
                return Err(oe);
            }
        }
        Ok(())
    });
    scope_err?;
    jh_broadcast.join().unwrap().map_err(|e| {
        let mut oe = ort_error(e.to_string());
        oe.context("broadcast thread error");
        oe
    })?;

    Ok(())
}
