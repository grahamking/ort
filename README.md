# Basic Rust CLI for openrouter.ai

You need an [openrouter.ai](https://openrouter.ai/) API key in environment variable `OPENROUTER_API_KEY`.

Example: 
```
# Basic
ort "Prompt goes here"

# Full
ort -p price -m moonshotai/kimi-k2 -s "Respond like a pirate" "Write a limerick about AI"
```

Usage:
```
ort [-m <model>] [-s "<system prompt>"] [-p <price|throughput|latency>] <prompt>
```

Default model is `openrouter/auto:price`, meaning that OpenRouter chooses the provider and model, prioritizing cheap ones.

Mostly written by GPT-5 to my spec. Only dependencies are `ureq` for HTTP, and `serde_json` to build valid JSON. No async. Built because I got frustrated waiting for Python CLIs to start. For best perf build it in `--release` mode and then run `strip` on it.

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
