# How Pi compacts conversation context

## Scope and main result

This report describes the source in this checkout, at commit `4c8eb393c73220c742e75745df210335aedc020e`. Paths below are relative to this folder (`packages/`). This is a source review, not a live-provider experiment.

**Pi's default compaction is a lossy rewrite of model context, not deletion of session history.** It:

1. Preserves the current system prompt and tool declarations as a separate checkpoint.
2. Keeps a recent tail of conversation messages without summarizing them.
3. Asks the current model to summarize the older conversation.
4. Stores a new compaction entry in its append-only session log.
5. Builds future requests from the system checkpoint, summary, retained tail, and new messages.

There is no embedding search, vector database, model-side memory, or automatic retrieval of discarded messages in this algorithm. Old messages remain available in the session file, but are not automatically supplied to subsequent model calls.

Manual `/compact` and automatic compaction use the same preparation and summary-generation functions. They differ in when they run and whether the interrupted request is retried.

## 1. What survives, and in what form?

| Context component | Default treatment |
|---|---|
| Current system prompt, named prompt sections, active tool declarations | Preserved as resolved state, not LLM-summarized. Historical updates are folded into one checkpoint. |
| Recent user messages | Preserved as complete messages in the retained tail. |
| Recent assistant messages | Preserved, including text, stored thinking blocks, and tool calls. |
| Recent tool results | Preserved as stored. The compactor does not shorten retained results. |
| Recent images | Preserved in retained message content, subject to the normal provider/image handling outside compaction. |
| Older user/assistant messages and tool calls | Replaced by a generated text summary. There is no guarantee that every instruction or fact survives. |
| Older tool-result text | Only its first 2,000 characters per result are shown to the summarizer. The summary replaces the original result in future context. |
| Older images | Not sent to the summarizer. Only text blocks are serialized. Earlier textual descriptions can survive through the summary. |
| Previous compaction summary | Usually supplied to the summarizer for an iterative update, rather than keeping a growing list of summaries. There is one split-turn exception described below. |
| Paths of files read or modified | Also preserved through deterministic, cumulative path lists. File contents are not preserved by this mechanism. |
| Metadata such as labels, model changes, usage records, extension state | Remains in storage. Normally not conversation text and not sent to the summarizer. |
| Messages omitted by a context edit | Not included in cut selection, summary input, or projected token estimates. Their raw entries remain stored. |

“Unchanged” means unchanged by the compactor relative to the current projected context. A prior context edit may already have replaced message content. Provider adapters and request-time extensions can also transform messages independently.

**There is no special protection for the first user message.** Once it falls in the old prefix, it is summarized. Instructions in system state survive separately; instructions given as old user messages depend on the summary.

### System state is a distinct kind of compaction

System messages can carry prompt content, section patches, and tool additions/removals. `getCurrentSystemMessage()` folds these updates:

- Nonempty system text is joined with blank lines.
- Named sections are updated by name; a `null` value deletes a section.
- Tool removals delete definitions; additions add or replace definitions by tool name.

`appendCompaction()` saves this resolved `SystemMessage` on the compaction entry, with a new timestamp. Historical system-message entries in the retained pre-compaction range are not replayed again. The resolved prompt and tool state are preserved, but the history of how they changed is no longer model context.

Thus project instructions, skill listings, and other information present in the effective system state are not entrusted to the summarizer. Skill content or files loaded into ordinary conversation messages have no comparable special exemption.

Sources: [session-manager.ts](coding-agent/src/core/session-manager.ts), `appendCompaction()`, `buildContextEntries()`, `buildSessionProjection()`; [transcript.ts](ai/src/utils/transcript.ts), `getCurrentSystemMessage()`.

## 2. Storage and reconstruction

Pi stores session entries as an append-only JSONL tree. Each entry has an `id`, `parentId`, and timestamp. Compaction operates on the path from the root to the active leaf, not on every branch in the file.

The important compaction record is:

```typescript
interface CompactionEntry {
  type: "compaction";
  id: string;
  parentId: string | null;
  timestamp: string;
  summary: string;
  firstKeptEntryId: string;
  tokensBefore: number;
  systemMessage?: SystemMessage;
  details?: { readFiles: string[]; modifiedFiles: string[] };
  usage?: Usage;
  fromHook?: boolean;
}
```

`details` can have a different shape for extension-generated summaries. `usage` records the summary call's billing usage, not the size of the new main-agent context.

Example:

```text
Stored before:
  system, U1, A1, T1, U2, A2, T2, A3

Append:
  C1(summary of U1/A1/T1, firstKeptEntryId = U2)

Main-agent context after:
  resolved system checkpoint
  summary of U1/A1/T1
  U2, A2, T2, A3
```

The old entries are not rewritten or removed.

Reconstruction finds the newest compaction on the active path, then selects:

1. That compaction's system checkpoint and summary.
2. Entries from `firstKeptEntryId` up to, but not including, the compaction entry.
3. All entries appended after the compaction.

Older compaction entries that happen to lie inside the kept raw range do not contribute another summary. Earlier system-message entries in that range are skipped because the checkpoint already contains their resolved state.

For retain-none compactions, the recorded kept ID can be the compaction's own ID. If a kept ID is missing from the earlier path, reconstruction retains no pre-compaction tail and proceeds with entries after the checkpoint.

### Summary role in the next real request

Internally the summary has role `compactionSummary`. `convertToLlm()` turns it into a **user-role** text message:

```text
The conversation history before this point was compacted into the following summary:

<summary>
SUMMARY TEXT
</summary>
```

It is not installed as the system prompt or an assistant response.

### Context edits

A `context_edit` record targets an earlier entry. `replacement: null` omits it; a non-null replacement changes its content without changing its other metadata. The latest applicable edit wins.

Pi builds one canonical projection—the active model-visible view with compaction and edits applied—and uses it for summary preparation and request reconstruction. This prevents a discarded failed attempt or superseded tool output from reappearing through summarization.

Sources: [session-manager.ts](coding-agent/src/core/session-manager.ts), `buildSessionProjection()`; [messages.ts](coding-agent/src/core/messages.ts), `convertToLlm()`.

## 3. How Pi chooses what to keep

Defaults:

```json
{
  "compaction": {
    "enabled": true,
    "reserveTokens": 16384,
    "keepRecentTokens": 20000
  }
}
```

`keepRecentTokens` controls the cut, not the size of the summary. It is an approximate tail budget, not a hard maximum or minimum.

### Preparation algorithm

`prepareCompaction(pathEntries, settings)`:

1. Returns no preparation if the last raw entry is already a compaction.
2. Builds the canonical projection.
3. Finds the previous active compaction, if any. Its summary becomes `previousSummary`; the candidates start immediately after that projected checkpoint. This includes messages retained by the previous compaction, even if they precede it in storage order.
4. Estimates the actual projected context size as `tokensBefore`.
5. Walks candidate entries backward, adding per-message size estimates, until the total reaches `keepRecentTokens`.
6. Chooses a legal kept boundary near that entry.
7. Separates the old prefix into complete history and, if needed, the beginning of a split turn.
8. Extracts file-operation paths from the messages being summarized.
9. Returns no preparation if both summary inputs are empty.

The active implementation uses the private `findProjectedCutPoint()`. The exported `findCutPoint()` is similar, but does not implement all projection/recovery handling. Use the projected version as the reference for reproducing actual session behavior.

### Legal boundaries

A boundary can start at:

- A user message.
- An assistant message.
- A bash-execution message.
- A custom message or branch-summary message.

It cannot start at a tool result or a system message. Actual compaction entries are excluded as candidates.

Once the reverse scan reaches the budget, Pi selects the first legal cut at or after that entry. If none exists, it uses the final legal cut. This last case keeps the preceding assistant call when trailing tool results alone exceed the budget.

Example with artificial token sizes:

```text
U1(100), A1-call(100), T1(1000), A2-call(100), T2(1000)
keepRecentTokens = 1200

Reverse scan: T2=1000; +A2=1100; +T1=2100.
First valid cut at/after T1 is A2.
Keep A2 + T2, approximately 1100 tokens.
```

If `T2` alone exceeded the budget and there were no later legal boundary, Pi would still retain `A2 + T2`, exceeding the target. It does not split the body of `T2`.

The algorithm also moves the boundary backward over adjacent entries that contribute no context, stopping at a visible entry or compaction. Such bookkeeping can therefore be part of the retained range.

A zero tail budget does not mean “keep no messages.” The scan still chooses a legal message boundary. A single oversized user message generally cannot be compacted in isolation because there is no older prefix to summarize.

### Split turns

For compaction, a turn starts at a user-like message and includes subsequent assistant messages and results until the next user-like message. Bash executions, custom messages, and summaries count as user-like starts.

When the cut is inside such a turn:

```text
[complete older turns] [user request + early work] [recent assistant + results]
       history              turn prefix                  kept suffix
```

The inputs are:

- `messagesToSummarize`: complete history before the split turn.
- `turnPrefixMessages`: the turn's beginning, from its initiating user-like message up to the kept boundary.
- The retained suffix: not included in either summary request.

This is not “always keep complete turns.” Assistant boundaries allow a very long tool-using turn to be reduced.

### Recovery-only boundary adjustment

A special case allows Pi to summarize a large input followed only by an omitted failed assistant attempt and invisible bookkeeping. It advances the cut into that invisible suffix if:

- The reverse scan reached the budget.
- The suffix contains an omitted assistant attempt.
- It contains no unomitted context-producing entry or compaction.
- It has no content replacement affecting an entry outside the omitted suffix.

Arbitrary metadata alone does not permit this advancement. Nor does a newly appended visible custom message. This helps compact an oversized recovered input without treating arbitrary bookkeeping as permission to discard unsent input.

Source: [compaction.ts](coding-agent/src/core/compaction/compaction.ts), `prepareCompaction()`, `findProjectedCutPoint()`.

## 4. Exactly what is sent to the summarization model

### Model and request configuration

By default, Pi uses the **currently selected model**, with its resolved authentication, headers, and endpoint. It does not select a cheaper or larger summarizer automatically.

It invokes the configured stream function, or `completeSimple()` for direct helper calls, with a standalone context:

```text
system: dedicated summarization instruction
user:   serialized old conversation + optional previous summary + format instructions
```

The normal coding-agent system prompt and active tool declarations are **not** included in this context. No tools are supplied for the summary request. Tool calls in the old conversation are plain text, not executable requests.

Options include:

- `maxTokens` as described below.
- The abort signal and resolved auth information.
- The session's thinking level when the model supports reasoning and thinking is not off.
- `cacheRetention: "none"`.
- A fresh UUID routing session ID by default, rather than the main conversation's routing ID.

A direct helper caller can supply a routing ID. The built-in session path does not. Cache retention is a request option; its effect depends on provider support.

### Exact summarization system instruction

```text
You are a context summarization assistant. Your task is to read a conversation between a user and an AI assistant, then produce a structured summary following the exact format specified.

Do NOT continue the conversation. Do NOT respond to any questions in the conversation. ONLY output the structured summary.
```

### Conversation serialization

Pi first calls `convertToLlm()`, then serializes messages as plain text:

```text
[User]: Fix the parser.

[Assistant thinking]: Stored reasoning text, if present.

[Assistant]: I found the problem.

[Assistant tool calls]: read(path="src/parser.ts"); edit(path="src/parser.ts", ...)

[Tool result]: Tool output text.
```

Rules:

- User text blocks are concatenated without a separator.
- Assistant thinking blocks are joined with newlines and labeled separately.
- Assistant text blocks are joined with newlines.
- Each tool call becomes `name(key=JSON.stringify(value), ...)`. Multiple calls are separated with `; `.
- Tool-result text blocks are concatenated without a separator and truncated to the first **2,000 characters per message**.
- A truncated result receives `\n\n[... N more characters truncated]`.
- Images are not serialized, nor are thinking signatures, call IDs, timestamps, usage, tool-result metadata, or structured error flags. Error text inside content remains text.
- User and assistant text, thinking, and tool arguments are not capped by this serializer.
- Bash-execution messages become user text containing the command, output, and applicable exit/cancellation/truncation notices. They therefore do **not** receive the 2,000-character tool-result cap.
- `!!` bash executions marked `excludeFromContext` are filtered out by `convertToLlm()`.
- Custom-message content becomes user content; its display flag does not exclude it from context.
- Branch summaries become wrapped user text.
- System state is excluded from summary preparation and is not serialized here.

Consequently the summarizer cannot know details that exist only after character 2,000 of an old tool result, or only inside an old image, unless another included message already describes them.

### Normal history request

The single user message is constructed as:

```text
<conversation>
SERIALIZED MESSAGES TO SUMMARIZE
</conversation>

<previous-summary>
PREVIOUS SUMMARY, IF ANY
</previous-summary>

INITIAL OR UPDATE INSTRUCTIONS

Additional focus: OPTIONAL /compact INSTRUCTIONS
```

The previous-summary block and additional-focus line are omitted when absent. The recent retained tail is not sent to the summarizer.

The initial instructions ask for this exact Markdown structure:

```markdown
## Goal
[What is the user trying to accomplish? Can be multiple items if the session covers different tasks.]

## Constraints & Preferences
- [Any constraints, preferences, or requirements mentioned by user]
- [Or "(none)" if none were mentioned]

## Progress
### Done
- [x] [Completed tasks/changes]

### In Progress
- [ ] [Current work]

### Blocked
- [Issues preventing progress, if any]

## Key Decisions
- **[Decision]**: [Brief rationale]

## Next Steps
1. [Ordered list of what should happen next]

## Critical Context
- [Any data, examples, or references needed to continue]
- [Or "(none)" if not applicable]
```

It ends: `Keep each section concise. Preserve exact file paths, function names, and error messages.`

For repeated compaction, the update prompt uses the same headings and instructs the model to:

- Preserve existing information.
- Add new progress, decisions, and context.
- Move completed tasks from In Progress to Done.
- Update Next Steps.
- Preserve exact paths, function names, and errors.
- Remove information that is no longer relevant.

These are natural-language requests, not enforced retention guarantees. The old summary is itself subject to rewriting and loss.

### Split-turn requests

When a turn is split, Pi makes up to two calls, **sequentially**:

1. Summarize complete earlier history, incorporating the previous summary if available.
2. Separately summarize the turn prefix.

If there is no new complete history, call 1 is skipped. Pi preserves the previous summary verbatim, or uses `No prior history.` if none exists.

Call 2 receives only:

```text
<conversation>
SERIALIZED TURN PREFIX
</conversation>

This is the PREFIX of a turn that was too large to keep. The SUFFIX (recent work) is retained.

Summarize the prefix to provide context for the retained suffix:

## Original Request
[What did the user ask for in this turn?]

## Early Progress
- [Key decisions and work done in the prefix]

## Context for Suffix
- [Information needed to understand the retained recent work]

Be concise. Focus on what's needed to understand the kept suffix.
```

It does **not** receive the retained suffix, history summary, previous summary, or custom `/compact` focus instructions. Custom focus applies only to the history summary path.

The program joins the results; there is no third LLM merge call:

```text
HISTORY SUMMARY

---

**Turn Context (split turn):**

TURN PREFIX SUMMARY
```

### Output limits and validation

Requested history-summary output limit:

```text
min(floor(0.8 × reserveTokens), positive model.maxTokens or infinity)
```

Turn-prefix output limit:

```text
min(floor(0.5 × reserveTokens), positive model.maxTokens or infinity)
```

With default reserve, these are 13,107 and 8,192 tokens before the model cap. They are ceilings, not target lengths. Split summaries have separate budgets, so their combined ceiling can exceed `reserveTokens`. Provider-specific reasoning/output handling can further affect the actual request.

Pi rejects summary responses with `stopReason: "error"`, `stopReason: "length"`, or any tool call. A length-limited partial summary is not a safe checkpoint. The session path also checks cancellation before appending.

Only returned text blocks become the summary; generated reasoning is not stored as summary text. There is no parser validating the requested headings and no general nonempty-summary check in these helpers.

Transient summary-call errors use the configured retry policy. Deterministic errors and cancellation are not handled by recursively compacting the summary request.

**There is no general input-budget packing or recursive chunk summarization in this default path.** Removing the recent tail and truncating tool results reduces input, but very large user messages, thinking blocks, tool arguments, or previous summaries can still make the summary request too large.

Sources: [compaction.ts](coding-agent/src/core/compaction/compaction.ts), `generateSummaryWithUsage()`, `generateTurnPrefixSummary()`, `completeSummarization()`, `compact()`; [utils.ts](coding-agent/src/core/compaction/utils.ts), `serializeConversation()`.

## 5. Deterministic file-path preservation

In addition to the model's prose, Pi tracks paths from assistant tool calls:

- `read` with a string `arguments.path` adds a read path.
- `write` adds a written path.
- `edit` adds an edited path.

It combines these with the previous built-in compaction's `details.readFiles` and `details.modifiedFiles`. Previous extension-generated details (`fromHook`) are not assumed to have this shape.

After generating the summary:

1. Modified paths are the union of written and edited paths.
2. Read-only paths exclude all modified paths.
3. Both lists are deduplicated and sorted.
4. Nonempty lists are appended to the text:

```xml
<read-files>
src/input.ts
</read-files>

<modified-files>
src/parser.ts
</modified-files>
```

The same arrays are persisted in `details` for the next compaction.

This preserves **path strings**, not file versions, contents, diffs, or proof that operations succeeded. Extraction examines calls, not their success results. It does not infer file changes from shell commands or arbitrary custom tools. Recent kept calls are not added to these lists until they are included in a later summarized prefix.

Implementation caveat: the compaction path extractor directly imports the previous compaction's details and scans assistant calls. It does not directly merge arbitrary branch-summary details. A branch summary's text can still enter the model-generated summary. Do not infer a broader metadata merge from the documentation's general description of cumulative tracking.

Source: [utils.ts](coding-agent/src/core/compaction/utils.ts), file-operation helpers; [compaction.ts](coding-agent/src/core/compaction/compaction.ts), `extractFileOperations()`.

## 6. Token estimation

Pi does not run a model tokenizer for this compaction algorithm. It combines provider-reported usage with a character-based estimate.

### A. Provider usage as an anchor

For an assistant response:

```text
contextTokens = usage.totalTokens, if nonzero
             else usage.input + usage.output + usage.cacheRead + usage.cacheWrite
```

This represents the request context plus the generated response. Cached tokens still occupy context, so they count. This is **not** the cumulative token spend of the session.

`estimateContextTokens(messages)` searches backward for the latest assistant response with:

- A stop reason other than `error` or `aborted`.
- Positive calculated usage.

Then:

```text
estimate = that response's contextTokens
         + estimated sizes of every message after that response
```

For example, usage of 150,000 followed by tool results estimated at 9,000 gives 159,000. The assistant's output is already in the usage anchor and must not be counted twice. With no valid anchor, all messages are estimated locally.

All-zero usage is treated as missing measurement, not as evidence of an empty context. Failed and aborted responses cannot replace a valid earlier anchor in this estimator.

### B. Per-message character heuristic

For each message, Pi totals the following quantities, then uses `ceil(characters / 4)`:

| Role | Counted content |
|---|---|
| System | Text content; nonempty section strings; JSON of `toolsAdded`. |
| User | Text content; 4,800 synthetic characters per image. |
| Assistant | Text; thinking text; each tool name plus JSON of its arguments. |
| Tool result or custom | Text content; 4,800 synthetic characters per image. |
| Bash execution | Command plus output text. |
| Compaction or branch summary | Summary text. |

An image therefore costs an estimated 1,200 tokens, regardless of its actual dimensions or encoding. “Characters” here means JavaScript string length, not bytes or tokenizer units.

Rounding happens per message. Message framing, wrapper text, role names, call IDs, thinking signatures, most metadata, and tool-removal fields are not explicitly counted.

The source calls this conservative, but it is not a guaranteed upper bound. Tokenization varies by language, code, model, and image handling. The projection also exists before all request transforms. For example, an excluded bash execution can still contribute to this local estimate even though `convertToLlm()` filters it out.

### C. Invalidating old usage

A retained assistant message may report 180,000 tokens from before compaction, while the new context is only 25,000. Reusing that usage would immediately trigger another compaction.

`estimateProjectedContextTokens()` maps the usage anchor back to its source entry. It trusts it only if that entry follows the latest raw `compaction` or `context_edit` record. Otherwise it estimates:

```text
estimated resolved current system state
+ estimated non-system messages in the projection
```

This also avoids counting old system-section updates as if all remained simultaneously active. It is intentionally conservative about invalidation: any later context edit invalidates the old usage anchor.

Cut selection always uses individual message estimates. It cannot use assistant usage totals as message sizes because those totals include earlier history.

### D. UI count versus control-loop count

Immediately after compaction, `getContextUsage()` reports `tokens: null` and `percent: null` until a valid post-compaction assistant response exists. This avoids presenting stale provider usage as a measured count.

Internal compaction checks can still use a heuristic projection estimate. Successful compaction also returns `estimatedTokensAfter`, calculated from the rebuilt messages. That number is not a new provider measurement.

Sources: [compaction.ts](coding-agent/src/core/compaction/compaction.ts), token-calculation functions; [agent-session.ts](coding-agent/src/core/agent-session.ts), `getContextUsage()`.

## 7. Automatic triggers and recovery

### Threshold

The normal predicate is strictly:

```text
settings.enabled && contextTokens > model.contextWindow - reserveTokens
```

For a 200,000-token context window with defaults, it triggers above **183,616**, not at equality. This is an absolute reserve, not a percentage threshold.

The model's configured context window is used. It is not discovered from the prompt or learned from the server.

### When checks run

1. **Before a new user prompt:** `prompt()` checks the existing last assistant response, including an aborted one. This happens before the new prompt is appended. It does not automatically retry the old turn here.
2. **Between assistant responses in a running tool loop:** after assistant and tool results are persisted, `prepareNextTurn` checks the canonical projected context before the next response. This accounts for newly accumulated tool results. No between-turn check is needed if the loop ends without another response.
3. **After the low-level run ends:** `_checkCompaction()` handles the final response, including threshold checks and overflow/length recovery.

The pre-prompt check does not include the incoming prompt, and the between-turn check precedes appending prepared/queued messages for the next request. These checks are not a complete exact preflight tokenization of every outgoing payload.

The post-run/pre-prompt path has additional guards:

- Auto-compaction must be enabled.
- Post-run checks skip aborted responses; pre-prompt checks can include them.
- An assistant response older than the newest compaction is ignored.
- If context edits exist, use the projection-aware estimator.
- Otherwise, errors or zero usage fall back to the latest valid usage plus trailing estimates, with stale-usage protection.
- Otherwise, the checked response's usage is used directly.

The between-turn path uses the projection-aware estimate and requires a positive model context window.

### Overflow detection

[overflow.ts](ai/src/utils/overflow.ts) recognizes several signals:

- Provider error strings such as `prompt is too long`, `exceeds the context window`, `context_length_exceeded`, and related provider-specific forms.
- Known rate-limit/service-unavailable strings are excluded, so “too many tokens” in a throttling error does not necessarily mean context overflow.
- A successful `stop` response with `usage.input + usage.cacheRead > contextWindow`.
- A `length` response with zero output and input plus cache-read tokens at least 99% of the context window.

Separately, an early `length` response is recoverable when:

```text
model.maxTokens > 0 && usage.output < model.maxTokens
```

This compares against the intended model output limit, not a context-clamped request limit.

Pi checks that the response belongs to the current provider/model before applying overflow recovery. It also checks whether edits or a later compaction have already invalidated or omitted the selected attempt.

### Recovery sequence

For a failed overflow or recoverable final length response:

```text
persist attempted assistant response
→ finish turn and agent-end notifications
→ append omission edits for selected assistant and associated results
→ rebuild projected context
→ prepare and run compaction
→ append checkpoint on success
→ retry as a fresh agent run
```

The failed attempt remains in raw history for inspection and billing. It is not used as summary input after omission.

Only one compact-and-retry recovery attempt is allowed before reporting recovery failure. A new user message or a response that is neither error nor length resets the guard.

A successful response that exceeded the configured window is compacted without repeating the completed response. A normal threshold compaction likewise does not regenerate a successful answer. Queued work may still cause another run.

If recovery compaction fails or is cancelled, its omission edits remain, no compaction entry is appended, and no internal recovery retry is scheduled. Ordinary queue handling is separate.

A length-truncated assistant response containing tool calls is special: the low-level loop does not execute those potentially incomplete arguments. It creates failed tool results and follows normal tool/queue scheduling. Not every length stop immediately forces post-run compaction.

### Manual path

`AgentSession.compact(customInstructions)` first aborts and waits for the current operation. It then prepares, summarizes, appends the checkpoint, and refreshes context. It does not resume the interrupted turn automatically.

Manual compaction is allowed even when automatic compaction is disabled. It can fail with `Already compacted` or `Nothing to compact (session too small)` when preparation produces no older content.

Sources: [agent-session.ts](coding-agent/src/core/agent-session.ts), `compact()`, `_compactBeforeNextAssistantResponse()`, `_checkCompaction()`, `_runAutoCompaction()`; [agent-loop.ts](agent/src/agent-loop.ts).

## 8. Settings and extension behavior

Global settings and trusted project settings merge recursively. Each token setting then resolves independently:

```text
exact provider/modelId override → ordinary setting → built-in default
```

Example:

```json
{
  "compaction": {
    "reserveTokens": 16384,
    "keepRecentTokens": 20000,
    "modelOverrides": {
      "provider/large-model": { "reserveTokens": 400000 }
    }
  }
}
```

For a 1,000,000-token model, this triggers above 600,000 and still keeps roughly 20,000 recent tokens. It also increases the summary output allowance, subject to the model cap. `reserveTokens` serves both purposes.

Model keys are exact and case-sensitive. Token values must be non-negative safe integers. Zero is accepted, although zero reserve produces a zero requested summary output budget. `enabled` is global rather than model-specific.

Extensions receive `session_before_compact` with the preparation, raw branch entries, reason (`manual`, `threshold`, or `overflow`), custom instructions, `willRetry`, and abort signal. They can cancel or supply their own summary, kept boundary, usage, and details. Such an extension can replace the default behavior described here.

`session_compact` reports success; `session_compact_failed` reports failure/cancellation. Extensions can also append context edits and compaction drafts at lifecycle boundaries. A from-scratch default implementation does not need extension machinery, but should keep a clear boundary between preparation, generation, and commit.

Sources: [compaction documentation](coding-agent/docs/compaction.md), [settings documentation](coding-agent/docs/settings.md), and the session methods above.

## 9. Implementation recipe

The minimum design needed to reproduce the default behavior is:

```text
State:
  append-only entries with stable IDs
  current branch/leaf
  current system/tool state
  compaction settings
  model context/output limits
  one-attempt overflow recovery guard

compact(reason, optionalFocus):
  capture active model and resolved settings
  projection = build active context with latest checkpoint and edits
  preparation = choose cut and partition history / turn prefix / retained tail
  if no old content: stop without checkpoint

  if split turn:
    history = summarize(history, previousSummary, optionalFocus)
              if history has messages
              else previousSummary or "No prior history."
    prefix = summarizeTurnPrefix(turnPrefix)
    summary = concatenate(history, split marker, prefix)
  else:
    summary = summarize(history, previousSummary, optionalFocus)

  reject failed, truncated, tool-calling, or cancelled generation
  append sorted cumulative file-path lists
  append checkpoint(summary, firstKeptId, resolvedSystemState,
                    tokensBefore, fileDetails, summaryUsage)
  rebuild active messages
```

Important invariants:

1. Never keep tool results without their assistant tool call.
2. Never delete raw transcript history merely to shrink model context.
3. Never use pre-compaction usage as the new context size.
4. Use the same edited projection for cut selection, summarization, and requests.
5. Keep system/tool state out of the lossy conversation-summary path.
6. Do not replace a checkpoint with a partially generated summary.
7. Distinguish retrying an incomplete answer from repeating a successful answer.
8. Preserve pending user work and cancellation semantics while summarization runs.

### Suggested tests for an independent implementation

- Exact threshold equality versus one token above it.
- No usage, all-zero usage, error usage, and valid usage plus trailing tool results.
- A single turn larger than the retained budget.
- A huge final tool result that requires retaining its preceding call.
- Repeated compaction that summarizes previously retained messages exactly once.
- No new complete history in a split turn: preserve the previous summary and make only one call.
- Tool-result truncation at 2,000 characters; no accidental truncation of retained results.
- Old images excluded from summary input; retained images left intact.
- System-section and tool-set changes resolved correctly across a checkpoint.
- Edited/omitted content never reappearing through a later summary.
- Summary failure, cancellation, tool-call output, and length-limited output.
- Overflow retry capped at one attempt; no automatic repeat of a completed answer.

Useful existing reference tests include [compaction-serialization.test.ts](coding-agent/test/compaction-serialization.test.ts) and [compaction-summary-reasoning.test.ts](coding-agent/test/compaction-summary-reasoning.test.ts).

### Optional improvements, not claims about Pi

For a new implementation, consider an exact tokenizer or provider count endpoint, a hard budget for summary input, chunked summarization for huge inputs, explicit nonempty-summary validation, and checking that compaction actually reduced context. You may also want independent settings for trigger reserve and summary output length. Pi's default compaction code does not provide all of these safeguards.

## 10. Source map

| Source | What to copy or study |
|---|---|
| [compaction/compaction.ts](coding-agent/src/core/compaction/compaction.ts) | Defaults, estimates, actual cut algorithm, prompts, request construction, split-turn merge. |
| [compaction/utils.ts](coding-agent/src/core/compaction/utils.ts) | Text serialization, 2,000-character truncation, file lists, summarization system prompt. |
| [session-manager.ts](coding-agent/src/core/session-manager.ts) | Append-only checkpoint records, active branch projection, system snapshots, context edits. |
| [messages.ts](coding-agent/src/core/messages.ts) | Summary-to-user-message conversion and custom/bash conversion. |
| [agent-session.ts](coding-agent/src/core/agent-session.ts) | Manual/automatic lifecycle, threshold checks, overflow omission/retry, auth, callbacks, UI usage. |
| [agent-loop.ts](agent/src/agent-loop.ts) | Between-response scheduling, queue ordering, truncated tool-call handling. |
| [transcript.ts](ai/src/utils/transcript.ts) | Folding system prompt sections and tool declarations into current state. |
| [overflow.ts](ai/src/utils/overflow.ts) | Provider overflow patterns and early-length detection. |

Branch summarization on `/tree` is a separate feature. It shares summary utilities but has different history selection and budgets; it is not the implementation of `/compact` described here.
