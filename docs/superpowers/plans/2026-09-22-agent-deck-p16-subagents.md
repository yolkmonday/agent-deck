# Agent Deck P16: Show running subagents inside the parent card

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A Claude session that has dispatched subagents lists them inside its own card, and its token and cost figures include them.

**Architecture:** The parent transcript already tells us which agents have finished. A new `subagent` module pairs that with the `subagents/` directory to find the ones still running, tails each one's transcript with the existing `TranscriptState`, and returns them on the parent `Session`. The card renders them as rows beneath the activity box.

**Depends on:** P1 (transcript tailer), P11 (pricing).

## What exists today

Subagents are **invisible** in the dashboard. They have no process of their own, so they never appear
in `~/.claude/sessions/<pid>.json` and therefore never become a card. P1 also explicitly left their
tokens out of the parent's total. This phase closes both gaps.

The data is already on disk and is rich:

```
~/.claude/projects/<enc-cwd>/<sessionId>/subagents/
  agent-<agentId>.jsonl        # same record shape as a parent transcript, isSidechain: true
  agent-<agentId>.meta.json    # { agentType, description, model, requestShape, spawnDepth, toolUseId }
```

A real example from this machine:
`agentType: "scout"`, `model: "sonnet"`, `spawnDepth: 1`, `requestShape: "background"`,
`description: "Research Iconify icon names"`.

## Decisions taken with the user

- **Only subagents that are still running are listed.** Finished ones are not shown on the Live
  board; the Timeline is where history belongs. This keeps the card short and keeps the board a
  picture of *now*.
- **Tokens are shown per subagent AND folded into the parent's total**, so the card's headline
  number and the KPI row reflect what the session actually costs.

## Global Constraints

Same as previous phases. Plus:

- **No double counting.** A parent transcript does not contain its subagents' usage (verified in the
  P1 probe), so adding them is correct. A test must pin this.
- The per-second loop must stay cheap: one `read_dir` per Claude session, and `meta.json` parsed once
  per agent and then cached. Never re-read a meta file that has not changed.
- UI text Indonesian, code and commit messages English. No `Co-Authored-By`. Never push.

## Command API (the contract)

`Session` gains:

```ts
subagents: SubAgent[];   // running only, newest first
interface SubAgent {
  id: string;             // the agentId from the filename
  agentType: string;      // "scout", "Explore", "code-reviewer", ...
  description: string;    // the short task description
  model: string | null;
  tokens: TokenUsage;
  costUsd: number;
  priced: boolean;
  startedMs: number | null;
}
```

`Session.tokens` and `Session.costUsd` INCLUDE the running subagents. Add `ownTokens: TokenUsage`
so the parent's own usage is still available and the UI can show the split if it ever needs to.

---

### Task 1: Track finished agents in the parent transcript

**Files:** Modify `src-tauri/crates/collector/src/transcript.rs`

**Interfaces:**
- `TranscriptState` gains `pub agents_done: std::collections::HashSet<String>`.
- In `apply_user`, a `tool_result` item is already handled; additionally, when the record has a
  `toolUseResult` object carrying an `agentId` string, insert that id into `agents_done`.
  The real shape is `{"toolUseResult": {"agentId": "...", "status": "...", ...}}` on a `user` record.
- A truncation reset must clear it, which `TranscriptState::default()` already handles.

- [ ] **Step 1: Write the failing tests:**
1. `agents_done_collects_agent_ids_from_tool_use_result`: a user record with `toolUseResult.agentId = "a1"` puts `a1` in the set.
2. `agents_done_ignores_a_tool_result_without_an_agent_id`: an ordinary `toolUseResult` without `agentId` adds nothing.
3. `agents_done_survives_incremental_reads`: write one, advance, write another, advance; both ids are present.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 3 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): track which subagents have finished"`

---

### Task 2: The subagent module

**Files:** Create `src-tauri/crates/collector/src/subagent.rs`; modify `src-tauri/crates/collector/src/lib.rs`

**Interfaces:**
- `struct SubAgentMeta { pub agent_type: String, pub description: String, pub model: Option<String> }`
- `fn read_meta(path: &Path) -> Option<SubAgentMeta>` — parses `agent-<id>.meta.json` defensively with
  `serde_json::Value`; a missing key yields an empty string rather than failing the whole read.
- `fn agent_id_from(file_name: &str) -> Option<String>` — `"agent-a357b1.jsonl"` → `"a357b1"`;
  returns `None` for anything not matching `agent-<id>.jsonl`.
- `struct SubAgentScan { pub id: String, pub jsonl: PathBuf, pub meta_path: PathBuf }`
- `fn scan(session_dir: &Path) -> Vec<SubAgentScan>` — lists `subagents/agent-*.jsonl`; returns an
  empty vec when the directory does not exist. It must NOT return `.meta.json` files as entries.
- `struct SubAgent { pub id: String, pub agent_type: String, pub description: String, pub model: Option<String>, pub tokens: TokenUsage, pub cost_usd: f64, pub priced: bool, pub started_ms: Option<i64> }`
  with serde camelCase.

- [ ] **Step 1: Write the failing tests** (all with `tempfile`):
1. `agent_id_from_parses_the_filename`: valid case; and `"agent-x.meta.json"`, `"notes.jsonl"`, `"agent-.jsonl"` all give `None`.
2. `scan_lists_only_jsonl_entries`: a directory with two `.jsonl` and two `.meta.json` returns 2 entries.
3. `scan_on_a_missing_directory_is_empty`.
4. `read_meta_reads_the_real_shape`: a file with `agentType`, `description`, `model` parses.
5. `read_meta_tolerates_missing_keys`: `{}` yields empty strings and `None` model.
6. `read_meta_on_invalid_json_returns_none`.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 6 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): add subagent scanning and metadata"`

---

### Task 3: Wire subagents into the live session

**Files:** Modify `src-tauri/crates/collector/src/live.rs`, `src-tauri/crates/collector/src/model.rs`

**Interfaces:**
- `Session` gains `pub subagents: Vec<SubAgent>` and `pub own_tokens: TokenUsage` (serde camelCase).
  `same_content` must compare both.
- `LiveCollector` gains `subagents: HashMap<(String /*session key*/, String /*agent id*/), (PathBuf, TranscriptState)>`
  and `meta_cache: HashMap<PathBuf, (i64 /*mtime ms*/, SubAgentMeta)>`.
- In `claude_session`, after advancing the parent transcript:
  - derive the session directory as the parent transcript path with its `.jsonl` extension removed
  - `scan` it, and keep only entries whose id is NOT in the parent's `agents_done`
  - for each remaining entry: read the meta through `meta_cache` (re-read only when the file's mtime
    changed), advance its own `TranscriptState`, and build a `SubAgent` with
    `cost_usd` from the price table and `started_ms` from the subagent transcript's first record
  - sort newest first by `started_ms`, with `None` last
  - set `own_tokens` to the parent's own usage, then add every subagent's usage into `tokens`, and
    set `cost_usd` to the parent's cost plus each subagent's cost
- Drop cache entries for sessions that are no longer live, exactly as the parent map already does.

- [ ] **Step 1: Write the failing tests** in `live.rs`:
1. `running_subagents_appear_on_the_parent`: a session with one subagent file and no matching `toolUseResult` yields one entry with the meta's agentType and description.
2. `finished_subagents_are_hidden`: the parent has a `toolUseResult` with that `agentId`; the list is empty.
3. `subagent_tokens_are_added_to_the_parent`: parent output 10, subagent output 5 → `tokens.output == 15` and `own_tokens.output == 10`.
4. `subagent_cost_is_added_to_the_parent`: with a known model on both, the parent cost equals the sum, within `1e-9`.
5. `a_session_without_a_subagent_directory_has_an_empty_list`.
6. `subagents_sort_newest_first`.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.** Update every existing `snapshot(...)` assertion that now sees new fields.
- [ ] **Step 4: Run to verify it passes** — 6 new tests, every pre-existing collector test still green.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): fold running subagents into the parent session"`

---

### Task 4: Render them in the card

**Files:** Modify `src/lib/types.ts`, `src/components/SessionCard.tsx`

**Interfaces:**
- Types mirror the Command API exactly.
- Below the activity box, when `subagents.length > 0`, render a block:
  - a header line `Subagent (N)` in `text-fg-3`, 11.5 px
  - one row per subagent: a small `lucide:git-branch` icon in `text-fg-3`, the `agentType` in
    semibold, the `description` truncated with a `title`, and on the right the token count and cost
    in monospace. A row with `priced: false` shows `-` for cost, matching the parent's rule.
  - rows are separated by a subtle top border, not a card each: they are part of the parent, not
    siblings of it
- The block sits between the activity box and the footer, so the footer stays pinned to the bottom by
  the existing `mt-auto`.
- Long lists must not stretch the card past its neighbours: cap the visible rows at 4 and, when there
  are more, append a final muted line `+N lagi`.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): list running subagents inside the parent card"`

---

## Done when

- The whole Rust suite passes with 15 new tests.
- `bunx tsc --noEmit`, `bun test`, `bun run build`, `cargo check --workspace` all clean.
- One commit per task, no `Co-Authored-By`, nothing pushed.

## Self-Review

- **Coverage:** running subagents become visible, nested in the parent, with their own numbers, and
  the parent's total finally reflects the real cost.
- **Honesty:** `own_tokens` is kept alongside the combined `tokens` so the split is never lost, and
  Task 3 test 3 pins both numbers.
- **Known asymmetry, stated deliberately:** the history index (P2) counts each subagent transcript as
  its own set of messages, so historical per-session totals group differently from this live total.
  Reconciling the two groupings is a separate decision, not a bug to paper over here.
