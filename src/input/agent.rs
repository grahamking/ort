//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use core::{ffi::c_void, mem::MaybeUninit};

use crate::common::tools::ReadTool;
use crate::ort_error;
use crate::{
    ErrorKind, Message, OrtResult, PromptOpts, Response, Write,
    cli::Env,
    common::{
        config,
        data::{Tool, ToolParameter},
        error,
        site::Site,
    },
    input::prompt::{self, ActivePrompt},
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
    messages: Vec<crate::Message>,
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
    let mut ie = MaybeUninit::<syscall::inotify_event>::uninit();

    // Show reasoning: false, quiet (don't show stats): true
    // TODO: We need an AgentWriter, needs to output differently
    let mut output_writer = ConsoleWriter::new(w_core, false, true);

    // Display the initial prompt
    if let Some(prompt) = &opts.prompt {
        output_writer.write(Response::Prompt(prompt.clone()))?;
    }

    // Run the initial query
    run_single(
        api_key,
        settings,
        env,
        opts.clone(),
        site,
        messages.clone(),
        tools.clone(),
        &mut output_writer,
    )?;

    loop {
        // Wait for a new prompt
        let res = syscall::read(
            ifd,
            ie.as_mut_ptr() as *mut c_void,
            size_of::<syscall::inotify_event>(),
        );
        if res <= 0 {
            break;
        }
        //let ie = unsafe { ie.assume_init() };
        //let ie_str = utils::num_to_string(ie.mask);
        //utils::print_string(c"mask: ", &ie_str);

        // If it was IN_MOVED_TO (the rename-and-move case) we need to add another
        // inotify watch

        // todo: Make an ErrorKind
        let next_prompt = utils::filename_read_to_string(&filename)
            .map_err(|str_err| error::ort_error(ErrorKind::Other, str_err))?;
        opts.prompt = Some(next_prompt);

        if let Some(prompt) = &opts.prompt {
            output_writer.write(Response::Prompt(prompt.clone()))?;
        }

        let mut last = prompt::load_last_data(env)?;
        opts.merge(last.opts);

        // Add the new prompt to the conversation
        last.messages
            .push(crate::Message::user(opts.prompt.take().unwrap()));

        // run it
        run_single(
            api_key,
            settings,
            env,
            opts.clone(),
            site,
            messages.clone(),
            tools.clone(),
            &mut output_writer,
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_single<W: Write + Send>(
    api_key: &str,
    settings: &config::Settings,
    env: &Env,
    opts: PromptOpts,
    site: &'static Site,
    messages: Vec<Message>,
    tools: Vec<Tool>,
    output_writer: &mut ConsoleWriter<W>,
) -> OrtResult<()> {
    let mut last_writer = LastWriter::new(opts.clone(), messages.clone(), tools.clone(), env)?;
    let mut active_prompt = ActivePrompt::new(
        api_key.to_string(),
        settings.dns.clone(),
        opts,
        messages,
        tools,
        0,
        site,
        Some(env),
    )?;
    active_prompt.start()?;

    loop {
        match active_prompt.next() {
            Ok(None) => {
                break;
            }
            Ok(Some(out)) => {
                for event in out {
                    match &event {
                        Response::ToolCalls(tool_calls) => {
                            for tool_call in tool_calls {
                                utils::print_string(c"Tool call: ", &tool_call.function.name);

                                match tool_call.function.name.as_ref() {
                                    "read" => {
                                        let res = tool_read(&tool_call.function.arguments)?;
                                        utils::print_string(c"Tool result: ", &res);
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

    let _stats = active_prompt.stop();
    output_writer.stop(true)?; // prints stats
    last_writer.stop(true)?; // Finalize JSON
    Ok(())
}

// TODO: make const
fn agent_tools() -> Vec<Tool> {
    alloc::vec![Tool {
        name: "read".to_string(),
        description: "Read the contents of a text file.".to_string(),
        parameters: alloc::vec![
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
        required_parameters: alloc::vec!["path".to_string()],
    }]
}

/// Tool: Read a file
/// Params is JSON
fn tool_read(params: &str) -> OrtResult<String> {
    // TODO: Specific error types
    let params = ReadTool::from_json(params)
        .map_err(|_err| ort_error(ErrorKind::Other, "Parsing read tool params JSON"))?;
    utils::filename_read_to_string(&params.path).map_err(|err| ort_error(ErrorKind::Other, err))
}
