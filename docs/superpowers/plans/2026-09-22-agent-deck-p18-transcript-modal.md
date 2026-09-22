# Agent Deck P18: Live transcript modal

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Click a running session and watch what it is actually doing, live, without leaving the dashboard.

**Architecture:** A `tail` command returns the last N interesting records of a session transcript plus a byte offset; the modal polls for the delta while it is open. Rendering uses the **brainless** components already installed — this is the surface they were designed for.

**Depends on:** P1 (transcript tailer), P7 (icons), P16 (subagents also get a viewer).

## Why brainless belongs here and not on the card

The card's thinking line is a status chip, so P14 stripped the fabricated token counter and the
`esc to interrupt` hint. A transcript modal is a different thing: it IS a conversation view — user
turns, assistant text, tool calls, diffs. That is exactly what `claude-message`, `claude-tool-call`
and `claude-thinking` were built to render, semantics and all. Install what is needed:

```
bunx shadcn@latest add @brainless/claude-message
bunx shadcn@latest add @brainless/claude-tool-call
```

Keep their semantics intact — `claude-tool-call` is a real `<details>` disclosure; do not flatten it
into a `<pre>`.

## Global Constraints

Same as previous phases. Plus these, because this phase puts conversation text on screen:

- **Nothing read here is ever persisted.** The transcript text goes from disk to the modal and
  nowhere else: not into `agent-deck.db`, not into any log, not into a zustand store that outlives
  the modal. The index keeps storing tool names and usage only.
- **Bounded reads.** A transcript can be 130 MB. Never read the whole file: seek to the end, read at
  most the last 256 KB, and parse forward from the first complete line. The modal shows the tail, not
  the history.
- Polling stops the moment the modal closes. No background polling of a closed viewer.
- UI text Indonesian, code and commit messages English. No `Co-Authored-By`. Never push.

## Command API (the contract)

```ts
// invoke("session_tail", { sessionId: string, cwd: string, agentId?: string | null })
//   -> TranscriptTail
interface TranscriptTail {
  entries: TailEntry[];   // oldest first
  fileSize: number;       // for the caller to notice growth
  found: boolean;         // false when no transcript file exists for that session
}
type TailEntry =
  | { kind: "user"; ms: number; text: string }
  | { kind: "assistant"; ms: number; text: string; model: string | null }
  | { kind: "thinking"; ms: number; text: string }
  | { kind: "tool"; ms: number; name: string; input: string; status: "running" | "ok" | "error" }
  | { kind: "result"; ms: number; toolName: string; preview: string; isError: boolean };
```

`agentId` selects a subagent transcript under `<sessionId>/subagents/agent-<id>.jsonl` instead of the
parent. Text fields are truncated: `text` to 4000 chars, `input` and `preview` to 600, each with a
trailing `…` when cut.

---

### Task 1: Bounded tail reader

**Files:** Create `src-tauri/crates/collector/src/tail.rs`; modify `src-tauri/crates/collector/src/lib.rs`

**Interfaces:**
- `const TAIL_BYTES: u64 = 256 * 1024;`
- `enum TailEntry { ... }` mirroring the Command API, serde tagged with `kind`, camelCase fields.
- `fn read_tail(path: &Path, max_entries: usize) -> anyhow::Result<(Vec<TailEntry>, u64)>`:
  - seek to `len.saturating_sub(TAIL_BYTES)`, read to the end, drop everything before the first `\n`
    (a partial line), parse each complete line
  - map records: `user` → text from `message.content` string or the first text block, skipping a
    record whose content is only `tool_result`; `assistant` → text blocks as `assistant`, thinking
    blocks as `thinking`, `tool_use` as `tool`; a `user` `tool_result` → `result`
  - keep only the last `max_entries`, oldest first
  - return the file length as the second value
- Truncation limits exactly as the contract states.

- [ ] **Step 1: Write the failing tests** (`tempfile` fixtures using the REAL record shapes, with ISO
  string timestamps — the numeric-fixture mistake from P12 must not repeat):
1. `reads_user_and_assistant_text`.
2. `maps_tool_use_and_tool_result`: a `tool_use` yields a `tool` entry and the matching `tool_result` yields a `result` with `isError` false; an `is_error: true` result sets it true.
3. `skips_a_user_record_that_is_only_a_tool_result_wrapper` — it is already represented as `result`.
4. `keeps_only_the_last_max_entries`: 50 records with `max_entries` 10 returns 10, oldest first.
5. `reads_only_the_tail_of_a_large_file`: write more than `TAIL_BYTES` of padding before 3 real records; all 3 come back and the call completes quickly.
6. `drops_a_partial_first_line`: a file whose tail window starts mid-record does not panic and does not emit a broken entry.
7. `truncates_long_text_with_an_ellipsis`: a 9000-char assistant text comes back at 4001 chars ending in `…`.
8. `missing_file_is_an_error_not_a_panic`.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 8 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): add a bounded transcript tail reader"`

---

### Task 2: The command

**Files:** Modify `src-tauri/src/lib.rs`

**Interfaces:**
- `session_tail(session_id, cwd, agent_id) -> Result<TranscriptTail, String>` resolves the path with
  the existing `transcript::find_transcript`, or the subagent path when `agent_id` is given, calls
  `read_tail` with `max_entries: 60`, and returns `found: false` with an empty list rather than an
  error when no file exists.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `cargo check --manifest-path src-tauri/Cargo.toml --workspace` exits 0 and the whole Rust suite passes.
- [ ] **Step 3: Commit** — `git commit -m "feat(app): expose the session tail command"`

---

### Task 3: The modal

**Files:** Create `src/components/TranscriptModal.tsx`; modify `src/components/SessionCard.tsx`, `src/lib/api.ts`, `package.json`

**Interfaces:**
- Install `@brainless/claude-message` and `@brainless/claude-tool-call`, and review what they render
  before wiring them, exactly as P14 did for `claude-thinking`. If either hardcodes a claim that is
  false here (an interrupt hint, an invented counter), add a prop and pass it off rather than
  displaying something untrue. Record what you changed in the commit message.
- The card becomes clickable: the project title row opens the modal for that session. Keep the
  existing buttons working — the click handler must not swallow them.
- `TranscriptModal` props `{ session: Session; agentId?: string | null; onClose: () => void }`:
  - fixed overlay, dark scrim, centered panel at most 900 px wide and 80 vh tall
  - header: agent icon, project, model, the live status pill, and a close button
  - body: the entries rendered with the brainless components, newest at the bottom, auto-scrolled to
    the bottom unless the user has scrolled up (then show a `Ke bawah` button rather than yanking
    the view)
  - polls `session_tail` every 1.5 s while open; stops on close and on unmount
  - when `session.subagents` is non-empty, a row of chips at the top switches the viewer between the
    parent and each running subagent
  - empty state: `Belum ada isi transcript untuk sesi ini.`
  - error state shows the message and keeps the modal open
- Escape closes it, focus is trapped inside while open, and focus returns to the card afterwards.
- A visible footer line: `Hanya menampilkan bagian akhir transcript. Tidak ada yang disimpan.`

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3:** Feel-check by running the app: open the modal on a busy session and confirm new
  lines appear without the view jumping while you are reading.
- [ ] **Step 4: Commit** — `git commit -m "feat(ui): add a live transcript modal"`

---

## Done when

- The whole Rust suite passes with 8 new tests.
- `bunx tsc --noEmit`, `bun test`, `bun run build`, `cargo check --workspace` all clean.
- One commit per task, no `Co-Authored-By`, nothing pushed.

## Self-Review

- **Coverage:** watch any running session, and any of its running subagents, live.
- **The size problem is the main risk** and is handled at the lowest level: a fixed 256 KB tail with
  a discarded partial first line. Task 1 test 5 pins it against a file larger than the window.
- **Privacy:** conversation text reaches the screen and nothing else. This is the first surface in
  the app that displays it, so the constraint is stated at the top of the plan and repeated in the
  modal footer.
- **brainless fits here properly**, unlike on the card: a transcript really is a conversation view.
  The P14 rule still applies — if a component asserts something untrue in this context, fix the
  component rather than show the lie.
