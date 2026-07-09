# Install

1. Install rust from [rust-lang.org](https://rust-lang.org/) or your package manager, any version should work.
2. Install the Rust version and component we need: `rustup toolchain install --profile minimal nightly-2026-03-25`
3. Install `ort`:

```
cargo +nightly-2026-03-25 install --locked ort-openrouter-cli
```

The binary is called `ort`.

Linux / x86_64 only. Uses Linux specific syscalls and x86_64 specific instrinsics.

If you consider your online activity very high risk (don't use Open Router then!) please review [SECURITY.md](SECURITY.md).

# Honest CLI for openrouter.ai

`ort` sends your prompts to AI models on [openrouter.ai](https://openrouter.ai/).

It is built the old fashioned way, in solid Rust. It doesn't slow you down with Python interpreters. This is a compact ~220 KiB ELF binary. It does not use the Rust std library or any external Rust crates, not even libc. It is statically linked.

It's direct. Use the default model model with no fuss: `ort "What is the capital of France?"`. And if you mess up, it tells you straight: `OPENROUTER_API_KEY is not set`. That's an environment variable.

If you're new here in town, it'll introduce you: `ort list [-json]`. Everyone's a model here.

You like to know who you're talking to so `-m <model>` selects your conversation partner, and it knows you don't want to impose more then necessary so `-r off|none|low|medium|high|<toks>` sets reasoning effort. But you have your own priorities, we all do. Use `-p price|throughput|latency` for that.

It is from a time when we countered bad arguments with good arguments, so it will show you the reasoning with `-rr`. As long as you're clear about what you want, it will respect your system prompt `-s "<system prompt>"`. We all got to live here together, and we're the better for it. Longer system prompts can be in a file: `-s @<filename>`.

Like a good friend, it remembers. `-c` will continue a conversation. And like a real friend, it accepts you how you are. In a **tmux** pane? It continues that conversation, not the one happening in the pane next door.

It sees things your way. `-f <filename.[jpg|png] | URL>` sends a multi-modal model an image to look at.

It's a global citizen. Pass `-ws` and it will enable OpenRouter's `web_search` and `web_fetch` tools to stay current. This is enabled by default in agent mode. Not supported with `build.nvidia.com`.

As an honest CLI, it cares about the small stuff. For humans there's ANSI codes. If you pipe the output somewhere else, it's clean ASCII. And you can pipe input in too. You do you.

Harking from a time when we trusted each other, it doesn't check TLS certificates, and because we know our neighbours, it uses hard-coded DNS if you set `dns` in the config file - you should. I don't mind telling you, those two can get the city folks riled up.

In short, `ort` is an honest CLI for openrouter, like it says on the box.

For experimental [build.nvidia.com](https://build.nvidia.com/) support see at the very end of this README.

# Give it to me straight

Usage:
```
ort [-m <model>] [-s "<system prompt>"] [-p <price|throughput|latency>] [-pr provider-slug] [-r off|none|low|medium|high|<toks>] [-rr] [-q] [-c] [-nc] [-ws] <prompt>
```

Use Kimi K2, select the provider with lowest price, and set a system prompt:
```
ort -p price -m moonshotai/kimi-k2 -s "Respond like a pirate" "Write a limerick about AI"
```

## Flags

- -m Model. This is the openrouter model ID. Can be provided multiple times to query multiple models at once (in which case the output does not stream).
- -s System Prompt. Either as a string `-s "Respond like a priate"` or a filename prefixed with '@' `-s @/data/system_prompts/the_pirate_one.txt`.
- -p Provider sort. `price` is lowest price, `throughput` is lowest inter-token latency, `latency` is lowest time to first token.
- -pr Provider choice. Pass the slug or name or a provider, and that will be get priority. If that provider is unavailable a different one will be chosen as if you had not provided one.
- -r Enable reasoning. Only certain models. Takes an effort level of "off" (equivalent to not passing -r, but can override config file), "none", "low", "medium" or "high". Default is off. "none" is only for GPT 5.1 so far. Can also take a number, which is max number of thinking tokens to use. Whether to use effort or max_tokens depends on the model. See reasoning model notes later.
- -rr Show the reasoning tokens. Default is not to show them.
- -q Quiet. Do not show Stats at end.
- -c Continue. Add a new prompt to the previous conversation, e.g. `ort -c "Are you sure?"`. All the fields default to the previous message (model, priority, provider, system prompt, etc, but you can override them here, for example continuing the conversation but with a different model, or a higher reasoning effort. The provider of the previous message is set as the first choice, to benefit from caching.
- -nc No config. Do not merge the default prompt options from the config into the command line prompt opts. Useful for disabling the default system prompt for example.
- -f filename.[jpg|png] or -f <url> Send that image to the model. E.g.: `ort -r low -m qwen/qwen3.5-35b-a3b -f ~/Temp/firefighter-cat.jpg "Describe this image"`. Can be passed multiple times. Accepts local JPG and PNG images as well as an http(s) URL for a remote image.
- -ws Enable web_search and web_fetch server-side tools.

Accepts piped stdin: `echo 'What is the capital of South Africa?' | ort -m z-ai/glm-4.5-air:free`

The prompt itself can be text `ort Say hello` or come from a file `ort @/data/prompts/test1.txt`.

## Build

`ort` has both a debug and a release build. The debug build and the tests are normal: `cargo build` and `cargo test` from workspace root.

To build in release mode use `./build_release.sh`. This tries to make the smallest binary possible. It uses immediate abort panic, and specific RUSTFLAGS. Running `cargo build --release` alone will not work.

## tmux

Continuation (`-c`) is TMUX aware. It continues the last conversation *from the current tmux pane*. That means you can carry on multiple conversations, one per pane. If there is no previous conversation for this pane, or you are not in tmux, it uses the most recent conversation globally.

The conversations are stored in `${XDG_CACHE_HOME}/ort/last-*.json`. To disable storing them set `save_to_file` to false in config.

## Stats

Stats printed at the end:

- Model: The model that executed the query. Usually only interesting with `openrouter/auto`. Useful if you're doing evals because now the output includes the model name.
- Provider: The provider selected by Open Router to run your query.
- Cost in cents: Because the cost in dollars is so low it's hard to read.
- Elapsed time: Total query duration, including network, queuing at the provider, thinking, and streaming all tokens.
- Time To First Token: Time until the first token was received. Note that reasoning (thinking) tokens count, but unless you pass `-rr` they are not displayed. That can make the TTFT look wrong.
- Inter Token Latency: Average time between each token in milliseconds.

## Config file

The API key and defaults can be stored in `${XDG_CONFIG_HOME}/ort.cfg`, which is usually `~/.config/ort.cfg`. There are also some settings you can use to go faster such as `dns`.

To choose a different config file use e.g. `--cfg ort_nvidia.cfg`. The file must still be in `XDG_CONFIG_HOME`. This replaces the pre 0.5.0 approach of switching based on the binary name. Make bash aliases!

Here are all the possible fields for doc purposes. You likely don't want to set all this, and some are somewhat contradictory (don't set both provider and priority). The CLI flags take precedence over the config settings, but only if you set them. For example if you put `provider:` in your `ort.cfg`, and don't pass `-pr <other>` on the cmd line, it will try using that provider for everything.

```
# Comments must start with # as first char
# This is the default, don't need to set
base_url: openrouter.ai/api/v1
# Or set env var OPENROUTER_API_KEY
api_key: sk-PASTE-KEY-HERE
# -m
model: openai/gpt-oss-120b
# -s
system_prompt: Make your answer concise but complete. No yapping. Direct professional tone. No emoji.
# -q
quiet: false
# -rr
show_reasoning: false
# -ws
include_web_tools: false
# -r
effort: low
# -pr
provider: baseten
# -p
priority: latency

# These two only available in config file

# Whether to also write the output to `$XDG_CACHE_HOME}/ort/last.json`. Defaults to true. The continuation (`-c`) feature needs this.
save_to_file: true

# The IP address(es) of openrouter.ai. This saves time, no DNS lookups. Highly recommend setting.
dns: 104.18.2.115, 104.18.3.115
```

Migrating from pre 0.5.0: ort previously had a JSON configuration file. Hopefully the field mapping is obvious. You'll also need to delete the contents of `~/.cache/ort`.

## Performance

Make sure to set the `dns` entry in config file. This saves a DNS query to at least the local resolver (typically `systemd-resolved`), possibly all the way to root servers.

Non-reasoning models are much faster (and cheaper!) than reasoning models.

# Agent mode (experimental)

`ort` has an experimental agent mode. This provides the model with some basic tools: read, write, edit and bash.

Usage:
```
ort agent -r medium -m openai/gpt-5.4-mini -s @agent_system_prompt.txt @/home/graham/prompt
```

So far I have only tested it with `openai/gpt-5.4-mini` and `openai/gpt-oss-120b:exacto`.

The `agent_system_prompt.txt` is in the root of this repo. Feel free to tune it. Special strings `$PWD` and `$DATE` are replaced with the current working directory, and the output of shell `date` command.

The `prompt` file is the initial prompt (the `@` is required here). We then watch (with `inotify`) that file for a change, which is the next prompt. So instead of a CLI, the interface is that `prompt` file that you edit with your own editor, and on save the new prompt is sent to the agent. Stdout shows the agent output.

The philosophy is that I already have a very good editor (`nvim`) and window manager (`tmux`) so I don't need the agent CLI to provide these. Run `ort agent` in tmux, split the window vertically about 80 / 20, and run `vim /home/graham/prompt` in the bottom 20%.

WARNING: Always run agents in a sandbox (I like `firejail`). The ort agent never asks you for confirmation and does not sandbox for you.

# Misc

## My shortcuts (Jun 2026)

All of this in my `.bash_aliases` file. Then are all bash aliases, so you type your prompt right after: `q Is this a question?`.

First make them token efficient with a system prompt:
```
SYSTEM_PROMPT="Make your answer concise but complete. No yapping. Direct professional tone. No emoji. Date: $(date)."
```

I use this hundreds of times a day for all my easy questions, it's quicker than reading the docs. This model is very fast, almost free, and high quality. Sterling job Google!

```
alias q='ort -p latency -m google/gemini-3.1-flash-lite -r off -s "$SYSTEM_PROMPT"'
```

A good alternative. A bit slower but smarter, and still very cheap:
```
alias dsf='ort -pr deepseek -m deepseek/deepseek-v4-flash -r low -s "$SYSTEM_PROMPT"'
```

Some fast good quality models:
```
alias fast_kimi='ort -pr moonshotai -r none -m moonshotai/kimi-k2.6 -s "$SYSTEM_PROMPT"'
alias fast_gpt='ort -m openai/gpt-5.5 -r none -s "$SYSTEM_PROMPT"'
alias fast_gemini='ort -m google/gemini-3.5-flash -r low -s "$SYSTEM_PROMPT"'
```

Thinking models, with web_search / web_fetch. Slower but smart.

```
alias glm='ort -r medium -pr z-ai -m z-ai/glm-5.2 -ws -s "$SYSTEM_PROMPT"'
alias minimax='ort -pr minimax -m minimax/minimax-m3 -r medium -ws -s "$SYSTEM_PROMPT"'
alias gpt='ort -m openai/gpt-5.4 -r medium -ws -s "$SYSTEM_PROMPT"' # Leave as 5.4 it's much cheaper
alias gemini='ort -m google/gemini-3.1-pro-preview -ws -r medium'
alias kimi='ort -r medium -pr moonshotai -m moonshotai/kimi-k2.6 -ws -s "$SYSTEM_PROMPT"'
alias ds='ort -pr deepseek -m deepseek/deepseek-v4-pro -r medium -ws -s "$SYSTEM_PROMPT"'
```

## tmux

Here's an advanced example of how I use it in tmux. I have this on my path as `/bin/smart` for when I need the best possible answers. Uses `SYSTEM_PROMPT` from earlier.

This will ask the three top frontier models the same question, and open the answer in three tmux panes.

```
#!/bin/bash

# Close all other panes in the current window (keep only the current one)
tmux kill-pane -a

# Split the current pane horizontally
tmux split-window -v
tmux split-window -v

# Select all panes and distribute them evenly
# tmux select-layout tiled
# tmux select-layout even-horizontal
tmux select-layout even-vertical

# Run commands in each pane
tmux send-keys -t 0 "ort -pr anthropic -m anthropic/claude-opus-4.8 -r high -ws -s \"$SYSTEM_PROMPT\" \"$*\"" Enter
tmux send-keys -t 1 "ort -pr google-vertex -m google/gemini-3.1-pro-preview -r high -ws -s \"$SYSTEM_PROMPT\" \"$*\"" Enter
tmux send-keys -t 2 "ort -pr openai -m openai/gpt-5.5 -r high -ws -s \"$SYSTEM_PROMPT\" \"$*\"" Enter

# Optional: Select the first pane
tmux select-pane -t 0
```

## Development

We do our own DNS resolution (of course!). Currently that's an A query to the first `nameserver` defined in `/etc/resolv.conf` or if there isn't one `127.0.0.53`. A resolver will need to be running there. On modern Linux that's `systemd-resolved`. `ort` does not check `/etc/hosts`, and does not support IPv6. We only do this query if you forgot to set `.config/ort.json` values `settings / dns`. See "Performance" section and example config file.

The most recent call is logged in `~/.cache/ort/log.jsonl`. Request JSON on the first line, then all the response lines.

MIT Licence.

# build.nvidia.com support

NVIDIA runs a model hub at [build.nvidia.com](https://build.nvidia.com) with some free quota.

1. Create an account and get an API key at build.nvidia.com.
1. Create a new config file in `XDG_CONFIG_HOME` and run ort as `ort --cfg <new_config>`. Set `base_url: integrate.api.nvidia.com/v1` and set `api_key: <here>`.
1. If you cache the IP address is must be the IP of `integrate.api.nvidia.com` (AWS load balancer). That's optional.
1. `ort list --cfg nrt.cfg` should now work

