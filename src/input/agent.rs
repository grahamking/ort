//! ort: Open Router CLI
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2025 Graham King

extern crate alloc;
use alloc::ffi::CString;
use alloc::string::ToString;
use alloc::vec::Vec;

use core::{ffi::c_void, mem::MaybeUninit};

use crate::{
    ErrorKind, OrtResult, PromptOpts, Write,
    cli::Env,
    common::{
        config,
        data::{Tool, ToolParameter},
        error,
        site::Site,
    },
    input::prompt,
    syscall::{self, IN_CLOSE_WRITE, IN_MOVED_TO},
    utils,
};

const BOLD_START: &str = "\x1b[1m";
const BOLD_END: &str = "\x1b[0m";

// TODO: this mixes input and output
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

    let mut separator = "—".repeat(20); // TODO: width of terminal, maybe with sides
    separator.push('\n');

    // Watch the file immediately
    let filename = opts.prompt_filename.as_ref().unwrap().as_str();
    let filename_ptr = CString::new(filename.to_string()).unwrap();
    let ifd = syscall::inotify_init1(0);
    let _wd = syscall::inotify_add_watch(
        ifd,
        filename_ptr.as_ptr().cast(),
        // IN_MOVED_TO should happen with a rename-and-move save, but I don't see it
        IN_MOVED_TO | IN_CLOSE_WRITE,
    );
    let mut ie = MaybeUninit::<syscall::inotify_event>::uninit();

    let _ = w_core.write_str(&separator);

    // TODO: Replace with the actual prompt
    // TODO: Change background color to indicate it's the prompt
    let c_prompt = CString::new("Initial prompt".to_string()).unwrap();
    let _ = w_core.write(BOLD_START.as_bytes());
    let _ = w_core.write(c_prompt.as_bytes());
    let _ = w_core.write(BOLD_END.as_bytes());
    let _ = w_core.write("\n".as_bytes());

    let _ = w_core.write_str(&separator);
    let _ = w_core.flush();

    // Run the initial query
    // TODO: We need our own "run", it is very simple, can re-use
    // so we can run tools
    prompt::run(
        api_key,
        settings,
        env,
        opts.clone(),
        site,
        messages.clone(),
        tools.clone(),
        false,
        w_core,
    )?;

    utils::print_string(c"\n Initial prompt run complete", "");

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

        // TODO: If it was IN_MOVED_TO (the rename-and-move case) we need to add another
        // inotify watch

        // TODO: Make an ErrorKind
        let next_prompt = utils::filename_read_to_string(filename)
            .map_err(|str_err| error::ort_error(ErrorKind::Other, str_err))?;
        opts.prompt = Some(next_prompt);

        // TODO: Single write syscall
        let _ = w_core.write_str(&separator);

        let c_prompt = CString::new(opts.prompt.clone().unwrap()).unwrap();
        let _ = w_core.write(BOLD_START.as_bytes());
        let _ = w_core.write(c_prompt.as_bytes());
        let _ = w_core.write(BOLD_END.as_bytes());

        let _ = w_core.write_str(&separator);
        let _ = w_core.flush();

        prompt::run_continue(api_key, settings, env, opts.clone(), site, false, w_core)?;
    }

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
