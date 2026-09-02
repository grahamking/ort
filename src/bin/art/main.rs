//! art: Open Router Agent
//! Part of the `ort` project
//! https://github.com/grahamking/ort
//!
//! MIT License
//! Copyright (c) 2026 Graham King

use ort_openrouter_cli::{ErrorKind, OrtResult, StdoutWriter, args, cli, config, ort_error};

mod agent;
mod inotify;
mod output;
mod system_prompt;
mod tools;

fn main() -> std::process::ExitCode {
    let env = build_env();
    /*
    let is_pipe_input = !syscall::isatty(STDIN_FILENO);
    let stdin = if is_pipe_input {
        let mut buffer = String::with_capacity(8 * 1024);
        buf_read::fd_read_to_string(STDIN_FILENO, &mut buffer);
        Some(buffer)
    } else {
        None
    };
    */
    let cli_args: Vec<String> = std::env::args().collect();
    let args::Cmd::Prompt(cli_opts) = args::parse_prompt_args(&cli_args, None /*stdin*/).unwrap()
    else {
        panic!("not prompt cmd");
    };
    let config_file = cli_opts.config_file.as_deref();
    // TODO: Have it read `ort.cfg` if `art.cfg` does not exist
    let mut cfg = config::Cfg::load(&env, config_file.unwrap_or("art.cfg")).unwrap();

    cli::override_config_from_cli(&mut cfg, cli_opts.clone());
    // This will create the prompt filename if missing, so that we are sure it exists
    cfg.setup(&env).unwrap();

    let api_key = get_api_key(&env, &cfg).unwrap();

    // The default system prompt is nearly always what you want
    if cfg.system_prompt.is_none() {
        cfg.system_prompt = Some(system_prompt::DEFAULT.to_string());
    }

    // Agent mode should always include server-side web tools,
    // but not all inference platforms have them. Probably
    // need more customization in config file.
    if !cfg.include_web_tools {
        eprintln!(
            "Warn: Web search / web fetch are disabled. Add `include_web_tools: true` to your config to enable."
        );
    }

    // We display stats in a different way so silent the normal way
    cfg.quiet = true;

    let messages = cfg.messages().unwrap();
    match agent::run(&api_key, &cfg, &env, messages, &mut StdoutWriter {}) {
        Ok(()) => 0.into(),
        Err(err) => {
            eprintln!("\nFailed: {}. {}", err.kind.as_string(), err.context);
            1.into()
        }
    }
}

fn build_env() -> cli::Env {
    // The environment variable are already in memory, above stack pointer on start.
    // Release mode build uses that, never copies them.
    // Rust's debug mode copies them onto the heap. Use `leak` to make them static again.
    macro_rules! env_str {
        ($name:literal) => {
            std::env::var($name).ok().map(|v| {
                let s: &'static str = v.leak();
                s
            })
        };
    }

    cli::Env {
        HOME: env_str!("HOME"),
        PWD: env_str!("PWD"),
        TMUX_PANE: env_str!("TMUX_PANE"),
        XDG_CONFIG_HOME: env_str!("XDG_CONFIG_HOME"),
        XDG_CACHE_HOME: env_str!("XDG_CACHE_HOME"),
        OPENROUTER_API_KEY: env_str!("OPENROUTER_API_KEY"),
        NVIDIA_API_KEY: env_str!("NVIDIA_API_KEY"),
    }
}

fn get_api_key(env: &cli::Env, cfg: &config::Cfg) -> OrtResult<String> {
    // Fail fast if key missing
    let api_key_ref = env.OPENROUTER_API_KEY.unwrap_or_default();
    let api_key = api_key_ref.to_string();
    if !api_key.is_empty() {
        return Ok(api_key);
    }
    match cfg.get_api_key() {
        Some(k) => Ok(k.to_string()),
        None => Err(ort_error(
            ErrorKind::MissingApiKey,
            "api_key not in ort.cfg and OPENROUTER_API_KEY is not set.",
        )),
    }
}
