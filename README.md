# Basic Rust CLI for openrouter.ai

You need an [openrouter.ai](https://openrouter.ai/) API key in environment variable `OPENROUTER_API_KEY`.

Usage:
```
ort [-m <model>] [-s "<system prompt>"] [-p <price|throughput|latency>] [-pr provider-slug] [-r off|low|medium|high|<toks>] [-rr] [-q] <prompt>
```

Use default model (currently `openai/gpt-oss-20b:free`):
```
ort "What is the capital of France?"
```

List available models:
```
ort list [-json]
```

Use Kimi K2, select the provider with lowest price, and set a system prompt:
```
ort -p price -m moonshotai/kimi-k2 -s "Respond like a pirate" "Write a limerick about AI"
```

## Flags

- -p Provider sort. `price` is lowest price, `throughput` is lowest inter-token latency, `latency` is lowest time to first token.
- -pr Provider choice. Pass the slug or name or a provider, and that will be get priority. If that provider is unavailable a different one will be chosen as if you had not provided one.
- -r Enable reasoning. Only certain models. Takes an effort level of "off" (equivalent to not passing -r, but can override config file), "low", "medium" or "high". Default is off. Can also take a number, which is max number of thinking tokens to use. Whether to use effort or max_tokens depends on the model. See reasoning model notes later.
- -rr Show the reasoning tokens. Default is not to show them.
- -q Quiet. Do not show Stats at end.

Accepts piped stdin: `echo 'What is the capital of South Africa?' | ort -m z-ai/glm-4.5-air:free`

## Stats

Stats printed at the end:

- Model: The model that executed the query. Usually only interesting with `openrouter/auto`. Useful if you're doing evals because now the output includes the model name.
- Provider: The provider selected by Open Router to run your query.
- Cost in cents: Because the cost in dollars is so low it's hard to read.
- Elapsed time: Total query duration, including network, queuing at the provider, thinking, and streaming all tokens.
- Time To First Token: Time until the first token was received. Note that reasoning (thinking) tokens count, but unless you pass `-rr` they are not displayed. That can make the TTFT look wrong.
- Inter Token Latency: Average time between each token in milliseconds.

## Config file

The API key and defaults can be stored in `${XDG_CONFIG_HOME}/ort.json`, which is usually `~/.config/ort.json`.

```
{
    "keys": [{"name": "openrouter", "value": "sk-..."}],
    "settings": {
        "save_to_file": true
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
- save_to_file: Whether to also write the output to `$XDG_CACHE_HOME}/ort/<model-name>.txt`. Defaults to true.

## Reasoning model configuration

Here's what I got from the models I use regularly.

Optional reasoning with effort, pass `-r off|low|medium|high`:

- deepseek/deepseek-chat-v3.1
- google/gemini-2.5-flash
- google/gemini-2.5-pro
- openai/gpt-oss-120b
- openai/gpt-oss-20b
- z-ai/glm-4.5

Optional reasoning with tokens, pass e.g `-r off|4096`:

- anthropic/claude-sonnet-4 # and other Anthropic models
- baidu/ernie-4.5-300b-a47b # I never see any reasoning from this model so not sure. Seems 'smarter' with -r.

Always fixed reasoning, cannot be configured or disabled:

- deepseek/deepseek-r1-0528
- qwen/qwen3-235b-a22b-thinking-2507

No reasoning:

- deepseek/deepseek-chat-v3-0324
- qwen/qwen3-235b-a22b-07-25
- moonshotai/kimi-k2

## tmux

Here's an advanced example of how I use it in tmux:

```
#!/bin/bash
#
# Query multiple models in tmux panes
# Usage: xx Prompt goes here
#
# - Resets window panes to three horizontal rows
# - Runs the query in three LLMs, one in each window

SYSTEM_PROMPT="Make your answer concise but complete. No yapping. Direct professional tone. No emoji."
MODEL_1=z-ai/glm-4.5                # Hybrid reasoning, -r useful here
MODEL_2=moonshotai/kimi-k2          # No reasoning
MODEL_3=deepseek/deepseek-r1-0528   # Always reasoning, cannot disable

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

## Misc

Only dependencies are `anyhow`, `ureq` for HTTP, and `serde_json` to build valid JSON. No async. Built because I got frustrated waiting for Python CLIs to start. For best perf build it in `--release` mode and then run `strip` on it.

MIT Licence.

