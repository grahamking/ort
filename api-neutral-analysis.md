# Chat Completions assumptions

The assumptions extend beyond the endpoint and JSON parser: **the shared conversation model, saved history, and `art` tool loop also use Chat Completions structure.** This inventory covers both binaries.

Line references describe the source at the time of analysis.

## 3. Response wire types and parsing

In **`src/common/data.rs`**:

- **`57–128` — `ChatCompletionsResponse`, `Choice`**
  Expects top-level `choices`, each containing `delta` and `finish_reason`. A tool-call turn finishes specifically with `finish_reason: "tool_calls"`.
- **`130–195` — `ToolCall`, `Function`**
  Parses chat-style indexed tool-call fragments with nested `function` objects. `update_from()` concatenates argument fragments.
- **`197–238` — `Usage`**
  Parses the usage object expected by the current stream: `total_tokens`, OpenRouter `cost`, and `server_tool_use_details`.
- **`245–266` — `RemoteError`**
  Parses the current remote-error shape (`code`, `message`), used within the Chat Completions envelope.
- **`490–566` — `Message::from_json()`**
  The same parser serves both streamed chat deltas and persisted messages. It expects chat message fields, including `reasoning` / `reasoning_content`, `reasoning_details`, `tool_calls`, and `annotations`.
- **`569–613` — `Annotation::from_json()`**
  Expects nested `url_citation` objects.
- **`659–713` — `Content::from_json()`**
  Parses chat-style content-part types and payloads.

The usage and error types contain reusable information, but their extraction is currently tied to this wire format.

## 4. Streaming protocol and completion handling

**`src/input/prompt.rs:461–681` — `ActivePrompt::next()` and `finish_reason()`** is the main concentration:

- **`493–511`:** Reads individual `data: ` lines, recognizes `[DONE]`, ignores other SSE fields, and parses each payload as `ChatCompletionsResponse`.
- **`513–525`:** Extracts usage, provider, and model from the top-level chunk.
- **`532–555`:** Takes one choice using `choices.pop()` and reads its delta and finish reason.
- **`575–599`:** Accumulates tool-call fragments by chat tool-call `index`; requires an ID to start a pending call.
- **`601–638`:** Converts delta fields into reasoning, annotation, and text events. Reasoning stop is inferred from the transition to regular content.
- **`641–644`:** Emits accumulated tool calls only on the chat `"tool_calls"` finish reason.
- **`647–672`:** Malformed-chunk recovery and event buffering are coupled to that pending-call assembly.
- **`679–681`:** Exposes completion state as the provider’s raw chat finish-reason string.

There is also a downstream assumption in **`src/input/prompt.rs:121–129`**: saving a response requires a non-missing finish reason other than `"error"`.

## 5. Shared conversation representation

These types are not merely DTOs confined to the network layer:

- **`src/common/data.rs:378–454` — `Message` and constructors**
  Represents tool calls as properties of an assistant message, and results as separate tool-role messages linked by `tool_call_id`. Reasoning is also attached to the assistant message.
- **`src/common/data.rs:756–783` — `Role`**
  Models only `system`, `user`, `assistant`, and `tool`.
- **`src/common/config.rs:156–179` — `Cfg::messages()`**
  Constructs initial input as system/user chat messages, with attachments inside the user message.
- **`src/bin/art/system_prompt.rs:54–72`**
  Assumes instructions reside in the text content of the first system-role message.

These can remain an internal representation if translated later, but they currently mirror Chat Completions rather than an API-neutral sequence of input/output items.

## 6. Saved history and continuation

- **`src/common/data.rs:268–305` — `LastData`**
  Stores and loads a `messages` array using the chat message parser.
- **`src/output/last_writer.rs:251–337`**
  Writes previous messages with the request serializer, then constructs one assistant message containing the accumulated text, reasoning, and tool calls.
- **`src/output/last_writer.rs:150–240`**
  Reassembles OpenRouter `reasoning_details`, including `reasoning.text`, `reasoning.summary`, encrypted blocks, and signatures.
- **`src/input/prompt.rs:146–177`**
  Continuation loads that history, appends another user message, and resends the message list.

**Important coupling:** the persisted history format and request wire serializer are shared.

## 7. `art` agent loop and compaction

- **`src/bin/art/agent.rs:208–215`**
  Sends the full chat-message history through `ActivePrompt`.
- **`src/bin/art/agent.rs:218–248`**
  Collects one assistant text response, reasoning fields, and an assistant tool-call batch.
- **`src/bin/art/agent.rs:308–329`**
  Explicitly reconstructs the chat tool cycle: one assistant message containing calls, followed by separate `role: tool` result messages.
- **`src/bin/art/compact.rs:35–57, 92–191`**
  History inspection, size estimation, transcript generation, and retention boundaries understand assistant messages containing calls and separate tool-role results.
- **`src/bin/art/compact.rs:228–238`**
  Summarization uses the same Chat Completions request path.
- **`src/bin/art/compact.rs:262`**
  Accepts a summary only when the raw finish reason is exactly `"stop"`.

## 8. Public interfaces, tests, and documentation

- **`src/lib.rs:20–23, 39–40`**
  Publicly exports the chat response types, shared message types, `ActivePrompt`, and the chat request builder.
- **`src/common/data.rs` tests**, especially `1128–1229`: chat chunks, choices, finish reasons, errors, reasoning details.
- **`src/input/to_json.rs` tests** and **`src/bin/art/agent.rs` tests**: exact chat request JSON.
- **`src/input/prompt.rs`, `src/output/last_writer.rs`, and `src/bin/art/compact.rs` tests**: chat SSE and completion semantics.
- **`tests/fixtures/*.jsonl`**: Chat Completions streams.
- **`src/common/buf_read.rs:324`**: a chat-shaped test payload, though the reader itself is generic.
- **`README.md:272–279`**: explicitly documents compatibility with Chat Completions APIs.

## What is mostly independent already

DNS, sockets, TLS, HTTP body framing, the JSON parser, tool execution, and ordinary console/file output are largely API-independent. The internal **`Response` event enum** (`src/common/data.rs:788–824`) is already a partial normalization layer, although `ToolCalls` carries the current `ToolCall` representation and `ThinkEvent::Details` carries raw OpenRouter reasoning JSON.

The main isolation boundaries are therefore **request encoding, stream decoding/completion state, conversation/history representation, and the agent’s tool-turn reconstruction**—not just the URL.
