# Agent Deck P3: Hemat Token (rtk + lean-ctx)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A Hemat Token screen showing how many tokens rtk and lean-ctx saved, per day and per command.

**Architecture:** A `savings` collector module reads rtk's SQLite history and lean-ctx's `stats.json` directly (no CLI shell-out, so the app does not depend on those binaries being on PATH). One Tauri command returns the combined view. One frontend page renders it.

**Tech Stack:** Same as P2.

**Spec:** `docs/superpowers/specs/2026-09-22-agent-deck-design.md` (section 3.1, "Savings"). Mockup: `docs/design/04-hemat-token.png`.

## Global Constraints

Same as the P2 plan: read-only on other tools' data, never open secrets, no conversation text
persisted, Bun only, `@/` alias, Indonesian UI text, English code, commits lowercase with no
`Co-Authored-By`, never push.

Extra for this phase: rtk's `commands` table stores `original_cmd` and `rtk_cmd`, which are full
command lines. **Never store or return the full command line.** Only the normalised first token or
two (for example `git status`, `bun test`) may leave the collector, and nothing is written to the
dashboard index.

## Data sources (verified on this machine)

- rtk: `~/Library/Application Support/rtk/history.db`, table `commands` with columns
  `timestamp, original_cmd, rtk_cmd, input_tokens, output_tokens, saved_tokens, savings_pct,
  exec_time_ms, project_path`. About 2200 rows. `timestamp` format must be verified in Task 1,
  Step 1 before writing the query.
- lean-ctx: `~/.lean-ctx/stats.json` with `total_commands`, `total_input_tokens`,
  `total_output_tokens`, `first_use`, `last_use`, `commands{}`, `daily[]` (about 90 entries).
  The exact key names inside `daily[]` must be verified in Task 2, Step 1.

## Command API (the contract)

```ts
// invoke("savings_summary", { days: number }) -> SavingsSummary
interface SavingsSummary {
  rtk: SavingsSource;
  leanCtx: SavingsSource;
  daily: SavingsDay[];        // ascending by date, one entry per date with any data
  topCommands: SavingsCommand[];  // rtk only, best saving first, at most 10
  warnings: string[];         // e.g. "rtk: database not found"
}
interface SavingsSource { available: boolean; savedTokens: number; totalTokens: number; savingsPct: number; entries: number }
interface SavingsDay { date: string; rtkSaved: number; leanCtxSaved: number }
interface SavingsCommand { command: string; savedTokens: number; savingsPct: number; runs: number }
```

`savingsPct` is `saved / total * 100`, or 0 when `total` is 0. `date` is `YYYY-MM-DD` local.
A missing source returns `available: false` with zeros and adds a warning; it is never an error.

---

### Task 1: rtk reader

**Files:**
- Create: `src-tauri/crates/collector/src/savings.rs`
- Modify: `src-tauri/crates/collector/src/lib.rs` (add `pub mod savings;`)

**Interfaces:**
- `struct RtkStats { pub saved_tokens: i64, pub total_tokens: i64, pub entries: i64, pub daily: Vec<(String, i64)>, pub top_commands: Vec<(String, i64, f64, i64)> }`
  where `top_commands` is `(command, saved_tokens, savings_pct, runs)`.
- `fn read_rtk(db: &Path, since_ms: i64) -> anyhow::Result<RtkStats>`
- `fn normalise_command(raw: &str) -> String` — the only function allowed to touch `original_cmd`.
  Rules: trim; drop a leading `rtk `; take the first token; if that token is one of
  `git`, `cargo`, `bun`, `npm`, `go`, `docker`, `kubectl`, `pnpm`, `yarn`, also take the second
  token when it is a bare word (matches `^[a-z][a-z0-9-]*$`); lowercase the result; return
  `"(lainnya)"` when the input is empty.
- `read_rtk` opens the DB read-only, groups by the normalised command, and filters by
  `since_ms` (pass 0 for all time). `total_tokens` is `sum(input_tokens)`, i.e. what the output
  would have cost before compression; `saved_tokens` is `sum(saved_tokens)`.

- [ ] **Step 1: Verify the real schema and timestamp format first.** Run
  `sqlite3 -readonly "$HOME/Library/Application Support/rtk/history.db" ".schema commands" "select typeof(timestamp), timestamp from commands limit 3"`.
  Record whether `timestamp` is epoch seconds, epoch millis or a text datetime, and write the
  `since_ms` filter accordingly. State the finding in the commit message. Do NOT print any
  `original_cmd` value into the conversation.
- [ ] **Step 2: Write the failing tests** against a `tempfile` SQLite fixture with the real schema:
1. `normalise_command_keeps_subcommand_for_known_tools`: `"git status --short"` → `"git status"`, `"cargo test -p x"` → `"cargo test"`.
2. `normalise_command_takes_first_token_otherwise`: `"ls -la /tmp"` → `"ls"`, `"  "` → `"(lainnya)"`.
3. `normalise_command_strips_rtk_prefix`: `"rtk git diff"` → `"git diff"`.
4. `normalise_command_ignores_flag_as_subcommand`: `"git --version"` → `"git"`.
5. `read_rtk_aggregates_by_normalised_command`: three rows, two of them `git status` variants, yield a `git status` entry with `runs == 2` and summed savings.
6. `read_rtk_respects_since_ms`: an old row is excluded.
7. `read_rtk_daily_groups_by_local_date`: two rows on one date and one on another yield 2 daily entries.
8. `read_rtk_missing_db_is_an_error_not_a_panic`: a nonexistent path returns `Err`, no panic.
- [ ] **Step 3: Run to verify it fails.**
- [ ] **Step 4: Implement.**
- [ ] **Step 5: Run to verify it passes** — 8 tests pass.
- [ ] **Step 6: Commit** — `git commit -m "feat(collector): read rtk savings history"`

---

### Task 2: lean-ctx reader

**Files:** Modify `src-tauri/crates/collector/src/savings.rs`

**Interfaces:**
- `struct LeanCtxStats { pub saved_tokens: i64, pub total_tokens: i64, pub entries: i64, pub daily: Vec<(String, i64)> }`
- `fn read_lean_ctx(stats_json: &Path, since_ms: i64) -> anyhow::Result<LeanCtxStats>`
- Parses defensively with `serde_json::Value`, not a rigid struct, because the file is written by
  another tool: a missing or renamed key contributes 0 rather than failing the whole read.

- [ ] **Step 1: Verify the real shape first.** Run
  `python3 -c "import json;d=json.load(open('/Users/yolk/.lean-ctx/stats.json'));print(list(d.keys()));print(json.dumps(d['daily'][-1] if d.get('daily') else {}, indent=0))"`
  and use the real key names for the saved-token and date fields. State them in the commit message.
- [ ] **Step 2: Write the failing tests** against temp JSON files:
1. `lean_ctx_reads_totals_and_daily`: a file with the real key names yields the expected totals and 2 daily entries.
2. `lean_ctx_respects_since_ms`: an older daily entry is excluded.
3. `lean_ctx_missing_keys_default_to_zero`: `{}` yields zeros without an error.
4. `lean_ctx_invalid_json_is_an_error`: `"not json"` returns `Err`.
- [ ] **Step 3: Run to verify it fails.**
- [ ] **Step 4: Implement.**
- [ ] **Step 5: Run to verify it passes** — 4 tests pass.
- [ ] **Step 6: Commit** — `git commit -m "feat(collector): read lean-ctx savings stats"`

---

### Task 3: `savings_summary` command

**Files:** Modify `src-tauri/src/lib.rs`

**Interfaces:**
- Command `savings_summary(days: i64) -> Result<SavingsSummary, String>` matching the Command API
  exactly. Paths: rtk at `<home>/Library/Application Support/rtk/history.db`, lean-ctx at
  `<home>/.lean-ctx/stats.json`.
- A failing source sets `available: false`, zeros, and pushes a warning like
  `rtk: <error>`; the command itself still returns `Ok`.
- `daily` merges both sources by date, filling 0 for a source with no entry that day.
- Results are cached in memory for 30 seconds so switching pages does not re-read the files.

- [ ] **Step 1:** Implement.
- [ ] **Step 2:** Verify — `cargo check --manifest-path src-tauri/Cargo.toml --workspace` exits 0.
- [ ] **Step 3: Commit** — `git commit -m "feat(app): expose savings summary command"`

---

### Task 4: Hemat Token page

**Files:**
- Create: `src/pages/SavingsPage.tsx`, `src/components/SavingsChart.tsx`
- Modify: `src/lib/api.ts` (add `savingsSummary` and the types), `src/lib/nav.ts` (enable `savings`), `src/App.tsx`

**Interfaces:**
- Four KPIs: `Total dihemat` (rtk + lean-ctx saved), `rtk`, `lean-ctx`, `Command teratas`
  (the first `topCommands` entry, or `-` when empty). Sub-lines give the percentage and the entry
  count, matching `docs/design/04-hemat-token.png`.
- `SavingsChart` is a stacked recharts `BarChart` of `daily` with `rtkSaved` in `--color-busy` and
  `leanCtxSaved` in `--color-ok`.
- A right-hand list shows `topCommands` with the command, saved tokens and percentage.
- Any `warnings` from the command render as a dim line above the chart, for example
  `rtk: database not found` shown as `rtk: tidak ditemukan`. Map known error text to Indonesian;
  fall back to showing the raw warning.
- Must render loading, error and empty states.

- [ ] **Step 1:** Implement the page and chart, enable the nav item.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): add savings page"`

---

## Done when

- `cd src-tauri && cargo test -p collector --lib` passes with 12 new tests on top of the previous total.
- `bunx tsc --noEmit`, `bun test`, `bun run build`, `cargo check --workspace` all clean.
- One commit per task, no `Co-Authored-By`, nothing pushed.

## Self-Review

- **Spec coverage:** rtk and lean-ctx savings per day and per command (Tasks 1-4). The spec also
  mentions `~/.lean-ctx/events.jsonl` and `cost_attribution.json`; this plan deliberately uses only
  `stats.json`, because it already carries the daily totals the screen needs and avoids tailing a
  second event stream. Noted as a gap, not an oversight.
- **Privacy:** `normalise_command` is the single choke point for rtk command text, and Task 1's test
  2 and 4 pin its behaviour. No raw command line can reach the UI or the index.
- **Type consistency:** `SavingsSummary` and its members are identical in the Command API section,
  Task 3's wire structs and Task 4's TypeScript.
