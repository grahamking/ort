//! art: Open Router Agent
//! Part of the `ort` project
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use ort_openrouter_cli::{ort_err, syscall};

use core::{ffi::c_void, mem::MaybeUninit};
use std::fs;

use crate::inotify;
use crate::ort_error;
use ort_openrouter_cli::{
    ActivePrompt, Content, ErrorKind, Message, OrtResult, OutputWriter as _, Response, Role, Stats,
    Tool, Write, cli::Env, config,
};

use super::output::AgentWriter;
use super::tools;

pub fn run<W: Write + Send>(
    api_key: &str,
    cfg: &config::Cfg,
    env: &Env,
    // This contains the system prompt
    // It grows to contain the whole conversation
    mut messages: Vec<Message>,
    w_core: &mut W,
) -> OrtResult<()> {
    // Watch the file immediately
    let filename = cfg.prompt_filename.as_ref().unwrap().to_string();
    let filename_ptr = CString::new(filename.to_string()).unwrap();
    let ifd = inotify::inotify_init1(0);
    let _wd = inotify::inotify_add_watch(
        ifd,
        filename_ptr.as_ptr().cast(),
        // IN_MOVED_TO should happen with a rename-and-move save, but I don't see it
        inotify::IN_MOVED_TO | inotify::IN_CLOSE_WRITE,
    );

    // Load AGENTS.md
    if let Ok(agents_md) = fs::read_to_string("AGENTS.md") {
        match messages.first_mut() {
            Some(Message {
                role: Role::System,
                content: content_vec,
                ..
            }) => {
                match content_vec.first_mut() {
                    Some(Content::Text(s)) => {
                        // Append AGENTS.md body
                        s.push('\n');
                        s.push_str(&agents_md);
                    }
                    _ => {
                        return Err(ort_error(ErrorKind::MissingSystemPrompt, "No content"));
                    }
                }
            }
            _ => {
                return Err(ort_error(
                    ErrorKind::MissingSystemPrompt,
                    "No system prompt",
                ));
            }
        }
    }

    let mut output_writer = AgentWriter::new(w_core, cfg.show_reasoning);

    // First prompt is already in `messages`, added in `input/cli.rs::main`.
    let inital_prompt = cfg.prompt.clone().unwrap(); // Safety: Always have initial prompt
    output_writer.write(Response::Prompt(inital_prompt))?;

    let mut total_stats = Stats::default();

    loop {
        // Send a prompt, run all the requested tools
        let mut has_tool_call = true;
        while has_tool_call {
            has_tool_call = run_single(
                api_key,
                cfg,
                env,
                &mut messages,
                tools::ALL_TOOLS,
                &mut output_writer,
                &mut total_stats,
            )?;
        }
        output_writer.write(Response::Stats(total_stats.clone()))?;

        // Wait for the next user prompt
        let Some(prompt) = next_prompt(ifd, &filename)? else {
            break;
        };
        messages.push(Message::user(prompt.clone()));
        output_writer.write(Response::Prompt(prompt))?;
    }

    Ok(())
}

/// Wait for next user prompt
fn next_prompt(ifd: i32, prompt_filename: &str) -> OrtResult<Option<String>> {
    let mut ie = MaybeUninit::<inotify::inotify_event>::uninit();
    let res = syscall::read(
        ifd,
        ie.as_mut_ptr() as *mut c_void,
        size_of::<inotify::inotify_event>(),
    );
    if res <= 0 {
        return Ok(None);
    }
    //let ie = unsafe { ie.assume_init() };
    //let ie_str = utils::num_to_string(ie.mask);
    //utils::print_string(c"mask: ", &ie_str);

    // If it was IN_MOVED_TO (the rename-and-move case) we need to add another
    // inotify watch

    // todo: Make an ErrorKind
    let prompt = fs::read_to_string(prompt_filename)
        .map_err(|err| ort_err(ErrorKind::Other, err.to_string().into()))?;
    Ok(Some(prompt))
}

#[allow(clippy::too_many_arguments)]
fn run_single<W: Write + Send>(
    api_key: &str,
    cfg: &config::Cfg,
    env: &Env,
    messages: &mut Vec<Message>,
    tools: &[&'static Tool],
    output_writer: &mut AgentWriter<W>,
    total_stats: &mut Stats,
) -> OrtResult<bool> {
    let mut active_prompt = ActivePrompt::new(
        api_key.to_string(),
        cfg,
        messages.clone(),
        tools.to_vec(),
        0,
        Some(env),
    )?;
    active_prompt.start()?;

    let mut assistant_message = String::new();
    let mut assistant_tool_calls = None;
    let mut tool_call_results = vec![];

    loop {
        match active_prompt.next() {
            Ok(None) => {
                break;
            }
            Ok(Some(out)) => {
                for event in out {
                    match &event {
                        Response::Content(content) => {
                            assistant_message.push_str(content);
                        }
                        Response::ToolCalls(tool_calls) => {
                            if tool_calls.is_empty() {
                                continue;
                            }
                            // We must send this back in the assistant message
                            assistant_tool_calls = Some(tool_calls.clone());

                            for tool_call in tool_calls {
                                let active_tool = tools::parse_function(&tool_call.function)?;
                                output_writer
                                    .write(Response::ToolDisplay(active_tool.display()))?;
                                let res = active_tool.run();
                                match res {
                                    Ok(res) => {
                                        tool_call_results
                                            .push((tool_call.id.clone().unwrap(), res));
                                    }
                                    Err(ort_err) => {
                                        let msg = ort_err.as_string();
                                        // TODO: Send to output writer instead of printing here
                                        println!("Tool call failed: {msg}");
                                        tool_call_results
                                            .push((tool_call.id.clone().unwrap(), error(&msg)));
                                    }
                                }
                            }
                        }
                        Response::Stats(_) => {}
                        _ => {}
                    }
                    output_writer.write(event.clone())?;
                }
            }
            Err(err) => {
                println!("active_prompt.next: {}", err.as_string());
            }
        }
    }

    let has_tool_call = match assistant_tool_calls {
        None => {
            messages.push(Message::assistant(assistant_message));
            false
        }
        Some(all_tool_calls) => {
            // The JSON response format is a "role: assistant" message with all the tool calls
            // the agent made inside of that.
            messages.push(Message::assistant_with_tool_call(
                assistant_message,
                all_tool_calls,
            ));
            // Then multiple messages with "role: tool" and the results one by one.
            // The calls and results are not co-located.
            for (id, res) in tool_call_results {
                messages.push(Message::tool(id, res));
            }
            true
        }
    };

    let stats = active_prompt.stop();
    *total_stats += stats;

    output_writer.stop(true)?;

    Ok(has_tool_call)
}

fn error(msg: &str) -> String {
    r#"{"success": false, "error": ""#.to_string() + msg + r#""}"#
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::string::ToString;
    use alloc::vec;
    use ort_openrouter_cli::build_body;

    use crate::tools::ALL_TOOLS;

    use super::*;

    #[test]
    fn test_build_body() {
        let cfg = config::Cfg {
            api_key: None,
            base_url: "test".to_string(),
            dns: vec![],
            prompt: None,
            models: vec!["google/gemma-3n-e4b-it:free".to_string()],
            provider: Some("google-ai-studio".to_string()),
            system_prompt: Some("System prompt here".to_string()),
            priority: None,
            effort: None,
            show_reasoning: false,
            quiet: false,
            prompt_filename: None,
            files: vec![], // TODO
            include_web_tools: true,
            is_private: false,
            session_id: "test".to_string(),
        };
        let messages = vec![
            Message::user("Hello".to_string()),
            Message::assistant("Hello there!".to_string()),
        ];
        let got = match build_body(0, &cfg, &messages, &[ALL_TOOLS[0]]) {
            Ok(got) => got,
            Err(err) => {
                panic!("{}", err.as_string());
            }
        };

        let expected = r#"{"stream": true, "model": "google/gemma-3n-e4b-it:free", "provider": {"order": ["google-ai-studio"]}, "reasoning": {"enabled": false}, "messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hello there!"}], "tools":[{"type": "openrouter:web_search"}, {"type": "openrouter:web_fetch"},{"type": "function", "function": {"name": "read", "description": "Read the contents of a text file.", "parameters": {"type": "object", "properties": {"path": {"type": "string", "description": "Path to the file to read (relative or absolute)"},"offset": {"type": "number", "description": "Line number to start reading from (1-indexed)"},"limit": {"type": "number", "description": "Maximum number of lines to read"}}, "required": ["path"]}}}]}"#;

        assert_eq!(got, expected);
    }
}
