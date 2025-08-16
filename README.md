# Basic Rust CLI for openrouter.ai

You need an [openrouter.ai](https://openrouter.ai/) API key in environment variable `OPENROUTER_API_KEY`.

Usage:
```
ort [-m <model>] [-s "<system prompt>"] [-p <price|throughput|latency>] [-r] [-rr] [-q] <prompt>
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

Flags:
- -p Provider sort. `price` is lowest price, `throughput` is lowest inter-token latency, `latency` is lowest time to first token.
- -r Enable reasoning. Only certain models.
- -rr Show the reasoning tokens. Default is not to show them.
- -q Quiet. Do not show Stats at end.

Accepts piped stdin: `echo 'What is the capital of South Africa?' | ort -m z-ai/glm-4.5-air:free`

Default model is `openrouter/auto`, meaning that OpenRouter chooses the provider. It often selects an older Claude Sonnet, which is quite expensive, so don't use the default for too much.

Orginal version written by GPT-5 to my spec.

Only dependencies are `anyhow`, `ureq` for HTTP, and `serde_json` to build valid JSON. No async. Built because I got frustrated waiting for Python CLIs to start. For best perf build it in `--release` mode and then run `strip` on it.

Stats printed at the end:
- Model: The model that executed the query. Usually only interesting with `openrouter/auto`. Useful if you're doing evals because now the output includes the model name.
- Provider: The provider selected by Open Router to run your query.
- Cost in cents: Because the cost in dollars is so low it's hard to read.
- Elapsed time: Total query duration, including network, queuing at the provider, thinking, and streaming all tokens.
- Time To First Token: Time until the first token was received. Note that reasoning (thinking) tokens count, but unless you pass `-rr` they are not displayed. That can make the TTFT look wrong.
- Inter Token Latency: Average time between each token in milliseconds.

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
MODEL_1=z-ai/glm-4.5
MODEL_2=moonshotai/kimi-k2
MODEL_3=deepseek/deepseek-r1-0528

# Close all other panes in the current window (keep only the current one)
tmux kill-pane -a

# Split the current pane horizontally to create 3 equal panes
tmux split-window -v
tmux split-window -v

# Select all panes and distribute them evenly
tmux select-layout even-vertical

# Run commands in each pane
tmux send-keys -t 0 "ort -m $MODEL_1 -s \"$SYSTEM_PROMPT\" \"$*\"" Enter
tmux send-keys -t 1 "ort -m $MODEL_2 -s \"$SYSTEM_PROMPT\" \"$*\"" Enter
tmux send-keys -t 2 "ort -m $MODEL_3 -s \"$SYSTEM_PROMPT\" \"$*\"" Enter

# Optional: Select the first pane
t
```

If you mostly use models from the big-three labs I highly recommend trying OpenRouter. You get to use Qwen, DeepSeek, Kimi, GLM - all of which have impressed me - and all sorts of cutting edge experiments, such as diffusion model Mercury.

MIT Licence.

