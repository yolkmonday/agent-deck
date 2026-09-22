# Agent Deck P12: Heartbeat — detect stalled agents

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** An agent that is alive but no longer making progress is flagged, instead of looking "busy" forever.

**Architecture:** The collector already knows each session's last activity. P12 turns that into three measured signals — time since last output, how long the current tool has run, and whether a running agent process has no session at all — and derives a `stalled` flag from them. The Live board, the attention bar and the notification path from P8 all reuse it.

**Tech Stack:** Same as P11. No new dependency.

**Spec:** Extends `docs/superpowers/specs/2026-09-22-agent-deck-design.md` section 3.2.

## Why this phase exists

On 2026-09-22 two delegated `opencode run` processes stayed alive for over two hours without ever
creating a session: no output, no files touched, no network. Every check that looked at "is the
process alive" said yes. Nothing noticed. This phase makes the dashboard notice.

## The hard part: silence is not always a stall

A long `Bash` tool call produces no transcript records until it returns. A ten-minute test suite
would look identical to a hang if we only measured "time since last record". So P12 measures three
distinct things and never collapses them into one number:

| Signal | Meaning | Honest reading |
|---|---|---|
| `quietMs` | time since the session's last new record or DB row | high + no tool running = suspicious |
| `toolRunningMs` | how long the currently open tool call has been running | high = the tool may be hung, or may just be slow |
| `orphanProcess` | an agent process exists for a directory with no session at all | almost always a real failure |

## Global Constraints

Same as previous phases. Plus:

- **Never claim a session is stalled when it might be working.** A running tool is reported as
  `Tool berjalan lama`, not as stalled. Only silence with no open tool counts toward `stalled`.
- Thresholds are user-editable, and the UI states which threshold triggered.
- UI text Indonesian, code and commit messages English. No `Co-Authored-By`. Never push.

## Command API (the contract)

`Session` gains:

```ts
quietMs: number;            // now - last activity for this session
toolRunningMs: number | null;  // null when no tool is open
health: "ok" | "slow" | "stalled";
healthReason: string | null;   // Indonesian, e.g. "diam 7 mnt tanpa tool berjalan"
```

`LiveSnapshot` gains:

```ts
orphans: Orphan[];   // agent processes with no matching session
interface Orphan { agent: Agent; pid: number; cwd: string; ageMs: number }
```

`Settings` (from P8) gains:

```ts
stallMinutes: number;      // default 5   — quiet with no tool => stalled
slowToolMinutes: number;   // default 10  — one tool open this long => slow
```

Health rules, evaluated in this order:
1. `status === "waiting"` → `ok` (waiting for a human is not a stall).
2. A tool is open and `toolRunningMs >= slowToolMinutes` → `slow`, reason
   `tool "<name>" berjalan <n> mnt`.
3. A tool is open and below the threshold → `ok`.
4. No tool open, `status === "busy"`, and `quietMs >= stallMinutes` → `stalled`, reason
   `diam <n> mnt tanpa tool berjalan`.
5. Otherwise `ok`. An `idle` session is never stalled.

---

### Task 1: Health rules as pure Rust functions

**Files:** Create `src-tauri/crates/collector/src/health.rs`; modify `src-tauri/crates/collector/src/lib.rs`, `src-tauri/crates/collector/src/model.rs`

**Interfaces:**
- `enum Health { Ok, Slow, Stalled }` (serde lowercase) and
  `struct Thresholds { pub stall_ms: i64, pub slow_tool_ms: i64 }` with
  `Thresholds::defaults()` = 5 min / 10 min.
- `fn evaluate(status: Status, quiet_ms: i64, tool: Option<(&str, i64)>, t: &Thresholds) -> (Health, Option<String>)`
  implementing the five rules above. The tuple is `(tool_name, tool_running_ms)`.
- Reason strings are built with whole minutes, rounded down.
- `Session` gains `quiet_ms: i64`, `tool_running_ms: Option<i64>`, `health: Health`,
  `health_reason: Option<String>` (serde camelCase).
- `LiveSnapshot` gains `orphans: Vec<Orphan>` and `struct Orphan { agent: Agent, pid: u32, cwd: String, age_ms: i64 }`.
  `same_content` must compare orphans and every session's health.

- [ ] **Step 1: Write the failing tests** in `health.rs`:
1. `waiting_is_never_stalled`: status waiting, quiet 60 min, no tool → `Ok`.
2. `idle_is_never_stalled`: status idle, quiet 60 min → `Ok`.
3. `busy_and_quiet_past_threshold_is_stalled`: busy, quiet 6 min, no tool, default thresholds → `Stalled`, reason contains `6 mnt` and `tanpa tool`.
4. `busy_and_quiet_below_threshold_is_ok`: busy, quiet 4 min, no tool → `Ok`, reason `None`.
5. `open_tool_below_threshold_is_ok_even_when_quiet`: busy, quiet 30 min, tool running 2 min → `Ok`. This is the "long silence but a tool is working" case and must NOT be stalled.
6. `open_tool_past_threshold_is_slow_not_stalled`: tool running 11 min → `Slow`, reason contains the tool name and `11 mnt`.
7. `custom_thresholds_are_respected`: with `stall_ms` of 60 s, busy and quiet 90 s → `Stalled`.
8. `reason_rounds_minutes_down`: quiet 6 min 59 s reports `6 mnt`.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 8 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): add session health rules"`

---

### Task 2: Measure the signals

**Files:** Modify `src-tauri/crates/collector/src/live.rs`, `src-tauri/crates/collector/src/transcript.rs`, `src-tauri/crates/collector/src/opencode.rs`

**Interfaces:**
- `TranscriptState` gains `pub last_record_ms: Option<i64>` (the `timestamp` of the most recent
  record it parsed) and `pub tool_started_ms: Option<i64>` (the timestamp of the record that opened
  the still-pending tool; cleared when the tool closes).
- `OpencodeLive` gains `pub tool_started_ms: Option<i64>` read from the running part's
  `state.time.start`, falling back to `part.time_created`.
- `LiveCollector::snapshot` computes, per session:
  - `quiet_ms = now_ms - max(last activity)`, where "last activity" is the transcript's
    `last_record_ms` for Claude (falling back to the session file's `updated_at_ms`), or
    `time_updated` for opencode.
  - `tool_running_ms = now_ms - tool_started_ms` when a tool is open, else `None`.
  - then calls `health::evaluate`.
- `snapshot` also collects **orphans**: for each running agent process (from `ProcessTable`) whose
  directory matches no session in this snapshot, emit an `Orphan` with the process age. Add
  `ProcessTable::start_time_ms(&self, pid: u32) -> Option<i64>` (from `ps -o lstart=` or
  `-o etime=`; the fake returns a stored value) so the age is real rather than guessed.
- `snapshot`'s signature gains `thresholds: &Thresholds`.

- [ ] **Step 1: Write the failing tests** in `live.rs`, extending the existing fixtures:
1. `quiet_ms_comes_from_the_last_transcript_record`: a transcript whose last record is at T, snapshot at T+120000, gives `quiet_ms == 120000`.
2. `quiet_ms_falls_back_to_the_session_file`: a session with no transcript file uses `updated_at_ms`.
3. `tool_running_ms_is_set_while_a_tool_is_open_and_cleared_after`: open a tool, assert `Some`; append the `tool_result`, assert `None`.
4. `a_long_quiet_busy_session_is_reported_stalled`: build one and assert `health == Stalled` and a non-empty reason.
5. `an_orphan_opencode_process_is_reported`: a fake process table lists an `opencode` process in `/x/y` with no session row; the snapshot's `orphans` has one entry with that pid and cwd.
6. `no_orphan_when_the_directory_has_a_session`: the same process with a matching session yields no orphan.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.** Update every existing `snapshot(...)` call site.
- [ ] **Step 4: Run to verify it passes** — 6 new tests, and every pre-existing collector test still green.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): measure quiet time, tool duration and orphan processes"`

---

### Task 3: Thresholds in settings and the live loop

**Files:** Modify `src-tauri/src/lib.rs`

**Interfaces:**
- `Settings` gains `stall_minutes: i64` (default 5) and `slow_tool_minutes: i64` (default 10),
  persisted in the existing `setting` table. Values below 1 are rejected with
  `menit harus minimal 1`.
- The live loop and `live_snapshot` read the settings, build `Thresholds`, and pass them to
  `snapshot`. Do not hold the settings lock while emitting or sleeping.

- [ ] **Step 1: Write the failing tests** in `lib.rs`: the two new settings default correctly, round-trip, and reject `0`.
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes**, and `cargo check --workspace` exits 0.
- [ ] **Step 5: Commit** — `git commit -m "feat(app): make heartbeat thresholds configurable"`

---

### Task 4: Show it

**Files:** Modify `src/lib/types.ts`, `src/lib/attention.ts`, `src/components/SessionCard.tsx`, `src/components/AttentionBar.tsx`, `src/components/KpiRow.tsx`, `src/components/SettingsPopover.tsx`, `src/pages/LivePage.tsx`; test `src/lib/attention.test.ts`

**Interfaces:**
- Types mirror the Command API exactly.
- `attention.ts` gains:
  - `unhealthySessions(sessions: Session[]): Session[]` — `health !== "ok"`, stalled before slow, then longest `quietMs` first.
  - `healthLabel(session: Session): string` — `"<project> macet · <reason>"` for stalled,
    `"<project> lambat · <reason>"` for slow.
- `SessionCard`: a `stalled` session gets the `err` colour treatment (border and status pill) with
  the pill text `Macet`; a `slow` one gets `waiting` colours and `Lambat`. The reason renders under
  the activity line. An `ok` session is unchanged.
- `KpiRow`: the first KPI's subtitle gains `· N macet` in the `err` colour when any session is
  stalled.
- `AttentionBar`: shows waiting sessions first (existing behaviour), then stalled ones, using
  `healthLabel`. Orphans render as their own line:
  `opencode jalan di <cwd> tapi tidak ada sesi · <n> mnt` with no jump button, because there is
  nothing to jump to.
- `SettingsPopover`: two number inputs, `Anggap macet setelah` and `Anggap tool lambat setelah`,
  both in minutes, saved through `settingsSet`, with the note
  `Tool yang sedang jalan tidak dihitung macet.`
- Notifications: reuse the P8 path so a session becoming `stalled` notifies under the same mode
  rules as `waiting`. Do not notify twice for the same session while it stays stalled.

- [ ] **Step 1: Write the failing tests** in `src/lib/attention.test.ts`:
1. `unhealthy_sessions_puts_stalled_before_slow`.
2. `unhealthy_sessions_sorts_by_quiet_time_within_a_group`.
3. `unhealthy_sessions_excludes_ok_sessions`.
4. `health_label_formats_stalled_and_slow` with exact strings.
- [ ] **Step 2: Run to verify it fails** — `bun test`.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes**, and `bunx tsc --noEmit` and `bun run build` are clean.
- [ ] **Step 5: Commit** — `git commit -m "feat(ui): surface stalled and slow agents"`

---

## Done when

- The whole Rust suite passes with 16 new tests; `bun test` passes with 4 new tests.
- `bunx tsc --noEmit`, `bun run build`, `cargo check --workspace` all clean.
- One commit per task, no `Co-Authored-By`, nothing pushed.

## Self-Review

- **Coverage:** the three failure shapes seen in practice — a process alive but never starting
  (orphan), an agent that went quiet mid-task (stalled), and a tool that never returns (slow).
- **The honesty constraint is the design:** test 5 in Task 1 exists specifically so a long-running
  tool is never mislabelled as a hang. A dashboard that cries wolf during every long test run would
  be worse than no heartbeat at all.
- **Deliberate gap:** P12 only reports. It does not kill or restart anything. Automatic recovery
  needs its own decision about what is safe to kill, and that is a separate phase.
