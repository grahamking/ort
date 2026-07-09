//! ort: Open Router CLI
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

use core::{ffi::c_void, mem::MaybeUninit};

use crate::Role;
use crate::common::config::Cfg;
use crate::common::data::Content;
use crate::common::stats::Stats;
use crate::common::tools::{self};
use crate::ort_error;
use crate::{
    ErrorKind, Message, OrtResult, PromptOpts, Response, Write,
    cli::Env,
    common::{data::Tool, error},
    input::prompt::ActivePrompt,
    output::{OutputWriter, agent::AgentWriter, last_writer::LastWriter},
    syscall::{self, IN_CLOSE_WRITE, IN_MOVED_TO},
    utils,
};

pub fn run<W: Write + Send>(
    api_key: &str,
    cfg: &Cfg,
    env: &Env,
    mut opts: PromptOpts,
    // This contains the system prompt
    // It grows to contain the whole conversation
    mut messages: Vec<crate::Message>,
    w_core: &mut W,
) -> OrtResult<()> {
    opts.quiet = Some(true);

    // Watch the file immediately
    let filename = opts.prompt_filename.as_ref().unwrap().to_string();
    let filename_ptr = CString::new(filename.to_string()).unwrap();
    let ifd = syscall::inotify_init1(0);
    let _wd = syscall::inotify_add_watch(
        ifd,
        filename_ptr.as_ptr().cast(),
        // IN_MOVED_TO should happen with a rename-and-move save, but I don't see it
        IN_MOVED_TO | IN_CLOSE_WRITE,
    );

    // Load AGENTS.md
    if let Ok(agents_md) = utils::filename_read_to_string("AGENTS.md") {
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

    let mut output_writer = AgentWriter::new(w_core, opts.show_reasoning.unwrap_or(false));

    // First prompt is already in `messages`, added in `input/cli.rs::main`.
    let inital_prompt = opts.prompt.take().unwrap(); // Safety: Always have initial prompt
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
                opts.clone(),
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
    let mut ie = MaybeUninit::<syscall::inotify_event>::uninit();
    let res = syscall::read(
        ifd,
        ie.as_mut_ptr() as *mut c_void,
        size_of::<syscall::inotify_event>(),
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
    let prompt = utils::filename_read_to_string(prompt_filename)
        .map_err(|str_err| error::ort_error(ErrorKind::Other, str_err))?;
    Ok(Some(prompt))
}

#[allow(clippy::too_many_arguments)]
fn run_single<W: Write + Send>(
    api_key: &str,
    cfg: &Cfg,
    env: &Env,
    opts: PromptOpts,
    messages: &mut Vec<Message>,
    tools: &[&'static Tool],
    output_writer: &mut AgentWriter<W>,
    total_stats: &mut Stats,
) -> OrtResult<bool> {
    let mut last_writer = LastWriter::new(opts.clone(), messages.clone(), tools.to_vec(), env)?;
    let mut active_prompt = ActivePrompt::new(
        api_key.to_string(),
        cfg,
        opts,
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
                                        crate::utils::print_string(c"Tool call failed: ", &msg);
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
                    last_writer.write(event)?;
                }
            }
            Err(err) => {
                utils::print_string(c"active_prompt.next: ", &err.as_string());
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
    last_writer.stop(true)?; // Finalize JSON

    Ok(has_tool_call)
}

fn error(msg: &str) -> String {
    r#"{"success": false, "error": ""#.to_string() + msg + r#""}"#
}
