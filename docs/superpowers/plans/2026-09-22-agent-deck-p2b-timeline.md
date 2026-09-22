# Agent Deck P2b: Timeline

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A Timeline screen showing one lane per session with coloured spans for each tool call, and a detail panel for the selected span.

**Architecture:** The indexer gains a second table, `tool_span`, filled from the same Claude transcript pass (a `tool_use` starts a span, the matching `tool_result` ends it) and from opencode `part` rows. A `timeline_spans` command returns lanes for a time window. The frontend adds a Timeline page that lays lanes out proportionally.

**Tech Stack:** Same as P2.

**Spec:** `docs/superpowers/specs/2026-09-22-agent-deck-design.md`. Mockup: `docs/design/03-timeline.png`.

## Global Constraints

Same as the P2 plan. In particular: read-only on agent data, no conversation text persisted (the
span stores the tool NAME and a detail string already truncated to 80 chars by
`transcript::tool_detail`; that truncated detail is allowed because P1 already shows it live), no
`Co-Authored-By`, never push, Bun only, `@/` alias, Indonesian UI text.

## Command API (the contract)

```ts
// invoke("timeline_spans", { fromMs: number, toMs: number }) -> TimelineLane[]
interface TimelineLane {
  sessionId: string;
  agent: Agent;
  project: string;
  model: string | null;
  spans: TimelineSpan[];
}
interface TimelineSpan {
  id: string;
  tool: string;
  detail: string | null;
  startMs: number;
  endMs: number | null;   // null = still running / never completed
  status: "ok" | "error" | "running";
  tokens: TokenUsage | null;  // usage of the assistant message that opened the span, when known
}
```

Lanes are sorted by their earliest span. Spans within a lane are sorted by `startMs`. A lane with
no span inside the window is omitted.

---

### Task 1: `tool_span` table in the store

**Files:** Modify `src-tauri/crates/collector/src/store.rs`

**Interfaces:**
- `struct SpanRow { pub id: String, pub agent: Agent, pub session_id: String, pub project: String, pub model: Option<String>, pub tool: String, pub detail: Option<String>, pub start_ms: i64, pub end_ms: Option<i64>, pub status: String, pub tokens: Option<TokenUsage> }`
- `Store::upsert_spans(&mut self, rows: &[SpanRow]) -> anyhow::Result<usize>` (one transaction, `INSERT OR REPLACE`)
- `Store::spans(&self, from_ms: i64, to_ms: i64) -> anyhow::Result<Vec<SpanRow>>` returning every span that OVERLAPS the window, i.e. `start_ms < to_ms AND (end_ms IS NULL OR end_ms > from_ms)`, ordered by `session_id, start_ms`.

Schema to add in `open`:

```sql
CREATE TABLE IF NOT EXISTS tool_span (
  id TEXT PRIMARY KEY,
  agent TEXT NOT NULL,
  session_id TEXT NOT NULL,
  project TEXT NOT NULL,
  model TEXT,
  tool TEXT NOT NULL,
  detail TEXT,
  start_ms INTEGER NOT NULL,
  end_ms INTEGER,
  status TEXT NOT NULL,
  input INTEGER, output INTEGER, cache_read INTEGER, cache_write INTEGER, reasoning INTEGER
);
CREATE INDEX IF NOT EXISTS tool_span_start ON tool_span(start_ms);
```

- [ ] **Step 1: Write the failing tests:**
1. `spans_upsert_is_idempotent_by_id`: insert the same span twice, one row results.
2. `spans_returns_overlapping_only`: four spans — entirely before the window, entirely after, overlapping the start, and fully inside — assert only the last two come back.
3. `spans_with_null_end_are_treated_as_open`: a span with `end_ms: None` starting before the window is returned.
4. `spans_round_trip_tokens_and_status`: a span with `Some(TokenUsage)` and status `error` reads back identically; a span with `None` tokens reads back as `None`.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 4 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): add tool span table"`

---

### Task 2: Emit spans while indexing Claude

**Files:** Modify `src-tauri/crates/collector/src/indexer.rs`

**Interfaces:**
- `index_claude` now also produces spans. Within one file pass, keep a map from `tool_use.id` to an
  open span. Rules:
  - An `assistant` record's `tool_use` item opens a span: `id` is `claude:<tool_use.id>`, `tool` is
    the tool name, `detail` comes from the same helper the live tailer uses (first line, 80 chars),
    `start_ms` from the record's `timestamp`, `status` `running`, `tokens` from that record's usage.
  - A `user` record's `tool_result` with a matching `tool_use_id` closes it: `end_ms` from that
    record's timestamp, `status` `error` when the `tool_result` has `is_error: true`, otherwise `ok`.
  - A span never closed stays `running` with `end_ms: None`.
- `IndexReport` gains `pub spans_upserted: usize`.
- Because a `tool_result` can appear in a later indexing pass than its `tool_use`, the open-span map
  must be persisted across passes. Simplest correct approach: when closing a span, write the closing
  update even if the opening record was seen in an earlier run, by upserting a row that reads the
  existing row first (`Store::span_by_id(&self, id: &str) -> anyhow::Result<Option<SpanRow>>`, add
  it) and merging. If no opening row exists, ignore the orphan `tool_result`.

- [ ] **Step 1: Write the failing tests:**
1. `claude_span_opens_and_closes`: one `tool_use` then its `tool_result` in the same file yields one span with `status: "ok"` and the right `start_ms`/`end_ms`.
2. `claude_span_error_flag_sets_error_status`: a `tool_result` with `"is_error":true` yields `status: "error"`.
3. `claude_unclosed_span_stays_running`: a `tool_use` with no result yields `end_ms: None`, `status: "running"`.
4. `claude_span_closed_in_a_later_pass`: index the file with only the `tool_use`, then append the `tool_result` and index again; the span ends up closed with `status: "ok"`.
5. `claude_span_carries_detail_and_tokens`: a `Bash` tool_use with a long command has `detail` truncated to 80 chars and non-zero `tokens`.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 5 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): emit tool spans from claude transcripts"`

---

### Task 3: Emit spans from opencode

**Files:** Modify `src-tauri/crates/collector/src/indexer.rs`

**Interfaces:**
- `index_opencode` also reads `part` rows whose `json_extract(data,'$.type') = 'tool'` and upserts a
  span each: id `opencode:<part.id>`, `tool` from `$.tool`, `start_ms` from `$.state.time.start`
  (falling back to `part.time_created`), `end_ms` from `$.state.time.end` (null when absent),
  `status` mapped from `$.state.status` — `completed` → `ok`, `error` → `error`, anything else →
  `running`, `detail` left `None` (opencode tool input is not read, to avoid storing command text),
  `tokens` `None`.

- [ ] **Step 1: Write the failing tests:**
1. `opencode_spans_map_status`: three parts with statuses `completed`, `error` and `running` produce spans with `ok`, `error`, `running`.
2. `opencode_span_uses_state_times_then_falls_back`: a part with `state.time.start`/`end` uses them; one without uses `part.time_created` and a null end.
3. `opencode_spans_store_no_command_text`: a part whose `state.input.command` holds a command string results in a span whose `detail` is `None`.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 3 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): emit tool spans from opencode"`

---

### Task 4: `timeline_spans` command

**Files:** Modify `src-tauri/src/lib.rs`

**Interfaces:**
- Command `timeline_spans(from_ms: i64, to_ms: i64) -> Result<Vec<TimelineLane>, String>` grouping
  `Store::spans` rows into lanes by `session_id`, filling `agent`, `project` and `model` from the
  first span of that session (the most recent non-null model wins), sorting lanes by their earliest
  span and spans by `start_ms`.
- Define the wire structs in `src-tauri/src/lib.rs` (or a small `src-tauri/src/dto.rs`) with serde
  camelCase, exactly matching the Command API section.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `cargo check --manifest-path src-tauri/Cargo.toml --workspace` exits 0.
- [ ] **Step 3: Commit** — `git commit -m "feat(app): expose timeline spans command"`

---

### Task 5: Timeline page

**Files:**
- Create: `src/pages/TimelinePage.tsx`, `src/components/TimelineLane.tsx`, `src/components/SpanDetail.tsx`
- Modify: `src/lib/api.ts` (add `timelineSpans`), `src/lib/nav.ts` (enable `timeline`), `src/App.tsx`
- Create: `src/lib/timeline.ts`, test `src/lib/timeline.test.ts`

**Interfaces:**
- `src/lib/timeline.ts` exports:
  - `windowFor(rangeMinutes: number, nowMs: number): { fromMs: number; toMs: number }`
  - `layoutSpan(span: TimelineSpan, fromMs: number, toMs: number, nowMs: number): { leftPct: number; widthPct: number }` — clamps to `[0, 100]`, treats `endMs === null` as `nowMs`, and enforces a minimum width of `0.5` so a very short span stays visible.
  - `ticksFor(fromMs: number, toMs: number, count: number): { ms: number; label: string }[]` — evenly spaced, `HH:MM` labels in local time.
- `TimelinePage`: a range filter (`30 mnt`, `2 jam`, `Hari ini` → 30, 120, 1440 minutes), a tick axis,
  one `TimelineLane` per lane, and a right-hand `SpanDetail` panel for the selected span (empty state
  `Pilih satu blok untuk lihat detail.`).
- Span colours: `ok` uses `--color-busy`, `error` uses `--color-err`, `running` uses `--color-waiting`,
  each at low opacity with a stronger border, matching `docs/design/03-timeline.png`.
- Lane label column is 180 px: agent dot, project name, model underneath.

- [ ] **Step 1: Write the failing tests** in `src/lib/timeline.test.ts`:
1. `windowFor` returns a window whose width equals the requested minutes and whose `toMs` is `nowMs`.
2. `layoutSpan` places a span covering the middle half of the window at `leftPct` 25 and `widthPct` 50.
3. `layoutSpan` clamps a span that starts before the window to `leftPct` 0 and reduces its width accordingly.
4. `layoutSpan` treats a null `endMs` as `nowMs`.
5. `layoutSpan` gives a zero-length span at least `0.5` width.
6. `ticksFor` returns the requested count, ascending, first tick at `fromMs`.
- [ ] **Step 2: Run to verify it fails** — `bun test`.
- [ ] **Step 3: Implement the helpers.**
- [ ] **Step 4: Run to verify it passes.**
- [ ] **Step 5:** Build the page and components, enable the nav item.
- [ ] **Step 6:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 7: Commit** — `git commit -m "feat(ui): add timeline page"`

---

## Done when

- `cd src-tauri && cargo test -p collector --lib` passes with 12 new tests on top of P2's total.
- `bun test` passes with 6 new tests on top of P2's total.
- `bunx tsc --noEmit`, `bun run build`, `cargo check --workspace` all clean.
- One commit per task, no `Co-Authored-By`, nothing pushed.

## Self-Review

- **Spec coverage:** timeline lanes with tool spans and a detail panel (Tasks 1-5). The spec's
  "per-turn token detail" is covered by `TimelineSpan.tokens` for Claude; opencode spans carry no
  tokens because its `part` rows do not hold per-tool usage, which is stated in Task 3.
- **Type consistency:** `TimelineLane` / `TimelineSpan` are identical in the Command API section,
  Task 4's wire structs and Task 5's TypeScript.
- **Privacy:** opencode spans deliberately store no `detail`, so no command text from opencode
  reaches the index; Claude spans reuse the same 80-char truncated detail the live view already
  shows.
