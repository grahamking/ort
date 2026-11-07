# Common streaming format implementation plan

## Goal

Introduce an API-neutral `StreamEvent` representation between API decoding and `ActivePrompt::next()`. Decode both Chat Completions and Responses into semantic stream events, rather than adapting Responses into Chat Completions' `choices/delta` structure.

Keep the existing downstream `Response` interface for output writers, history persistence, and the `art` agent wherever practical. This document is a plan, not an implementation.

Constraints: preserve `no_std`, no external crate dependencies, small binary size, and low allocation overhead in the core client.

## Current boundaries

- `src/input/prompt.rs`: `ActivePrompt::next()` currently reads SSE lines, calls `ChatCompletionsResponse::from_json`, updates statistics, accumulates tool calls, and emits downstream `Response` values.
- `src/common/data.rs`: contains `ChatCompletionsResponse`, `Choice`, `Message`, `ToolCall`, `Usage`, and downstream `Response` types.
- `src/output/`: consumes downstream `Response` values.
- `src/bin/art/agent.rs`: consumes tool-call and reasoning events.
- `src/bin/art/compact.rs`: checks `finish_reason() == Some("stop")`.
- `src/output/last_writer.rs`: persists reasoning details and tool calls; includes finish-reason-dependent tests.

Inspect these consumers before changing completion semantics or reasoning-detail representation.

## 1. Define an internal semantic event type

Suggested starting point (illustrative Rust, not a fixed public API):

```rust
enum StreamEvent {
    Text(String),
    Reasoning(String),
    ReasoningDetails(String),
    Annotation(Annotation),

    ToolCallStart {
        index: u32,
        id: String,
        name: String,
    },
    ToolCallArguments {
        index: u32,
        delta: String,
    },

    Metadata {
        model: Option<String>,
        provider: Option<String>,
    },
    Usage(Usage),
    Error(String),
    Finished(FinishStatus),
}

enum FinishStatus {
    Complete,
    ToolCallsReady,
    Incomplete { reason: Option<String> },
    Failed { reason: Option<String> },
}
```

These are internal decoded events, not a replacement for downstream `Response`.

Design requirements:

- No `choices`, `delta: Message`, or API-specific event names in the common contract.
- One JSON payload can produce zero, one, or multiple common events.
- Text and argument events contain incremental fragments, never final snapshots replayed as deltas.
- `index` correlates a tool call within one response. Its meaning is not an array position in `pending_tool_calls`.
- `id` is the identifier required when submitting the tool result.
- `Finished` means the generation outcome is known. It is not necessarily the end of transport data: Chat Completions can send usage after the finish reason.
- An error message and a failed outcome are separate concepts. An error alone must never make an incomplete generation look successful.
- Unknown optional metadata must not overwrite previously known metadata.

Review the current `Usage` type before reusing it unchanged. Its cost defaults to zero; Responses may omit cost entirely. Prefer optional fields or patch semantics where absence is meaningful. Do not turn missing cost into a reported free request, and do not erase known totals with absent values.

`ReasoningDetails(String)` is provisional: currently downstream code expects particular JSON structures. Do not pass arbitrary Responses reasoning-item JSON into that interface without checking history parsing and request serialization. Preserve opaque reasoning data losslessly where supported, or explicitly scope out round-tripping until a suitable representation exists.

## 2. Separate wire decoding from stream processing

Use three layers:

1. SSE/transport reader: reads payloads and logs them.
2. API decoder: understands Chat Completions or Responses wire schemas and emits `StreamEvent` values.
3. Common processor in `ActivePrompt`: updates state and produces existing downstream `Response` events.

Place decoders in small API-specific modules using the repository's existing JSON parser. Choose their exact location after inspecting the current API module layout.

Prefer static dispatch or a small enum selecting the decoder. Avoid adding a heap-allocated trait object merely for protocol selection. Decoder state is allowed where normalization requires it, especially for Responses tool correlation and terminal status classification.

Avoid allocating a fresh `Vec<StreamEvent>` and a second parsed message tree for every line. Consider emitting into a shared handler or using reusable storage. Decide the exact interface based on borrowing simplicity and measured code size; preserve event ordering regardless of interface.

## 3. Chat Completions mapping

- `delta.content` -> `Text`.
- Reasoning text -> `Reasoning`.
- Existing opaque reasoning details -> `ReasoningDetails`, preserving current behavior.
- Annotations -> `Annotation`.
- Initial tool-call fragments with ID/name -> `ToolCallStart`.
- Argument fragments -> `ToolCallArguments` using the same index.
- Model/provider fields -> `Metadata`, independently of whether usage is present.
- Usage chunk -> `Usage`.
- Remote error -> `Error`, plus failed outcome when appropriate.
- `stop` -> `Finished(Complete)`.
- `tool_calls` -> `Finished(ToolCallsReady)`.
- Length limits, filtering, and other non-success finishes -> explicit incomplete/failed status as appropriate; document the mapping rather than treating every non-null reason as success.

A tool-start fragment can also contain arguments: emit the start before those arguments. Inspect existing fixture variations before assuming the function name is always wholly present in the first fragment. If supported providers split names or identifiers, either normalize them in the decoder or extend the common tool event contract deliberately; do not silently lose fragments.

Initially preserve the current single-choice behavior. Multiple choices require separate stream identities and are out of scope unless intentionally designed.

Do not stop reading just because a choice finish reason arrived. Consume trailing usage until `[DONE]` or the HTTP body ends. EOF without a known finish status remains premature completion.

## 4. Responses mapping

Confirm exact supported provider schemas against documentation and representative fixtures during implementation. Dispatch on the JSON `type`; SSE `event:` lines can remain ignored if the payload contains the authoritative event type.

- `response.output_text.delta` -> `Text`.
- Supported reasoning text/summary deltas -> `Reasoning`; choose the intended display source and avoid duplicate output if both are available.
- Annotation-added events -> `Annotation`, translating to the existing annotation representation where possible.
- Function-call `response.output_item.added` -> `ToolCallStart`.
- `response.function_call_arguments.delta` -> `ToolCallArguments`.
- Metadata in lifecycle/terminal response objects -> `Metadata`.
- Usage in the terminal response -> `Usage` before final completion is processed.
- `response.completed` -> `Finished(Complete)` or `Finished(ToolCallsReady)` depending on collected client-executable calls.
- `response.incomplete` -> `Finished(Incomplete { ... })`.
- `response.failed` and terminal errors -> error details and a failed outcome.
- Unneeded lifecycle and housekeeping events -> no common event.

Tool-call rules:

- Use Responses `call_id` as the tool-call ID, not output item `id`.
- Use `output_index` as the common correlation index where available. Maintain item-ID-to-index correlation if some events require it.
- Output indexes may include non-tool items and therefore need not be contiguous among tools.
- Only normalize client-executable function calls into tool-call events; server-side tools must not accidentally be executed by `art`.
- `.done` events and terminal output arrays can repeat full argument/text snapshots. Do not append or emit these again after deltas.
- Define handling for snapshot-only calls explicitly. If supported, reconcile in decoder state and emit missing content exactly once; do not blindly replay terminal snapshots.
- Do not execute a call on its individual item/arguments `.done` event. Collect calls until successful response-level completion, matching current turn-level behavior.
- Failed or incomplete terminal responses must not release accumulated calls for execution.

Responses terminal events normally carry the final outcome and usage, so the decoder can also signal that no more useful payloads are required. Keep this transport-exhaustion signal separate from `Finished`: Chat Completions still needs its trailing usage. Do not wait indefinitely for EOF after a Responses terminal event, and do not drop the final queued output by returning `None` too early.

## 5. Refactor the common processor

Keep these responsibilities in `ActivePrompt`:

- Start notification, SSE reading/filtering, and raw payload logging.
- Applying metadata and usage to `Stats`.
- Timing and current chunk-based token-rate estimation (do not claim chunks are actual tokens).
- Tool-call assembly and malformed-fragment safety.
- Translating semantic events into `Response::Content`, `Think`, `Annotation`, `Error`, and `ToolCalls`.
- Recording generation outcome and transport exhaustion separately.

Replace implicit conditions on `choice.delta` and string finish reasons with explicit common-event handling.

Tool assembly must preserve the current safety property: malformed fragments poison affected pending calls so they cannot be executed with missing arguments. Unknown well-formed API events are not malformed data and should not generate `Response::Missing`. Handle orphan argument fragments explicitly rather than accidentally attaching them to the wrong call.

On successful `ToolCallsReady`, emit the collected calls once. On failed/incomplete completion, retain the unsuccessful outcome and do not emit executable calls. Ensure pending calls cannot cause an infinite read loop after terminal failure.

Preserve current reasoning display behavior initially: open on the first reasoning text and close on the first normal content. Arbitrary alternating reasoning/text phases are a separate enhancement, not silently solved by the new enum.

Ensure queued output is drained before returning `Ok(None)`, including terminal errors, start notifications, and tool calls. Repeated calls after exhaustion should return `None` consistently.

## 6. Completion compatibility and integration scope

Keep `finish_reason()` temporarily as a compatibility accessor, or migrate every consumer together. Suggested compatibility values are `stop`, `tool_calls`, an incomplete reason, and `error`; retain the richer internal status rather than deriving success from strings.

Review the policy in `run()`: it currently accepts any finish reason other than absent/error. Decide and test whether incomplete responses may replace the last saved conversation. This is an explicit policy decision, not an incidental consequence of the normalization refactor. A failed or prematurely disconnected response must not replace usable history.

Preserve existing Chat Completions behavior first, then add Responses decoding behind explicit protocol selection. Request-side Responses support is also necessary for end-to-end use: endpoint selection, body construction, conversation/tool serialization, and reasoning-history handling. This event-format plan does not by itself implement those request-side changes. Do not infer protocol from the first received payload.

## 7. Implementation sequence

1. Add the common event and finish-status types, with documented invariants.
2. Extract the Chat Completions decoder and refactor `next()` to consume common events. Run existing fixtures/tests before introducing Responses.
3. Add decoder/transport completion separation and audit completion consumers in `run()`, history, and `art`.
4. Implement Responses decoding with representative SSE fixtures and explicit tool correlation state.
5. Connect protocol selection alongside the separately implemented Responses request path.
6. Run regression tests and inspect release binary-size/performance impact using existing repository tooling.

## 8. Acceptance tests

Cover decoder unit tests and end-to-end `ActivePrompt::next()` fixture tests:

- Equivalent Chat Completions and Responses streams produce equivalent downstream text, reasoning, annotations, tool calls, and statistics.
- Chat Completions trailing usage is retained after a finish reason.
- Responses terminal completion returns final queued events before exhaustion, without requiring `[DONE]` or transport EOF.
- Role-only/lifecycle events, heartbeats, blank lines, and SSE `event:` lines cause no missing-content warnings.
- Metadata without usage updates correctly; absent metadata/cost does not erase known values.
- Multiple interleaved tool calls, sparse output indexes, start-plus-arguments in one payload, and mixed text/tool output are handled correctly.
- Responses tool-result IDs use `call_id`, not item `id`.
- Final snapshots and `.done` events do not duplicate text, reasoning, annotations, or tool arguments.
- Tool calls emit once at successful response-level completion, never at item completion or failed/incomplete completion.
- Malformed tool fragments cannot produce executable incomplete calls; orphan fragments have a tested policy.
- Failed/incomplete events, remote error events, and premature EOF record the correct outcome and do not silently become success.
- Empty successful responses, errors before content, and terminal events on the first payload do not lose queued events.
- Existing `art` tool-call tests, compaction success checks, and last-conversation persistence tests remain valid or are deliberately updated with documented policy changes.
- Opaque reasoning persistence/round-trip behavior is tested for each API claimed to support it.

After implementation changes, run in order:

```sh
cargo fmt
cargo clippy
cargo test
```
