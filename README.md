# Honest CLI for openrouter.ai

`ort` sends your prompts to AI models on [openrouter.ai](https://openrouter.ai/).

It is built the old fashioned way, in solid Rust. It doesn't slow you down with Python interpreters. This is a reasonable ~580 KiB ELF binary, with no dependencies.

It's direct. Use the default model model with no fuss: `ort "What is the capital of France?"`. And if you mess up, it tells you straight: `OPENROUTER_API_KEY is not set`. That's an environment variable.

If you're new here in town, it'll introduce you: `ort list [-json]`. Everyone's a model here.

You like to know who you're talking to so `-m <model>` selects your conversation partner, and it knows you don't want to impose more then necessary so `-r off|none|low|medium|high|<toks>` sets reasoning effort. But you have your own priorities, we all do. Use `-p price|throughput|latency` for that.

It is from a time when we countered bad arguments with good arguments, so it will show you the reasoning with `-rr`. As long as you're clear about what you want, it will respect your system prompt `-s "<system prompt>"`. We all got to live here together, and we're the better for it.

Like a good friend, it remembers. `-c` will continue a conversation. And like a real friend, it accepts you how you are. In a **tmux** pane? It continues that conversation, not the one happening in the pane next door.

As an honest CLI, it cares about the small stuff. For humans there's ANSI codes. If you pipe the output somewhere else, it's clean ASCII. And you can pipe input in too. You do you.

Harking from a time when we trusted each other, it doesn't check TLS certificates, and because we know our neighbours, it uses hard-coded DNS if you set `dns` in the config file - you should. I don't mind telling you, those two can get the city folks riled up.

In short, `ort` is an honest CLI for openrouter, like it says on the box.

## Give it to me straight

Usage:
```
ort [-m <model>] [-s "<system prompt>"] [-p <price|throughput|latency>] [-pr provider-slug] [-r off|none|low|medium|high|<toks>] [-rr] [-q] [-c] <prompt>
```

Use Kimi K2, select the provider with lowest price, and set a system prompt:
```
ort -p price -m moonshotai/kimi-k2 -s "Respond like a pirate" "Write a limerick about AI"
```

## Flags

- -m Model. This is the openrouter model ID. Can be provided multiple times to query multiple models at once (in which case the output does not stream).
- -p Provider sort. `price` is lowest price, `throughput` is lowest inter-token latency, `latency` is lowest time to first token.
- -pr Provider choice. Pass the slug or name or a provider, and that will be get priority. If that provider is unavailable a different one will be chosen as if you had not provided one.
- -r Enable reasoning. Only certain models. Takes an effort level of "off" (equivalent to not passing -r, but can override config file), "none", "low", "medium" or "high". Default is off. "none" is only for GPT 5.1 so far. Can also take a number, which is max number of thinking tokens to use. Whether to use effort or max_tokens depends on the model. See reasoning model notes later.
- -rr Show the reasoning tokens. Default is not to show them.
- -q Quiet. Do not show Stats at end.
- -c Continue. Add a new prompt to the previous conversation, e.g. `ort -c "Are you sure?"`. All the fields default to the previous message (model, priority, provider, system prompt, etc, but you can override them here, for example continuing the conversation but with a different model, or a higher reasoning effort. The provider of the previous message is set as the first choice, to benefit from caching.
- -nc No config. Do not merge the default prompt options from the config into the command line prompt opts. Useful for disabling the default system prompt for example.

Accepts piped stdin: `echo 'What is the capital of South Africa?' | ort -m z-ai/glm-4.5-air:free`

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

The API key and defaults can be stored in `${XDG_CONFIG_HOME}/ort.json`, which is usually `~/.config/ort.json`. There are also some settings you can use to go faster such as `dns`.

```
{
    "keys": [{"name": "openrouter", "value": "sk-..."}],
    "settings": {
        "save_to_file": true,
        "dns": ["104.18.2.115", "104.18.3.115"]
    },
    "prompt_opts": {
        "model": "deepseek/deepseek-r1-0528",
        "system": "Make your answer concise but complete. No yapping. Direct professional tone. No emoji.",
        "priority": "latency",
        "quiet": false,
        "show_reasoning": false,
        "reasoning": {
            "enabled": true,
            "effort": "medium"
        }
    }
}
```

Here are the settings that are not available on the command line:

- `save_to_file`: Whether to also write the output to `$XDG_CACHE_HOME}/ort/last.json`. Defaults to true. The continuation (`-c`) feature needs this.
- `dns`: The IP address(es) of openrouter.ai. This saves time, no DNS lookups. Allows up to 16 addresses, although fewer is probably better.

## Security

`ort` does not validate TLS certificates. This makes the binary smaller and the connection start faster. It also makes it vunlerable to man-in-the-middle attacks. You can verify easily enough by browsing to openrouter.ai to see if your browser shows an alert. If it doesn't you're likely fine. It would be _possible_ to identify an `ort` request vs a browser request, by fingerprinting the TLS `ClientHello`, but it is extremely unlikely at this stage.

The biggest risk is a man-in-the-middle gaining your OpenRouter API key. I have a $5 daily limit on mine, which I recommend. Your prompts are already likely saved by the provider, so hopefully you weren't expecting those to be secret anyway.

A man-in-the-middle attack could be perpetrated by the owner of the wi-fi router (so use a VPN when not at home!), your ISP, or a major network provider between your ISP and openrouter.

To summarize, we gain a smaller binary and faster requests, in exchange for a very small risk of losing up to $5 a day, that you can mitigate simply by browsing to openrouter.ai. Security engineers hate this one weird trick.

`ort` has it's own TLS 1.3 stack, including all crypto operations: AES-128 GCM, HMAC, HKDF, ECDH/X25519, SHA-256. GPT-5 wrote the bulk of these implementations, one at a time. I wrote the tests first. They have not been reviewed by anyone else.

## Reasoning model configuration

Here's what I got from the models I use regularly.

Required reasoning, MUST pass `-r low|medium|high` flag:
- openai/gpt-5.1 (pass `-r none` for no reasoning, it's both required and optional)
- openai/gpt-5
- openai/gpt-5-mini
- openai/gpt-5-nano
- openai/gpt-oss-120b
- moonshotai/kimi-k2-thinking
- minimax/minimax-m2
- google/gemini-2.5-pro
- google/gemini-3-pro-preview

Optional reasoning with effort, pass `-r off|low|medium|high`:

- deepseek/deepseek-v3.1-terminus
- google/gemini-2.5-flash
- openai/gpt-oss-20b
- z-ai/glm-4.6

Optional reasoning with tokens, pass e.g `-r off|4096`:

- anthropic/claude-sonnet-4.5 # and other Anthropic models
- baidu/ernie-4.5-300b-a47b # I never see any reasoning from this model so not sure. Seems 'smarter' with -r.

Always fixed reasoning, cannot be configured or disabled:

- qwen/qwen3-235b-a22b-thinking-2507

No reasoning:

- qwen/qwen3-235b-a22b-07-25
- moonshotai/kimi-k2-0905

## My shortcuts (Nov 2025)

All of this in my `.bash_aliases` file. Then are all bash aliases, so you type your prompt right after: `q Is this a question?`.

First make them token efficient with a system prompt:
```
SYSTEM_PROMPT="Make your answer concise but complete. No yapping. Direct professional tone. No emoji."
```

I use this hundreds of times a day for all my easy questions, it's quicker than reading the docs. This model is very fast, almost free, and high quality. Sterling job Google!

```
alias q='ort -p latency -m google/gemini-2.5-flash-lite-preview-09-2025 -r off -s "$SYSTEM_PROMPT"'
```

I just added multi-model support, so after a bit of investigation these are the best combination of fast, token efficient, and good quality:

```
alias qc='ort -p latency -r off -s "$SYSTEM_PROMPT" \
-m amazon/nova-micro-v1 \
-m qwen/qwen3-next-80b-a3b-instruct \
-m x-ai/grok-4-fast \
-m google/gemini-2.5-flash-lite-preview-09-2025'
```

Sometimes you need to write some code or ask a hard question. These are IMHO the smartest models today. GPT 5 and 5.1 write very good Rust.
```
alias gpt='ort -m openai/gpt-5.1 -r medium -s "$SYSTEM_PROMPT"'
alias gemini='ort -m google/gemini-3-pro-preview -r medium'
```

For open models, I like these two, although I don't use them as much:
```
alias glm='ort -r medium -pr z-ai -m z-ai/glm-4.6:exacto -s "$SYSTEM_PROMPT"'
alias m2='ort -m minimax/minimax-m2 -r medium -s "$SYSTEM_PROMPT"'
```

## tmux

Here's an advanced example of how I use it in tmux:

```
#!/bin/bash
# # Query multiple models in tmux panes
# Usage: xx Prompt goes here
#
# - Resets window panes to three horizontal rows
# - Runs the query in three LLMs, one in each window

SYSTEM_PROMPT="Make your answer concise but complete. No yapping. Direct professional tone. No emoji."
MODEL_1=z-ai/glm-4.5                # Hybrid reasoning, -r useful here
MODEL_2=moonshotai/kimi-k2-0905     # No reasoning
MODEL_3=deepseek/deepseek-chat-v3.1 # Hybrid

# Close all other panes in the current window (keep only the current one)
tmux kill-pane -a

# Split the current pane horizontally to create 3 equal panes
tmux split-window -v
tmux split-window -v

# Select all panes and distribute them evenly
# try also: even-vertical (2 or 3 panes) or tiled (good for 4 panes)
tmux select-layout even-vertical

# Run commands in each pane
tmux send-keys -t 0 "ort -m $MODEL_1 -r medium -s \"$SYSTEM_PROMPT\" \"$*\"" Enter
tmux send-keys -t 1 "ort -m $MODEL_2 -s \"$SYSTEM_PROMPT\" \"$*\"" Enter
tmux send-keys -t 2 "ort -m $MODEL_3 -r medium -s \"$SYSTEM_PROMPT\" \"$*\"" Enter

# Optional: Select the first pane
t
```

If you mostly use models from the big-three labs I highly recommend trying OpenRouter. You get to use Qwen, DeepSeek, Kimi, GLM - all of which have impressed me - and all sorts of cutting edge experiments, such as diffusion model Mercury.
Orginal version written by GPT-5 to my spec.

MIT Licence.

