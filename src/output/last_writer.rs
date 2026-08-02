//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;

use alloc::ffi::CString;
use alloc::string::String;
use alloc::vec::Vec;

use crate::cli::Env;
use crate::common::config::Cfg;
use crate::output::OutputWriter;
use crate::{
    Context, ErrorKind, LastData, Message, OrtResult, Response, Write, common::config,
    common::file, common::utils,
};
use crate::{Role, ort_error};

/// How many bytes of content tokens to buffer before streaming to disk.
/// This limits max memory, but also the biggest message we can handle.
const TOKEN_MEM_BUFFER: usize = 4096;

/// LastWriter saves to disk the model response and enough information so that we can
/// continue the conversation with `ort -c "next prompt"` later.
pub struct LastWriter {
    w: file::File,
    env: Env,
    cfg: Cfg,
    data: LastData,
    buffer: [u8; TOKEN_MEM_BUFFER],
    buf_idx: usize,
}

fn last_path(env: &Env, ext: &str) -> OrtResult<([u8; 128], usize)> {
    let mut last_path = [0u8; 128];
    let idx = config::cache_dir(env, &mut last_path)?;
    last_path[idx] = b'/';
    let last_filename = utils::last_filename(env, ext);
    let start = idx + 1;
    let end = start + last_filename.len();
    last_path[start..end].copy_from_slice(last_filename.as_bytes());

    Ok((last_path, end))
}

/// The full path of the file where we stored the last conversation
pub(crate) fn last_data_file(env: &Env) -> OrtResult<String> {
    let (last_path, end) = last_path(env, ".json")?;

    let cs = CString::new(&last_path[..end]).expect("Null bytes in config cache dir");
    if utils::path_exists(cs.as_ref()) {
        Ok(unsafe { String::from_utf8_unchecked(last_path[..end].into()) })
    } else {
        let mut last_path = [0u8; 128];
        let cache_dir_end = config::cache_dir(env, &mut last_path)?;
        let cache_dir = unsafe { str::from_utf8_unchecked(&last_path[..cache_dir_end]) };
        utils::most_recent(cache_dir, "last-").context("most_recent")
    }
}

// The config we used last time
pub(crate) fn last_cfg(env: &Env) -> OrtResult<Cfg> {
    let (prev_config, prev_config_len) = last_path(env, ".cfg")?;
    let prev_config = unsafe { str::from_utf8_unchecked(&prev_config[..prev_config_len]) };
    let Ok(cfg_str) = utils::filename_read_to_string(prev_config) else {
        return Err(ort_error(ErrorKind::ConfigReadFailed, ""));
    };
    let prev_cfg = config::Cfg::from_str(&cfg_str)?;
    Ok(prev_cfg)
}

impl LastWriter {
    pub fn new(messages: Vec<Message>, env: &Env, cfg: &Cfg) -> OrtResult<Self> {
        let (lp, end) = last_path(env, ".json")?;
        // end + 1 to add a null byte on the end
        let last_file = unsafe { file::File::create(&lp[..end + 1]).context("create last file")? };
        let data = LastData { messages };
        Ok(LastWriter {
            data,
            env: env.clone(),
            cfg: cfg.clone(),
            w: last_file,
            buffer: [0u8; TOKEN_MEM_BUFFER],
            buf_idx: 0,
        })
    }
}

impl OutputWriter for LastWriter {
    /// Received messages and stream response to disk.
    fn write(&mut self, data: Response) -> OrtResult<()> {
        match data {
            Response::Start => {
                self.w.write_char('{')?;
                self.w.write_str(r#""messages":"#)?;

                // Write the initial messages (system, user)
                self.w.write_char('[')?;
                for (i, msg) in self.data.messages.iter().enumerate() {
                    if i != 0 {
                        self.w.write_char(',')?;
                    }
                    crate::input::to_json::write_json_message(msg, &mut self.w)?;
                }

                // Setup streaming for the response message
                self.w.write_char(',')?;
                self.w.write_str("{\"role\":")?;
                crate::input::to_json::write_json_str_simple(
                    &mut self.w,
                    Role::Assistant.as_str(),
                )?;
                self.w.write_str(",\"content\":\"")?;
            }
            Response::Think(_) => {}
            Response::Content(content) => {
                let b = content.as_bytes();
                if b.len() > TOKEN_MEM_BUFFER {
                    let l = crate::utils::num_to_string(b.len());
                    crate::utils::print_string(c"Content too long: ", &l);
                    panic!("Received content longer than TOKEN_MEM_BUFFER.");
                }

                let mut end = self.buf_idx + b.len();
                if end >= TOKEN_MEM_BUFFER {
                    crate::input::to_json::write_encoded_bytes(
                        &mut self.w,
                        &self.buffer[..self.buf_idx],
                    )?;
                    self.buf_idx = 0;
                    end = b.len();
                }

                self.buffer[self.buf_idx..end].copy_from_slice(b);
                self.buf_idx = end;
            }
            Response::ToolCalls(tool_calls) => {
                self.w.write_str(", \"tool_calls\": [")?;
                for (i, tool_call) in tool_calls.iter().enumerate() {
                    if i != 0 {
                        self.w.write_char(',')?;
                    }
                    tool_call.write_json(&mut self.w)?;
                }
                self.w.write_char(']')?;
            }
            Response::ToolDisplay(_) => {}
            Response::Stats(stats) => {
                // Update cfg because we need to use the same provider next time
                self.cfg.provider = Some(utils::slug(stats.provider()));
            }
            Response::Prompt(_) => {}
            Response::Error(_err) => {
                return Err(ort_error(
                    ErrorKind::LastWriterError,
                    "LastWriter run error",
                ));
            }
            Response::None => {
                // TODO: Can this still happen?
                panic!("Response::None means we read the wrong Queue position");
            }
        }
        Ok(())
    }

    fn stop(&mut self, _include_stats: bool) -> OrtResult<()> {
        // Write final contents
        crate::input::to_json::write_encoded_bytes(&mut self.w, &self.buffer[..self.buf_idx])?;

        // Close the contents message and messages array
        self.w.write_str("\"}]")?;
        self.w.write_char('}')?; // End of whole object
        let _ = self.w.flush();

        // Save the config
        let (last_cfg_path, last_cfg_len) = last_path(&self.env, ".cfg")?;
        self.cfg.save(last_cfg_path, last_cfg_len)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::string::ToString;
    use alloc::vec;

    use super::*;
    use crate::{LastData, ThinkEvent, common::stats, utils::num_to_string};

    #[test]
    fn test_run_success() {
        const TEST_PATH_C: &[u8] = b"/tmp/ort-last-writer-test.json\0";
        const TEST_PATH: &str = "/tmp/ort-last-writer-test.json";

        let messages = vec![
            Message::system("system prompt".to_string()),
            Message::user("user prompt".to_string()),
        ];
        let file = match unsafe { file::File::create(TEST_PATH_C) } {
            Ok(file) => file,
            Err(err) => panic!("{}", err.as_string()),
        };
        let data = LastData { messages };
        let mut writer = LastWriter {
            w: file,
            data,
            buffer: [0u8; TOKEN_MEM_BUFFER],
            buf_idx: 0,
            env: Env {
                HOME: Some("/tmp"),
                XDG_CONFIG_HOME: Some("/tmp"),
                XDG_CACHE_HOME: Some("/tmp"),
                ..Default::default()
            },
            cfg: Cfg::default(),
        };

        let mut q = vec![
            Response::Start,
            Response::Think(ThinkEvent::Start),
            Response::Think(ThinkEvent::Content("thinking...".to_string())),
            Response::Think(ThinkEvent::Stop),
        ];
        for i in 1..100 {
            q.push(Response::Content("Hello".to_string()));
            q.push(Response::Content(" world ".to_string()));
            q.push(Response::Content(num_to_string(i)));
            q.push(Response::Content(". ".to_string()));
        }
        q.push(Response::Stats(stats::Stats {
            provider: "OpenRouter AI".to_string(),
            ..Default::default()
        }));

        for event in q {
            writer
                .write(event)
                .map_err(|err| panic!("LastWriter::write failed: {}", err.as_string()))
                .unwrap();
        }
        writer
            .stop(true)
            .map_err(|err| panic!("LastWriter::stop failed: {}", err.as_string()))
            .unwrap();

        let json = utils::filename_read_to_string(TEST_PATH).unwrap();
        let data = LastData::from_json(&json).unwrap();

        //assert_eq!(data.opts.provider.as_deref(), Some("openrouter-ai"));
        assert_eq!(data.messages.len(), 3);
        assert_eq!(data.messages[0].text(), Some("system prompt"));
        assert_eq!(data.messages[1].text(), Some("user prompt"));
        let Some(content) = data.messages[2].text() else {
            panic!("Assistant message is empty");
        };
        assert!(content.starts_with("Hello world 1. "));
        assert!(content.ends_with("Hello world 99. "));
    }
}
