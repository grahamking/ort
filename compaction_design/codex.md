# How Codex compacts context

Codex has three compaction implementations. Which one runs depends on the provider and a feature flag. All three are entered the same way: the `/compact` slash command, or an automatic check when the thread gets near its token limit.

| Path | When it runs | What the model does |
| --- | --- | --- |
| Remote compaction v2 | OpenAI and Azure Responses providers | The server writes one opaque `compaction` item. The client does not summarize. |
| Local compaction | Every other provider | The client sends a summarization prompt and keeps the assistant's text reply as the summary. |
| Token-budget reset | `features.token_budget` is on | No model call. History is replaced with a fresh copy of the standing instructions. |

The dispatch is in `core/src/tasks/compact.rs` for `/compact` and `core/src/session/turn.rs` (`run_auto_compact`) for automatic compaction.

`/compact` aborts the current turn, starts a new turn, and runs `CompactTask` (`core/src/session/handlers.rs`, `compact`). Automatic compaction runs inside the normal turn loop and does not start its own user-visible turn, except that the manual path emits `turn/started`.

## What "the context" is

The model never sees the on-disk transcript directly. Each request is built from:

1. **Base instructions.** The system prompt for the current model (`get_prompt_base_instructions`).
2. **Initial context.** Developer and user messages rendered from the current session: model instructions, permissions, `AGENTS.md`, tools, apps, plugins, environments, collaboration mode, realtime state, and similar standing text. Built by `Session::build_initial_context_with_world_state`.
3. **Conversation history.** User messages, assistant messages, reasoning items, tool calls, tool outputs, and any previous compaction item.

After the first turn, initial context is not copied into every request in full. Codex stores a reference snapshot and later turns send only the diff. Compaction throws that reference away or rebuilds it. That is why the sections below separate "standing context" from "conversation history."

## What is preserved unchanged

Nothing in the conversation is kept byte-for-byte except the items listed here, and even those are cut down if they exceed a token budget.

### Remote compaction v2 (the OpenAI path)

After the server returns its compaction item, the replacement history is built by `build_v2_compacted_history` in `core/src/compact_remote_v2.rs`.

Kept, in original order, newest-biased when the budget overflows:

- **Real user messages.** Role `user`, parsed as a user message. Text, images, and audio stay on the item. They are not rewritten into a summary.
- **Hook prompt messages.** User-role messages that are hook prompts, not ordinary user text.
- **Some inter-agent messages** (`AgentMessage`), only when all of these are true:
  - The text does not start with `Message Type: MESSAGE\n` on a descendant progress message (author is `recipient` plus a `/...` suffix).
  - The text does not start with `Message Type: FINAL_ANSWER\n`.
  - The estimated size is at most **10,000 tokens**.
- **Image-resize notices** that sit immediately after a kept message. A notice is a developer message whose only text matches the image-resize notice marker. It travels with the message it annotates.
- **Client-authored developer messages**, only if the feature `retain_client_developer_messages` is on. That feature is under development and **off by default**. When it is off, every developer message is dropped.
- **The new compaction item**, appended last. This is the server's summary. See below.

Then the kept messages are trimmed to **64,000 approximate tokens**, walking from the newest message backward (`truncate_retained_messages`). Messages that fit are copied unchanged. The one message that crosses the budget is truncated. Older messages are dropped. Truncation cuts the **middle** of text (`…N tokens truncated…`) and keeps the start and end. With `compaction_image_budget` on (the default), images count toward that 64,000 and are kept or dropped as a unit with their surrounding image tags. An image that does not fit ends the walk: older messages are not pulled in to fill the leftover budget.

### Local compaction (non-OpenAI providers)

`build_compacted_history` in `core/src/compact.rs` keeps only:

- **User message text**, up to **20,000 approximate tokens**, newest first. Images and audio on those messages are dropped. The text is the concatenation of `InputText` parts. A message that crosses the budget is middle-truncated. Older user messages are dropped.
- **One new summary message**, appended last. See "What is summarized."

Previous summaries are detected by the prefix in `prompts/templates/compact/summary_prefix.md` and are **not** copied again as user messages. The new summary replaces them.

Hook prompts are not kept. Developer messages are not kept. Assistant text is not kept, except as the source of the new summary.

### Token-budget reset

No conversation items are kept. The new history is a fresh render of initial context, plus client-authored developer messages if that feature is on (same 64,000-token trim). There is no summary.

### Standing context (all three paths)

Standing instructions are not summarized. They are rebuilt from live session state.

- **Manual `/compact`, pre-turn auto-compact, and post-turn auto-compact** do not put initial context into the replacement history. They clear `reference_context_item`. The next normal turn renders initial context again in full.
- **Mid-turn auto-compact** does insert a fresh full render of initial context into the replacement history. It is placed immediately before the last real user message. If no real user message remains, it is placed before the summary or compaction item, so that item stays last. The inserted snapshot becomes the new reference, and later turns send diffs again.

Initial context includes, when the session has them: model-switch instructions, developer instructions, extension policy and capability fragments, permissions, apps, plugins, tools, environments, collaboration mode, persistent mode, realtime instructions, `AGENTS.md`, multi-agent hints, managed developer instructions, and recommended-plugin user text. The exact set is `build_world_state_for_step` plus `build_initial_context_with_world_state`.

Base instructions (the system prompt) are not stored in history. They are attached to every model request, including the compaction request, from the current model prompt.

## What is discarded

These items are sent to the model **during** compaction so it can summarize them, then removed from the history that later turns see.

Dropped by both summarizing paths:

- Assistant messages (commentary and final answers).
- Reasoning items, including encrypted reasoning.
- Function calls, custom tool calls, local shell calls, web search calls, image generation calls, and tool-search calls.
- All tool outputs (shell, function, custom, MCP, tool search).
- System-role messages.
- Ordinary developer messages (permissions, `AGENTS.md`, tool instructions, and so on). They come back only by the fresh initial-context render described above.
- Previous `compaction` / `context_compaction` items. The new compaction item replaces them.
- Configuration-update items.
- Inter-agent progress updates and `FINAL_ANSWER` agent messages.
- Inter-agent messages larger than 10,000 estimated tokens (remote v2). Local compaction drops all agent messages.
- User messages older than the retention budget (20,000 tokens local, 64,000 tokens remote).
- Images and audio on user messages, on the local path only. Remote v2 keeps them until the 64,000 budget drops them.
- The compaction prompt itself. It is added only to the request, not to the stored history.

The UI transcript is a separate log. Compaction replaces the **model** history. The warning after local compaction says long threads and repeated compactions make the model less accurate.

## What is summarized

### Remote v2

The client does not write the summary. It appends a `compaction_trigger` item to the request. The server must return **exactly one** output item of type `compaction`:

```json
{"type": "compaction", "encrypted_content": "<opaque string>"}
```

`encrypted_content` is stored and replayed as-is. Codex never decrypts it and never asks the model to expand it. On later turns the model receives that item in history, along with the retained user messages. The analytics name for this strategy is `memento`.

If the server returns any other number of compaction items, the attempt fails and history is left unchanged.

### Local

The summary is the last assistant message in the compaction response. Only `OutputText` on an assistant `message` is used. Hidden markup is stripped. Empty text becomes `(no summary available)`.

That text is stored as a **user** message, not an assistant message, with this prefix (from `prompts/templates/compact/summary_prefix.md`) plus a newline:

> Another language model started to solve this problem and produced a summary of its thinking process. You also have access to the state of the tools that were used by that language model. Use this to build on the work that has already been done and avoid duplicating work. Here is the summary produced by the other language model, use the information in this summary to assist with your own analysis:

The model is instructed by `prompts/templates/compact/prompt.md` (overridable with config `compact_prompt`) to cover:

- Current progress and key decisions.
- Important context, constraints, and user preferences.
- What remains to be done.
- Critical data, examples, and references.

The prompt asks for a concise structured handoff. It does not impose a JSON schema. Local compaction sends **no tools** and **no output schema**.

Post-turn compaction reads the summary from the response stream and only then replaces history. If that response has no assistant text, compaction fails and the completed turn is kept. Other phases read the summary from history after the assistant item has already been recorded, then replace history with the compacted form. The raw assistant summary does not remain as an assistant message.

### Token-budget reset

Nothing is summarized. `compact_token_budget.rs` calls `start_new_context_window`, which installs fresh initial context and advances the window id. The model tool `new_context` sets a flag that causes the same reset at the next mid-turn check, without a summary.

## What is sent to the model during compaction

Both summarizing paths use the Responses API (`client_session.stream`). The request kind in client metadata is `compaction`. Metadata includes `trigger`, `reason`, `implementation`, `phase`, and `strategy: memento`.

| Field | Remote v2 | Local |
| --- | --- | --- |
| `instructions` | Current base instructions | Current base instructions |
| `tools` | The tools advertised for this step | Empty |
| `parallel_tool_calls` | true | false |
| `input` | Normalized history, then a `compaction_trigger` item | Normalized history, then one user message containing the summarization prompt |
| `output_schema` | None | None |
| Reasoning effort | The effort pinned for this model in the current window, if pinning is on | Same |

History is normalized before it is sent (`ContextManager::for_prompt`):

- Every function or custom call must have an output, and orphan outputs are removed.
- Images are stripped if the model does not accept images. Audio is stripped if the model does not accept audio.
- Code Mode can attach bounded tool-call metadata onto outputs (`attach_to_compaction_prompt`). That metadata is for the compaction request. It is not a separate summary.

Remote v2 also rewrites the **oldest** tool outputs before the request if the estimated history is larger than the model context window (`trim_function_call_history_to_fit_context_window`). Each rewritten output becomes the text `Output exceeded the available model context and was truncated`. The rewrite is only so the compaction request fits. Those outputs are still discarded when the replacement history is built.

If local compaction gets `context_window_exceeded` and the request has more than one item, it deletes the oldest history item (and its paired call or output) and retries. Retries reset. Other stream errors retry up to the provider's stream retry limit with backoff. Remote v2 allows at most **2** stream retries. If the error looks like the previous model cannot compact (model switch), remote v2 retries once on the current model.

The compaction prompt on the local path is not recorded into session history. Only the assistant output is recorded, and then history is replaced.

## When auto-compact runs

`get_total_token_usage` is checked in `context_window_token_status` (`core/src/session/context_window.rs`).

### The limit

`ModelInfo::auto_compact_token_limit`:

- If the model has a context window, the ceiling is **90%** of that window (`context_window * 9 / 10`).
- If `auto_compact_token_limit` is set on the model, use the smaller of that value and the 90% ceiling.
- If there is no context window, use the configured limit alone.

A second, harder cap is the usable window: `context_window * effective_context_window_percent / 100`. The percent defaults to **95**. Auto-compact also fires when active tokens reach this usable window, even if the 90% limit was not the one that tripped.

`token_limit_reached` is true when either:

- scoped tokens `>=` auto-compact limit **plus** the token-budget fallback buffer, or
- active tokens `>=` the 95% usable window.

The fallback buffer is `features.token_budget.auto_compact_fallback_buffer_tokens`. It is 0 when that feature config is absent. It exists so a fallback prompt can still be sent before the hard reset.

### Two ways of counting

`model_auto_compact_token_limit_scope`:

- **`total` (default).** Compare the full active-context token count to the limit.
- **`body_after_prefix`.** Subtract the window's prefill baseline from the active count. The baseline is the input-token count of the first server usage sample in this compaction window. Until that sample exists, the baseline is an estimate taken right after compaction. Growth after the prefix is what counts. The usable-window cap still uses the full active count.

### The four triggers

1. **Pre-turn, context limit.** Before the new user message is recorded (`run_pre_sampling_compact`). Compaction does not see the incoming user message. Initial context is not injected. The next turn renders it, then the user message.
2. **Pre-turn, model switch.** If the previous turn's `comp_hash` and the current model's `comp_hash` are both set and they differ, compact on the **previous** model first. A missing hash does not trigger this. Also compact on the previous model when switching to a **smaller** context window and the current token count already exceeds the new window (`model_downshift`). If the previous model fails in a retryable way, remote v2 tries the new model.
3. **Mid-turn.** After a sampling step, if the model still needs a follow-up (tool calls, or queued user input) **and** either the token limit is reached or the `new_context` tool set the rollover flag. Initial context is injected before the last real user message so the model can continue the same turn. The next model call uses the compacted history.
4. **Post-turn.** Only when `model_post_turn_compact_threshold_percent` is greater than 0, token-budget mode is off, there is no queued input, and the turn was not cancelled. The threshold is reached when `active_tokens * 100 >= usable_window * percent`, or when `token_limit_reached` is already true. The turn's answer is already done. Failure leaves that turn in place.

Guardian review can also force a compact when its own budget is exhausted. That uses the same mid-turn or pre-turn path. Token-budget mode does not summarize in that case. It fails closed.

There is no infinite-loop guard beyond "compaction should land far under the limit." A mid-turn failure ends the turn. A post-turn failure keeps the turn.

## How token count is estimated

Auto-compact does **not** tokenize the whole thread on every check. It trusts the last server usage report, then adds a local estimate for anything appended after that report.

### Server count

On each completed response, Codex stores `usage.total_tokens` as `last_token_usage.total_tokens`. `get_total_token_usage` starts from that number. For the Responses API this is the size of the last request plus the last response, which is the active context.

Items that count as "already in the server total" are assistant messages, reasoning, function calls, tool-search calls, web search, image generation, custom tool calls, local shell calls, and compaction items (`is_model_generated_item`).

### Local add-on

Tokens for every history item **after** the last model-generated item are estimated and added. That covers the new user message, tool outputs, and injected context that the server has not seen yet.

If the server did not report that reasoning tokens are included (`server_reasoning_included == false`), Codex also adds an estimate of encrypted reasoning items that sit **before** the last real user turn. Plaintext reasoning with no `encrypted_content` counts as 0. If the server already included reasoning, that extra add is skipped.

After compaction, the server total is thrown away and replaced by a full local estimate of base instructions plus the new history (`recompute_token_usage`). The next real response overwrites it with server usage. In `body_after_prefix` mode, that post-compaction estimate is the prefill baseline until the first server sample arrives. A server sample replaces the estimate and is kept. Later estimates do not overwrite a server baseline.

If local compaction cannot shrink a one-item history, Codex sets the token count to the full context window so later checks keep treating the thread as full.

### The byte heuristic

`approx_token_count` in `utils/string/src/truncate.rs`:

```text
tokens = ceil(utf8_bytes / 4)
```

`APPROX_BYTES_PER_TOKEN` is 4. The code comments call this a coarse lower bound, not a real tokenizer.

`estimate_item_token_count` sums model-visible bytes, then applies that ceiling. It skips transport ids, metadata, and outer JSON escaping.

| Item | What is counted |
| --- | --- |
| Message text | UTF-8 byte length of each text part |
| Resized or non-original image | **7,373 bytes**, about **1,844 tokens** |
| `detail: original` inline image | Decode the base64 data URL, load the image, count 32×32 patches, cap at 10,000 patches, then `patches * 4` bytes. Fallback is the 7,373-byte estimate |
| `detail: original` file image | 10,000 patches (dimensions are unknown) |
| Audio | A separate audio token estimate, converted back to bytes at 4 bytes per token |
| Function or custom call | Name, namespace, and argument string |
| Tool output | Call id, name, namespace, and output text. Encrypted output bytes use `ceil(len * 9 / 16)` |
| Encrypted reasoning or compaction blob | `(encoded_len * 3 / 4) - 650` |
| Tool-search and shell action payloads | Serialized JSON byte length |
| Image generation result | Revised prompt bytes, plus 7,373 if a result image is present |
| Plaintext reasoning, configuration updates, empty context-compaction | 0 |

The 7,373-byte image figure is chosen so the 4-bytes-per-token ceiling lands near the published vision token cost. The 32-pixel patch rule follows the OpenAI vision sizing notes cited in `history.rs`.

The same estimator is used for the 20,000 and 64,000 retention budgets, the 10,000 agent-message cap, the pre-request tool-output trim, and the post-compaction token reset.

## Replacement history, in order

Remote v2, manual / pre-turn / post-turn:

1. Retained user, hook, and eligible agent messages, oldest to newest, already trimmed to 64,000 tokens.
2. The new `compaction` item.

Remote v2, mid-turn: the same list, with a fresh initial-context block inserted before the last real user message.

Local, manual / pre-turn / post-turn:

1. Retained user texts, oldest to newest, trimmed to 20,000 tokens.
2. The summary user message (`SUMMARY_PREFIX` + assistant text).

Local, mid-turn: initial context is inserted before the last real user message, or before the summary if every user message was itself an old summary.

Token-budget:

1. Fresh initial context only.

`replace_compacted_history` writes that list into live history, persists a `Compacted` rollout item, advances the compaction window (`window_number` increments, new UUID v7 `window_id`), and recomputes tokens. Pre-compact hooks can cancel before any of this. Post-compact hooks can cancel after a successful replace. A cancel from a hook aborts the turn.

## Minimal algorithm to reimplement

For a portable clone of the **local** path:

1. Track active tokens as the last API `total_tokens`, plus `ceil(bytes/4)` for items added since that response.
2. Before a turn, if active tokens are at least `min(configured_limit, context_window * 0.90)` or at least `context_window * 0.95`, compact first. Do not include the new user message in that request.
3. During a turn, if a tool call must continue and the same limit is hit, compact, then continue.
4. To compact, send the full history plus base instructions and one user message: the summarization prompt. Do not send tools.
5. Take the last assistant text. Prefix it with the summary prefix.
6. Walk user messages from the newest backward. Keep them until 20,000 approximate tokens. Middle-truncate the message that does not fit. Drop images. Drop anything that is not a real user message. Drop older copies of the summary prefix.
7. Replace model history with those user messages plus the new summary message.
8. On the next turn, send standing instructions again in full, then that shortened history, then the new user message.
9. If the compaction request itself overflows, drop the oldest history item and retry.

For an OpenAI-compatible clone of **remote v2**, send the same history and the normal tool list, append `{"type":"compaction_trigger"}`, require one `{"type":"compaction","encrypted_content":...}` result, and keep user messages (and small non-progress agent messages) up to 64,000 tokens in front of that item. The summary bytes stay opaque.

Do not mix the two summaries. Local compaction stores a prefixed user message. Remote compaction stores an encrypted `compaction` item. Later turns are expected to see exactly one of those shapes at the end of the compacted prefix.

## Source map

- Slash command and task: `core/src/session/handlers.rs`, `core/src/tasks/compact.rs`
- Auto triggers: `core/src/session/turn.rs` (`run_pre_sampling_compact`, `run_auto_compact`, the post-sampling block in `run_turn`)
- Limits: `core/src/session/context_window.rs`, `protocol/src/openai_models.rs` (`auto_compact_token_limit`, `usable_context_window`)
- Local summarize-and-replace: `core/src/compact.rs`
- Remote v2 request and retention: `core/src/compact_remote_v2.rs`, `core/src/compact_remote_v2_attempt.rs`, `core/src/compact_remote_v2_images.rs`
- Tool-output trim before the remote request: `core/src/compact_remote_history.rs`
- Token-budget reset: `core/src/compact_token_budget.rs`, `Session::start_new_context_window`
- Token estimate: `core/src/context_manager/history.rs` (`get_total_token_usage`, `estimate_item_token_count`), `utils/string/src/truncate.rs`
- Prompts: `prompts/templates/compact/prompt.md`, `prompts/templates/compact/summary_prefix.md`
- Provider selection: `model-provider/src/provider.rs` (`RemoteCompactionSupport::V2` for OpenAI and Azure)

## Example: local compaction (non-OpenAI providers)

This is a manual `/compact` after one user turn. Standing instruction text is shortened so the JSON stays readable. The replacement history does not include those instructions. The next turn renders them again and appends them after the summary.

### Stored history before

The image stays on the user message here. The shell call, its output, the assistant reply, and the reasoning item are still in history.

```json
[
  {
    "type": "message",
    "role": "developer",
    "content": [
      {
        "type": "input_text",
        "text": "<permissions and AGENTS.md, abbreviated>"
      }
    ]
  },
  {
    "type": "message",
    "role": "user",
    "content": [
      {
        "type": "input_text",
        "text": "Rename foo() to bar() in src/main.rs and show me the diff."
      },
      {
        "type": "input_image",
        "image_url": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="
      }
    ]
  },
  {
    "type": "reasoning",
    "summary": [
      {
        "type": "summary_text",
        "text": "I will edit the function and then run git diff."
      }
    ],
    "encrypted_content": "gAAAAA-opaque-reasoning"
  },
  {
    "type": "function_call",
    "name": "shell",
    "arguments": "{\"command\":\"git diff src/main.rs\"}",
    "call_id": "call_1"
  },
  {
    "type": "function_call_output",
    "call_id": "call_1",
    "output": "diff --git a/src/main.rs b/src/main.rs\n-fn foo() {}\n+fn bar() {}\n"
  },
  {
    "type": "message",
    "role": "assistant",
    "content": [
      {
        "type": "output_text",
        "text": "Renamed foo() to bar() in src/main.rs."
      }
    ]
  }
]
```

### What is sent to the model

Local compaction sends that history plus one extra user message. It does not store this extra message. `tools` is empty. `instructions` is the current base prompt, which is not part of the history array.

The extra message is `prompts/templates/compact/prompt.md`:

```json
{
  "type": "message",
  "role": "user",
  "content": [
    {
      "type": "input_text",
      "text": "You are performing a CONTEXT CHECKPOINT COMPACTION. Create a handoff summary for another LLM that will resume the task.\n\nInclude:\n- Current progress and key decisions made\n- Important context, constraints, or user preferences\n- What remains to be done (clear next steps)\n- Any critical data, examples, or references needed to continue\n\nBe concise, structured, and focused on helping the next LLM seamlessly continue the work.\n"
    }
  ]
}
```

The model replies with assistant `output_text`. That text is the summary body. In this example it is:

```json
{
  "type": "message",
  "role": "assistant",
  "content": [
    {
      "type": "output_text",
      "text": "The user asked to rename foo() to bar() in src/main.rs. The edit is done. git diff shows only that rename. No further work was requested."
    }
  ]
}
```

### Stored history after

`replace_compacted_history` writes only the kept user text and the new summary. The image is gone. The developer message, reasoning, tool call, tool output, and assistant reply are gone. The summary is a user message. Its text is the summary prefix, a newline, then the assistant text. `content_item_kinds` is `compaction.summary`.

```json
[
  {
    "type": "message",
    "role": "user",
    "content": [
      {
        "type": "input_text",
        "text": "Rename foo() to bar() in src/main.rs and show me the diff."
      }
    ]
  },
  {
    "type": "message",
    "role": "user",
    "content": [
      {
        "type": "input_text",
        "text": "Another language model started to solve this problem and produced a summary of its thinking process. You also have access to the state of the tools that were used by that language model. Use this to build on the work that has already been done and avoid duplicating work. Here is the summary produced by the other language model, use the information in this summary to assist with your own analysis:\nThe user asked to rename foo() to bar() in src/main.rs. The edit is done. git diff shows only that rename. No further work was requested."
      }
    ],
    "internal_chat_message_metadata_passthrough": {
      "content_item_kinds": ["compaction.summary"]
    }
  }
]
```

A later `/compact` sends this array plus a new summarization prompt. The old summary is not copied into the next replacement history, because its text starts with that prefix. Only the plain user sentence is kept, and a new summary message is appended.
