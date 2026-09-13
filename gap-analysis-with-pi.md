# `art` production-readiness gap analysis versus Pi

Scope: core agentic runtime loop and tools only. This report ignores UI/CLI concerns, extensions, TypeScript-vs-Rust differences, subagents, multi-provider support beyond OpenRouter, and tests/conformance. MCP and skill support are included only as known gaps, because the team already called them out.

## Answer

`art` is not ready to replace Pi for daily production use yet.

It has the essential skeleton of a coding agent: it builds an OpenRouter chat-completions request, streams assistant output, accumulates streamed tool-call deltas, executes local `read`/`bash`/`write`/`edit` tools, appends tool results to the message history, and continues until the assistant stops requesting tools. That is enough for small tasks.

The gap is reliability under ordinary daily-agent workloads. Pi has a runtime controller around the model loop: session persistence, queued steering/follow-up messages, abort/retry behavior, context accounting and compaction, robust shell execution, output truncation with full-output preservation, richer file/search tools, and safer file mutation serialization. `art` currently has a single in-memory conversation loop driven by a prompt file and a small tool set. Long sessions, large command output, hung commands, interrupted work, context overflow, and concurrent/adjacent file edits are the main reasons it is not production-ready.

## What `art` already has

`art` is intentionally small and direct:

- `src/bin/art/agent.rs` runs a loop over model calls. `run_single` sends a request, streams chunks through `ActivePrompt`, collects assistant content/reasoning/tool calls, runs all returned tool calls, appends an assistant message plus tool-result messages, and continues if there was at least one tool call.
- `src/input/prompt.rs` handles OpenRouter streaming: DNS/TLS/HTTP, SSE parsing, usage/stat collection, streamed reasoning, annotations, and streamed tool-call delta accumulation keyed by tool-call index.
- `src/bin/art/tools.rs` implements four local tools: `read`, `bash`, `write`, and `edit`.
- OpenRouter server-side `web_search`/`web_fetch` can be included through the shared request builder when configured.
- `AGENTS.md` in the current directory is appended to the system prompt at startup.
- The conversation remains in memory across prompt-file turns during the same process.

This is a good prototype of the core agent loop. It is not yet the operational runtime Pi provides.

## Production gaps in the core runtime loop

### 1. No durable session model

Pi has a `SessionManager` with append-only JSONL entries, session IDs, timestamps, branch-aware context building, message persistence, model/thinking changes, compaction entries, and recovery from existing session files. `AgentSession` persists user, assistant, and tool-result messages on `message_end` events.

`art` keeps `messages: Vec<Message>` in memory and optionally logs raw request/response data through `Logger` when not private. There is no production session transcript for resuming an agent task after a crash, process restart, terminal loss, or machine reboot. The base `ort` CLI has a last-response mechanism, but `art` does not have Pi-equivalent durable agent sessions with tool-call history as first-class state.

Impact: daily work cannot safely span long tasks or interruptions. Losing the process loses the active plan, tool results, and conversation state.

Minimum needed: append-only session storage for user/assistant/tool messages and a way to resume by rebuilding the exact LLM context.

### 2. No context-window management or compaction

Pi estimates and tracks context usage, runs automatic threshold compaction, handles context-overflow and recoverable length stops, removes a failed/truncated assistant message from active state, compacts, and retries once. It also supports manual compaction and stores compaction summaries in the session tree.

`art` never estimates tokens, never checks the model context window, never summarizes old turns, and never recovers from context overflow. The message vector grows until OpenRouter/model failure. For a daily coding agent, this will happen routinely because tool results and file reads are large.

Impact: long sessions fail abruptly or degrade as context fills. After an overflow, the user must manually restart or reduce context.

Minimum needed: usage-based/estimated context accounting, a summarization call, compaction boundary storage, and overflow recovery that retries the interrupted turn once.

### 3. No automatic retry policy for transient model/provider errors

Pi detects retryable assistant errors and can auto-retry with bounded attempts and delay. It emits start/end retry events and resets retry state after a successful assistant response.

`art` mostly prints or propagates stream errors. Inside `run_single`, `active_prompt.next()` errors are printed and the loop continues; rate limits are treated specially by the writer, but there is no systematic retry policy for recoverable OpenRouter/provider/network failures.

Impact: transient 5xx, network flakes, malformed partial responses, and provider hiccups become user-visible task failures or ambiguous partial turns.

Minimum needed: classify retryable failures, retry the same model request with backoff, and ensure partial assistant/tool state is not appended as a successful turn.

### 4. No abort/cancellation model

Pi threads `AbortSignal` through the agent, tools, shell processes, compaction, retry, and extension hooks. It kills process trees for shell commands and waits for the agent to settle.

`art` has no internal cancellation state. For shell tools, `syscall::system` notes that the child remains in the parent foreground process group so terminal Ctrl-C reaches it, but the runtime does not own a cancellable process tree or reconcile partial tool output/state after cancellation.

Impact: a hung or long-running tool can block the agent loop. The runtime cannot reliably cancel one tool/turn, clean up, and continue the session.

Minimum needed: per-turn cancellation, per-tool cancellation, process-group management for bash, and state rules for aborted turns.

### 5. No queueing or steering while the model is running

Pi distinguishes steering messages from follow-up messages. Steering is delivered after the current assistant turn and its tool calls; follow-up is delivered when the agent finishes. This matters for long agent runs where the user needs to correct course without corrupting assistant/tool message ordering.

`art` watches a prompt file. After a tool turn it polls once for a new prompt; after the assistant stops it blocks waiting. It does not maintain separate steering/follow-up queues and cannot safely accept multiple user messages during an active run.

Impact: daily interactive use is brittle during long tasks. User corrections can be missed, delayed in surprising ways, or inserted only at coarse boundaries.

Minimum needed: an input queue with at least one safe steering lane delivered between model calls and after tool results.

### 6. Tool execution is synchronous and serial with no streaming updates

Pi streams bash output into an accumulator, emits throttled tool updates, and preserves state during abort/timeout. Tool result updates are part of the session event model.

`art` executes each tool synchronously inside the `Response::ToolCalls` handling block. Bash output is returned only after command completion. Other tools are also synchronous. Multiple tool calls from a single assistant message run serially.

Impact: long commands appear stuck, cannot be timed out from the runtime, and can monopolize the agent. Parallel independent tool calls are unavailable.

Minimum needed: async or evented tool execution for bash, runtime-owned timeout/abort, and streaming output snapshots.

## Tool gaps

### 1. Bash tool is not robust enough for production

Pi's bash tool supports optional timeout, process-tree kill, cwd validation, environment shaping, streamed stdout/stderr, binary/ANSI sanitization, tail truncation, and saving full output to a temp file when truncated.

`art`'s bash tool calls `syscall::system`, captures stdout and stderr to memory, and returns the full strings unless a model-provided line limit is set. There is no timeout parameter in practice beyond line limiting after completion. There is no byte cap, full-output file, binary sanitization, process-tree kill, or incremental output.

Impact: `yes`, verbose builds, test logs, binary output, or a hung server command can hang or bloat the agent process and context.

Minimum needed: timeout, byte-bounded output capture, tail truncation, full-output temp file, binary/ANSI cleanup, and process-group termination.

### 2. Missing dedicated search/list tools

Pi exposes `grep`, `find`, and `ls` tools in addition to `read`, `bash`, `write`, and `edit`. These tools respect ignore behavior, return compact structured output, impose match/result limits, truncate long lines, and avoid forcing the model to craft shell commands for common exploration tasks.

`art` only has `bash` for listing/searching. The system prompt tells the model to use `bash` with `rg`, `find`, and `ls`.

Impact: common repository exploration is less reliable and more verbose. The model has to manage quoting, ignore rules, output limits, and command portability itself.

Minimum needed: add native `ls`, `grep`, and `find` tools with bounded output. Since this project targets Linux, these can wrap existing commands or implement simple native traversal/search later.

### 3. Read tool lacks byte-based truncation and image support

Pi's read tool handles text and images, auto-resizes images, warns when the current model cannot accept images, applies both line and byte limits, reports truncation metadata, and tells the model how to continue with offsets.

`art` detects text by scanning for NUL bytes and line-numbers text output, but still reads through lines into memory up to the line limit plus one. The default limit is 2000 lines, but there is no 50KB-style byte cap. Binary files are not safely represented; if a file has no NUL in the first 8KB but contains huge or unusual content, it can produce poor context. Image files are not returned as model image blocks by the tool.

Impact: large/minified files, generated files, logs, and non-text assets can waste context or produce unreadable output.

Minimum needed: byte cap, explicit truncation metadata, binary-file refusal/summary, and optionally image attachments if daily workflows use screenshots or diagrams.

### 4. Edit/write tools need production-grade mutation behavior

Pi serializes mutations per real path with `withFileMutationQueue`, resolves paths relative to cwd, normalizes path quirks, supports multiple non-overlapping replacements in one edit call, preserves BOM handling, and returns a diff/patch plus first changed line.

`art`'s `edit` handles one replacement span, or all occurrences if `expected_occurrences` is provided. It validates ambiguous single replacements, which is good. It then rewrites the whole file. `write` refuses overwrites unless requested and creates parent directories. However, there is no per-file mutation queue, no multi-edit support, no diff metadata returned to the model, no atomic temp-file rename, and `write` uses a fixed 128-byte stack path buffer before copying the path bytes.

Impact: multi-location edits are more error-prone and require repeated tool calls. Adjacent edits can race if the runtime ever gains parallel tool execution. Long paths risk failure/panic in `write`. Lack of diff feedback makes it harder for the model to verify edits efficiently.

Minimum needed: remove fixed path buffer, return diff metadata, add multi-edit exact replacements, and serialize mutations per canonical path.

### 5. Tool result format is too thin

Pi tools return content plus structured details, including truncation metadata, full-output paths, diffs, patches, and image blocks. Those details support both rendering and follow-up reasoning.

`art` tools return JSON strings that mostly contain success, raw output, and basic path/count fields. Errors are hand-built JSON strings; `error()` does not JSON-escape the error message, so a quote/newline in an error can produce invalid JSON for the model.

Impact: the model gets less actionable metadata, and malformed error JSON can poison a tool-result turn.

Minimum needed: central JSON-safe tool result builder for success/error and consistent metadata fields for truncation, diffs, and file paths.

## Known gaps already identified by the team

### MCP support is missing

This is a blocker if daily Pi usage depends on MCP tools. It is out of scope for discovery here because the team already identified it, but it is part of the production readiness decision.

### Skill support is missing

`art` appends `AGENTS.md`, but it does not discover skills, advertise available skills, load `SKILL.md`, or follow skill workflows. This is a blocker if current daily Pi workflows depend on skills. It is also already known by the team.

## Non-blocking or lower-priority differences within scope

- Pi has model/thinking-level mutation inside a running session. The team only uses OpenRouter, so this is useful but not a blocker if `art.cfg` and CLI flags cover the daily model choice.
- Pi has richer event emission. This matters because it enables robust persistence, compaction, abort, and tool updates. The event API itself is less important than the state transitions it supports.
- Pi normalizes tool-result images. This only matters if image-producing tools or screenshot workflows are part of daily use.

## Shortest path to daily-use readiness

The shortest path is not to port all of Pi. Keep `art` small, but add the runtime guardrails that prevent lost work, hung runs, and context failures.

### Phase 1: make local tool use safe and bounded

1. Replace `bash` execution with a runtime-owned process runner:
   - optional timeout;
   - process group/session isolation;
   - kill process tree on abort/timeout;
   - combined byte-bounded output capture;
   - tail truncation;
   - full-output temp file when truncated;
   - binary/ANSI sanitization.
2. Put hard byte caps on all tool results, especially `read` and `bash`.
3. Make all tool success/error results JSON-safe through one helper.
4. Fix `write` path handling so long paths cannot overrun the fixed 128-byte buffer.
5. Add native `ls`, `grep`, and `find` tools with default limits.

This phase makes short and medium tasks much safer without changing the model loop architecture.

### Phase 2: add durable sessions

1. Persist every user, assistant, and tool message as append-only JSONL.
2. Store enough metadata to rebuild the exact active LLM context: system prompt, model, reasoning fields if needed, tool calls, tool-call IDs, tool results, timestamps, and cwd.
3. Add resume/load support for the latest or named session.
4. Keep raw request/response logging separate from the semantic session transcript.

This phase removes the biggest daily-use operational risk: losing work or context when `art` exits.

### Phase 3: add context accounting and compaction

1. Track usage returned by OpenRouter and estimate tokens after tool calls when usage is missing.
2. Define context limits/reserve tokens per configured model.
3. Add automatic threshold compaction using a summarization request.
4. Store compaction summaries in the session transcript and rebuild context from the latest compaction boundary.
5. Detect context-overflow/recoverable-length failures, remove the failed assistant from active context, compact, and retry once.

This phase is the difference between a demo loop and an agent you can leave running on real repository work.

### Phase 4: add retry and cancellation semantics

1. Classify retryable OpenRouter/provider/network errors.
2. Retry failed model calls with bounded backoff without committing partial turns as successful state.
3. Add per-turn cancellation state and thread it through model streaming and tools.
4. Add a small steering queue so user corrections can be delivered between model calls after tool results.

This phase makes the runtime predictable during long or flaky sessions.

### Phase 5: add MCP and skills if they are required for parity

MCP and skills are already known gaps. If current daily Pi usage relies on them, they must land before migration. If not, they can follow the core reliability work above.

## Recommended cut line

For daily replacement of Pi, the minimum acceptable cut is Phases 1 through 3, plus MCP/skills if the team relies on them every day.

Phases 1 and 2 make `art` safe enough for real tasks that fit in context. Phase 3 makes it safe enough for long-running repository work. Phase 4 can follow shortly after, but some cancellation and bash timeout work from Phase 4 should be pulled into Phase 1 because hung commands are a production blocker.

If the team wants the absolute shortest trial path, use `art` internally only after Phase 1, with clear limits: one-session tasks, manual restarts, no expectation of context recovery, and no reliance on MCP/skills. That is a pilot, not a Pi replacement.

## Final readiness judgment

`art` is close to a useful minimal coding-agent prototype, but not production-ready as the team's daily Pi replacement. The shortest credible path is to harden tools first, add durable sessions second, and add compaction third. Those three areas cover the core failure modes that will otherwise interrupt daily use: hung commands, unbounded output, lost state, and context overflow.
