//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use core::{ffi::c_void, mem::MaybeUninit};

use crate::common::tools::BashTool;
use crate::common::tools::ReadTool;
use crate::ort_error;
use crate::syscall::system;
use crate::{
    ErrorKind, Message, OrtResult, PromptOpts, Response, Write,
    cli::Env,
    common::{
        config,
        data::{Tool, ToolParameter},
        error,
        site::Site,
    },
    input::prompt::ActivePrompt,
    output::{
        last_writer::LastWriter,
        writer::{ConsoleWriter, OutputWriter},
    },
    syscall::{self, IN_CLOSE_WRITE, IN_MOVED_TO},
    utils,
};

pub fn run<W: Write + Send>(
    api_key: &str,
    settings: &config::Settings,
    env: &Env,
    mut opts: PromptOpts,
    site: &'static Site,
    // This contains the system prompt
    // It grows to contain the whole conversation
    mut messages: Vec<crate::Message>,
    w_core: &mut W,
) -> OrtResult<()> {
    opts.quiet = Some(true);
    let tools = agent_tools();

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

    // Show reasoning: false, quiet (don't show stats): true
    // TODO: We need an AgentWriter, needs to output differently
    let mut output_writer = ConsoleWriter::new(w_core, false, true);

    while let Some(prompt) = next_prompt(opts.prompt.take(), ifd, &filename)? {
        output_writer.write(Response::Prompt(prompt.clone()))?;

        // Add the new prompt to the conversation
        messages.push(Message::user(prompt));

        let mut has_tool_call = true;
        while has_tool_call {
            has_tool_call = run_single(
                api_key,
                settings,
                env,
                opts.clone(),
                site,
                &mut messages,
                &tools,
                &mut output_writer,
            )?;
        }
    }

    Ok(())
}

/// Wait for next user prompt
fn next_prompt(
    initial_prompt: Option<String>,
    ifd: i32,
    prompt_filename: &str,
) -> OrtResult<Option<String>> {
    // This make the while loop simpler
    if initial_prompt.is_some() {
        return Ok(initial_prompt);
    }

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
    settings: &config::Settings,
    env: &Env,
    opts: PromptOpts,
    site: &'static Site,
    messages: &mut Vec<Message>,
    tools: &[Tool],
    output_writer: &mut ConsoleWriter<W>,
) -> OrtResult<bool> {
    let mut last_writer = LastWriter::new(opts.clone(), messages.clone(), tools.to_vec(), env)?;
    let mut active_prompt = ActivePrompt::new(
        api_key.to_string(),
        settings.dns.clone(),
        opts,
        messages.clone(),
        tools.to_vec(),
        0,
        site,
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
                            // We must send this back in the assistant message
                            assistant_tool_calls = Some(tool_calls.clone());

                            for tool_call in tool_calls {
                                match tool_call.function.name.as_ref() {
                                    "read" => {
                                        let res = run_tool_read(&tool_call.function.arguments)?;
                                        tool_call_results
                                            .push((tool_call.id.clone().unwrap(), res));
                                    }
                                    "bash" => {
                                        let res = run_tool_bash(&tool_call.function.arguments)?;
                                        tool_call_results
                                            .push((tool_call.id.clone().unwrap(), res));
                                    }
                                    "other" => {}
                                    _ => {}
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
        Some(tc) => {
            // TODO: Assistant can ask for multiple tool calls, return all of
            // the requests. We already return all the results.
            messages.push(Message::assistant_with_tool_call(assistant_message, tc));
            for (id, res) in tool_call_results {
                messages.push(Message::tool(id, res));
            }
            true
        }
    };

    let _stats = active_prompt.stop();
    output_writer.stop(true)?;
    last_writer.stop(true)?; // Finalize JSON

    Ok(has_tool_call)
}

// TODO: make const
fn agent_tools() -> Vec<Tool> {
    alloc::vec![def_tool_read(), def_tool_bash()]
}

fn def_tool_read() -> Tool {
    Tool {
        name: "read".to_string(),
        description: "Read the contents of a text file.".to_string(),
        parameters: vec![
            ToolParameter {
                name: "path".to_string(),
                param_type: "string".to_string(),
                description: "Path to the file to read (relative or absolute)".to_string(),
            },
            ToolParameter {
                name: "offset".to_string(),
                param_type: "number".to_string(),
                description: "Line number to start reading from (1-indexed)".to_string(),
            },
            ToolParameter {
                name: "limit".to_string(),
                param_type: "number".to_string(),
                description: "Maximum number of lines to read".to_string(),
            },
        ],
        required_parameters: vec!["path".to_string()],
    }
}

fn def_tool_bash() -> Tool {
    Tool {
        name: "bash".to_string(),
        description:
            "Execute a bash command in the current working directory. Returns stdout and stderr."
                .to_string(),
        parameters: vec![ToolParameter {
            name: "command".to_string(),
            param_type: "string".to_string(),
            description: "Bash command to execute".to_string(),
        }],
        required_parameters: vec!["command".to_string()],
    }
}

/// Tool: Read a file
/// Params is JSON
fn run_tool_read(params: &str) -> OrtResult<String> {
    //crate::utils::print_string(c"\nread: ", params);

    // TODO: Specific error types
    let params = ReadTool::from_json(params)
        .map_err(|_err| ort_error(ErrorKind::Other, "Parsing read tool params JSON"))?;
    Ok(match utils::filename_read_to_string(&params.path) {
        Ok(res) => res,
        // Return the string error so the model sees it.
        Err("NOT FOUND") => "No such file or directory: ".to_string() + &params.path,
        Err(s) => "Tool call error ".to_string() + s + ": " + &params.path,
    })
}

/// Tool: Run a bash command
fn run_tool_bash(params: &str) -> OrtResult<String> {
    //crate::utils::print_string(c"\nbash: ", params);

    let params = BashTool::from_json(params)
        .map_err(|_err| ort_error(ErrorKind::Other, "Parsing bash tool params JSON"))?;
    system(&params.command)
}
