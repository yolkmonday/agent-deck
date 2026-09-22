# Agent Deck P2: History Index, Pricing and Token & Biaya Screen

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A durable history index of every agent message, a user-editable pricing table, and a Token & Biaya screen with per-day stacked bars and a per-model table.

**Architecture:** A new `store` module owns a dashboard SQLite index (`agent-deck.db` in the Tauri app-data dir). An `indexer` module fills it incrementally from Claude transcripts, the opencode DB and Codex sessions, caching per-file progress by `(mtime, size, offset)`. A `pricing` module converts `TokenUsage` to USD. Tauri commands expose aggregate queries. The frontend gains a router, a nav, and the Token & Biaya page.

**Tech Stack:** Same as P1, plus `recharts` and `@tanstack/react-query` on the frontend, and `dirs` on the Rust side.

**Spec:** `docs/superpowers/specs/2026-09-22-agent-deck-design.md` (sections 3.1, 3.2). Mockup: `docs/design/02-token-biaya.png`.

## Global Constraints

Everything in the P1 plan's Global Constraints still applies, in particular:

- All agent data is opened read-only. Never open `auth.json`, `secrets.env`, `~/.claude/sessions/*.key`, or any file under `~/.config/opencode/`.
- Skip any path containing `/.claude-mem/observer-sessions`.
- Dedupe Claude usage by `message.id`; the index primary key enforces this.
- Never persist conversation text, prompts, or full command lines. The index stores tool NAMES only.
- Bun only. TypeScript only. `@/` alias. UI text Indonesian, code English.
- Commit messages: English, concise, lowercase. NEVER a `Co-Authored-By` line or any trailer. Never push.
- The index is the dashboard's own file and is the only thing P2 writes.

## Parallel Tracks

- **Track A (Rust)** — Tasks 1-5. Touches only `src-tauri/`.
- **Track B (Frontend)** — Tasks 6-9. Touches only `src/` and `package.json`.

They share no files. The contract between them is the **Command API** below; both tracks must
match it exactly.

## Command API (the contract)

Rust commands and their TypeScript shapes. camelCase on the wire.

```ts
// invoke("index_status") -> IndexStatus
interface IndexStatus { indexedFiles: number; messages: number; running: boolean; lastRunMs: number | null }

// invoke("reindex") -> IndexStatus    (runs a full incremental pass, then returns status)

// invoke("history_daily", { days: number }) -> DailyRow[]
interface DailyRow { date: string; agent: Agent; tokens: TokenUsage; costUsd: number }
// `date` is "YYYY-MM-DD" in LOCAL time. One row per (date, agent) that has data.

// invoke("history_by_model", { days: number }) -> ModelRow[]
interface ModelRow { model: string; agent: Agent; tokens: TokenUsage; costUsd: number; messages: number }

// invoke("history_by_project", { days: number }) -> ProjectRow[]
interface ProjectRow { project: string; tokens: TokenUsage; costUsd: number; messages: number }

// invoke("history_totals", { days: number }) -> Totals
interface Totals { tokens: TokenUsage; costUsd: number; messages: number; cacheHitPct: number }
// cacheHitPct = cacheRead / (cacheRead + input) * 100, 0 when the denominator is 0.

// invoke("pricing_get") -> PriceEntry[]
// invoke("pricing_set", { entries: PriceEntry[] }) -> PriceEntry[]
interface PriceEntry { model: string; inputPerM: number; outputPerM: number; cacheReadPerM: number; cacheWritePerM: number }
```

`Agent` and `TokenUsage` are the P1 types already in `src/lib/types.ts`.

---

## Track A — Rust

### Task 1: Pricing module

**Files:**
- Create: `src-tauri/crates/collector/src/pricing.rs`
- Modify: `src-tauri/crates/collector/src/lib.rs` (add `pub mod pricing;`)

**Interfaces:**
- Produces:
  - `struct PriceEntry { pub model: String, pub input_per_m: f64, pub output_per_m: f64, pub cache_read_per_m: f64, pub cache_write_per_m: f64 }` (serde camelCase)
  - `struct PriceTable { entries: Vec<PriceEntry> }` with:
    - `fn defaults() -> PriceTable`
    - `fn load(path: &Path) -> PriceTable` (falls back to `defaults()` on missing or invalid file, merging any known defaults the file omits)
    - `fn save(&self, path: &Path) -> anyhow::Result<()>`
    - `fn entries(&self) -> &[PriceEntry]`
    - `fn set(&mut self, entries: Vec<PriceEntry>)`
    - `fn cost_usd(&self, model: &str, u: &TokenUsage) -> f64`
- Matching rule for `cost_usd`: exact model match first; then the longest entry whose `model` is a
  prefix of the requested model (so `claude-sonnet-5` matches an entry `claude-sonnet`); then 0.0.
  A provider prefix is stripped before matching, so `kn/deepseek-v4-1-flash` also tries
  `deepseek-v4-1-flash`.
- Cost formula: `(input * input_per_m + output * output_per_m + cache_read * cache_read_per_m + cache_write * cache_write_per_m) / 1_000_000`. `reasoning` is NOT charged separately.

**Defaults** (USD per million tokens; these are placeholders the user will confirm — put a comment
saying so):

| model | input | output | cacheRead | cacheWrite |
|---|---|---|---|---|
| `claude-opus-5` | 15.0 | 75.0 | 1.5 | 18.75 |
| `claude-sonnet-5` | 3.0 | 15.0 | 0.3 | 3.75 |
| `claude-haiku-4-5` | 1.0 | 5.0 | 0.1 | 1.25 |
| `claude-fable-5` | 3.0 | 15.0 | 0.3 | 3.75 |
| `gpt-5.4` | 2.5 | 10.0 | 0.25 | 3.125 |
| `deepseek-v4-1-flash` | 0.27 | 1.1 | 0.027 | 0.34 |
| `deepseek-v4-pro` | 0.55 | 2.2 | 0.055 | 0.69 |

- [ ] **Step 1: Write the failing tests**

Cover, with `#[test]` each:
1. `defaults_price_known_models`: `cost_usd("claude-sonnet-5", &TokenUsage{input:1_000_000, ..default})` equals `3.0` (use `(x - 3.0).abs() < 1e-9`).
2. `cost_sums_all_four_buckets`: for `claude-sonnet-5` with `input:1_000_000, output:1_000_000, cache_read:1_000_000, cache_write:1_000_000, reasoning:9_999_999` the cost is `3.0 + 15.0 + 0.3 + 3.75 = 22.05`, proving reasoning is not charged.
3. `provider_prefix_is_stripped`: `cost_usd("kn/deepseek-v4-1-flash", ...)` equals the cost for `deepseek-v4-1-flash`.
4. `prefix_match_picks_longest_entry`: with a table containing `claude` and `claude-sonnet`, the model `claude-sonnet-5` uses the `claude-sonnet` entry.
5. `unknown_model_costs_zero`: `cost_usd("totally-unknown", ...)` is `0.0`.
6. `save_then_load_round_trips`: save to a temp path, load it back, and a custom entry survives.
7. `load_missing_file_returns_defaults`: loading a nonexistent path yields the default entry count.
8. `load_invalid_json_returns_defaults`: write `"not json"` then load; defaults come back.

- [ ] **Step 2: Run to verify it fails** — `cd src-tauri && cargo test -p collector --lib pricing` → FAIL.
- [ ] **Step 3: Implement** the module as specified.
- [ ] **Step 4: Run to verify it passes** — 8 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): add pricing table"`

---

### Task 2: Index store

**Files:**
- Create: `src-tauri/crates/collector/src/store.rs`
- Modify: `src-tauri/crates/collector/src/lib.rs` (add `pub mod store;`)

**Interfaces:**
- Produces `struct Store` with:
  - `fn open(path: &Path) -> anyhow::Result<Store>` (creates parent dirs, applies the schema, sets `PRAGMA journal_mode=WAL`)
  - `fn upsert_messages(&mut self, rows: &[MessageRow]) -> anyhow::Result<usize>` (single transaction, `INSERT OR REPLACE`)
  - `fn file_progress(&self, path: &str) -> Option<FileProgress>`
  - `fn set_file_progress(&mut self, p: &FileProgress) -> anyhow::Result<()>`
  - `fn counts(&self) -> anyhow::Result<(i64, i64)>` returning `(indexed_files, messages)`
  - `fn daily(&self, since_ms: i64) -> anyhow::Result<Vec<DailyAgg>>`
  - `fn by_model(&self, since_ms: i64) -> anyhow::Result<Vec<ModelAgg>>`
  - `fn by_project(&self, since_ms: i64) -> anyhow::Result<Vec<ProjectAgg>>`
  - `fn totals(&self, since_ms: i64) -> anyhow::Result<TotalsAgg>`
- `struct MessageRow { pub id: String, pub agent: Agent, pub session_id: String, pub project: String, pub model: String, pub ts_ms: i64, pub tokens: TokenUsage }`
- `struct FileProgress { pub path: String, pub mtime_ms: i64, pub size: i64, pub offset: i64 }`
- `struct DailyAgg { pub date: String, pub agent: Agent, pub tokens: TokenUsage }`
- `struct ModelAgg { pub model: String, pub agent: Agent, pub tokens: TokenUsage, pub messages: i64 }`
- `struct ProjectAgg { pub project: String, pub tokens: TokenUsage, pub messages: i64 }`
- `struct TotalsAgg { pub tokens: TokenUsage, pub messages: i64 }`

**Schema** (use exactly this):

```sql
CREATE TABLE IF NOT EXISTS message (
  id TEXT PRIMARY KEY,
  agent TEXT NOT NULL,
  session_id TEXT NOT NULL,
  project TEXT NOT NULL,
  model TEXT NOT NULL,
  ts_ms INTEGER NOT NULL,
  input INTEGER NOT NULL,
  output INTEGER NOT NULL,
  cache_read INTEGER NOT NULL,
  cache_write INTEGER NOT NULL,
  reasoning INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS message_ts ON message(ts_ms);
CREATE TABLE IF NOT EXISTS indexed_file (
  path TEXT PRIMARY KEY,
  mtime_ms INTEGER NOT NULL,
  size INTEGER NOT NULL,
  offset INTEGER NOT NULL
);
```

The `message.id` must be globally unique across agents: the indexer prefixes it (`claude:<message.id>`,
`opencode:<message id>`, `codex:<session>:<ordinal>`). `daily` groups by local date using
`strftime('%Y-%m-%d', ts_ms/1000, 'unixepoch', 'localtime')`.

- [ ] **Step 1: Write the failing tests** — each `#[test]`, all against a `tempfile` path:
1. `open_creates_schema_and_is_idempotent`: open twice on the same path, no error, counts are `(0,0)`.
2. `upsert_is_idempotent_by_id`: insert the same row twice, `counts().1 == 1`, and totals are not doubled.
3. `upsert_replaces_usage_for_same_id`: insert id `x` with output 10, then id `x` with output 50; totals show 50.
4. `daily_groups_by_local_date_and_agent`: three rows, two on one local date (one Claude, one opencode) and one on another; assert 3 `DailyAgg` rows with the right sums.
5. `by_model_and_by_project_aggregate_and_count`: assert token sums and `messages` counts.
6. `totals_respects_since_ms`: a row older than `since_ms` is excluded.
7. `file_progress_round_trips_and_updates`: set, read back, set again with a bigger offset, read the new value.

- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 7 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): add history index store"`

---

### Task 3: Claude and Codex indexers

**Files:**
- Create: `src-tauri/crates/collector/src/indexer.rs`
- Modify: `src-tauri/crates/collector/src/lib.rs` (add `pub mod indexer;`)

**Interfaces:**
- Produces:
  - `struct IndexReport { pub files_scanned: usize, pub files_skipped: usize, pub messages_upserted: usize, pub errors: Vec<String> }`
  - `fn index_claude(store: &mut Store, projects_dir: &Path, home: &str) -> IndexReport`
  - `fn index_codex(store: &mut Store, sessions_dir: &Path, home: &str) -> IndexReport`
- `index_claude` walks `projects_dir/*/` (one level, plus each session's `subagents/` folder),
  skips any path containing `/.claude-mem/observer-sessions`, and for each `*.jsonl`:
  - looks up `file_progress`; if `mtime_ms` and `size` are unchanged, skip it (count as skipped)
  - otherwise reads from the stored `offset` to the last complete line, parses `assistant` records,
    and upserts one `MessageRow` per `message.id` with `agent: Claude`, `project` from
    `project_name(cwd, home)`, `model` from `message.model` (skip the record when the model is
    missing or `<synthetic>`), and `ts_ms` parsed from the ISO `timestamp`
  - then stores the new progress. If the file shrank (`size < stored size`), restart from offset 0.
- `index_codex` walks `sessions_dir/**/rollout-*.jsonl` and upserts one row per `event_msg` whose
  `payload.type` is `token_count`, using `info.last_token_usage` (fields `input_tokens`,
  `cached_input_tokens`, `cache_write_input_tokens`, `output_tokens`, `reasoning_output_tokens`),
  the most recent `turn_context.model` and `session_meta.cwd` seen in that file, `agent: Codex`, and
  id `codex:<file stem>:<ordinal>`.
- Add a helper `pub fn parse_iso_ms(s: &str) -> Option<i64>` (RFC 3339 to epoch ms). Implement it by
  hand with `chrono` NOT allowed — instead add `time = { version = "0.3", features = ["parsing", "macros"] }`
  to the collector's dependencies and use `time::OffsetDateTime::parse` with
  `time::format_description::well_known::Rfc3339`.

- [ ] **Step 1: Write the failing tests** — build fixture directories with `tempfile`:
1. `parse_iso_ms_handles_utc_and_offset`: `"2026-09-21T09:42:37.055Z"` → `1789033357055`; an equivalent `+07:00` timestamp gives the same value.
2. `claude_indexes_assistant_usage_once_per_message_id`: a transcript with the same `message.id` written twice yields one row.
3. `claude_skips_observer_sessions`: a project dir named `-Users-yolk--claude-mem-observer-sessions` contributes nothing.
4. `claude_skips_unchanged_files_on_second_run`: run twice; the second report has `files_scanned == 0` and `files_skipped >= 1`.
5. `claude_resumes_from_offset_when_file_grows`: run, append one more assistant record, run again; the second run upserts exactly 1 message.
6. `claude_restarts_when_file_shrinks`: run, truncate the file to a shorter different content, run again; totals reflect only the new content.
7. `claude_indexes_subagent_files`: a `<sessionId>/subagents/agent-x.jsonl` record is indexed too.
8. `claude_skips_records_without_model`: a record with no `message.model` is not indexed.
9. `codex_indexes_token_count_events`: a rollout file with `session_meta`, `turn_context` and two `token_count` events yields 2 rows with the right model, project and tokens.

- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.** Reuse `transcript::encode_cwd` / `live::project_name` where useful; do not duplicate logic.
- [ ] **Step 4: Run to verify it passes** — 9 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): index claude and codex history"`

---

### Task 4: opencode indexer

**Files:**
- Modify: `src-tauri/crates/collector/src/indexer.rs`

**Interfaces:**
- Produces `fn index_opencode(store: &mut Store, db: &Path, home: &str) -> IndexReport`
- Opens the opencode DB read-only, reads `message` joined to `session` for `session.directory`,
  and upserts one row per assistant message: id `opencode:<message.id>`, `agent: Opencode`,
  `model` as `<providerID>/<modelID>` from `data`, `project` from `project_name(session.directory, home)`,
  `ts_ms` from `message.time_created`, tokens from `data.tokens{input,output,reasoning,cache{read,write}}`.
- Skips rows whose `data.role` is not `assistant`. Uses `file_progress` with the DB path as key and
  the last seen `time_created` stored in `offset`, so later runs only read newer messages.

- [ ] **Step 1: Write the failing tests**, using the same fixture builder style as the P1 `opencode.rs` tests:
1. `opencode_indexes_assistant_messages_only`: two messages, one `user` and one `assistant`; one row results, with the right tokens and `kn/deepseek-v4-1-flash` model.
2. `opencode_second_run_only_reads_newer_messages`: run, insert a newer message, run again; the second report upserts exactly 1.
3. `opencode_missing_db_reports_no_error`: a nonexistent path yields an empty report with no entry in `errors`.

- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes** — 3 tests pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(collector): index opencode history"`

---

### Task 5: Tauri commands for history and pricing

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml` (add `dirs = "5"`)

**Interfaces:**
- Produces the eight commands in the Command API above, returning the exact camelCase shapes.
- The store lives at `<app data dir>/agent-deck.db`; the price table at `<app data dir>/pricing.json`.
  Use `tauri::Manager::path().app_data_dir()`, falling back to `dirs::data_dir()`.
- `AppState` gains `store: Mutex<Store>` and `pricing: Mutex<PriceTable>`, plus an
  `indexing: AtomicBool` so `reindex` cannot run twice at once and `index_status.running` is honest.
- On startup, spawn ONE background thread that runs a full incremental index pass once, then exits.
  It must not block the window from opening, and must never hold the store lock while reading files
  (collect rows, then lock, upsert, unlock).
- `days` is converted to `since_ms = now_ms - days*86_400_000`. `days <= 0` means all time.
- All query commands return `Result<T, String>` so a DB error surfaces in the UI instead of panicking.

- [ ] **Step 1:** Implement the state, paths, commands, and startup indexing thread.
- [ ] **Step 2:** Verify — `cargo check --manifest-path src-tauri/Cargo.toml --workspace` exits 0 and `cargo clippy --manifest-path src-tauri/Cargo.toml --workspace -- -D warnings` reports nothing (install clippy if missing; if it cannot be installed, say so and skip).
- [ ] **Step 3: Commit** — `git commit -m "feat(app): expose history and pricing commands"`

---

## Track B — Frontend

### Task 6: Router, nav and page shells

**Files:**
- Modify: `src/App.tsx`, `src/components/Sidebar.tsx`
- Create: `src/lib/nav.ts`, `src/pages/TokenPage.tsx` (shell only)

**Interfaces:**
- Produces: `NAV_ITEMS` in `src/lib/nav.ts` as
  `{ key: PageKey; label: string; icon: LucideIcon; enabled: boolean }[]` with
  `type PageKey = "live" | "token" | "timeline" | "savings" | "terminal" | "provider"`, labels
  exactly `Live`, `Token & Biaya`, `Timeline`, `Hemat Token`, `Terminal`, `Model & Provider`.
  In P2, `live` and `token` are enabled; the rest are `enabled: false`.
- `App.tsx` holds `const [page, setPage] = useState<PageKey>("live")` and renders `LivePage` or
  `TokenPage`. Do NOT add a routing library; a state switch is enough.
- `Sidebar` takes `{ page, onSelect }` props, renders from `NAV_ITEMS`, marks the active item, and
  renders disabled items dimmed and non-clickable. It keeps the existing waiting badge on `live`.

- [ ] **Step 1:** Write `src/lib/nav.ts`, update `Sidebar` and `App`, add a `TokenPage` shell that
  renders only its header.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit` exits 0, `bun test` still passes, `bun run build` exits 0.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): add page nav and token page shell"`

---

### Task 7: History API module and formatting

**Files:**
- Create: `src/lib/api.ts`, `src/lib/cost.ts`
- Test: `src/lib/cost.test.ts`

**Interfaces:**
- `src/lib/api.ts` exports the typed wrappers and the interfaces from the Command API section:
  `indexStatus()`, `reindex()`, `historyDaily(days)`, `historyByModel(days)`,
  `historyByProject(days)`, `historyTotals(days)`, `pricingGet()`, `pricingSet(entries)`.
  Each is a one-line `invoke<T>(...)` call. Export the types too.
- `src/lib/cost.ts` exports:
  - `formatUsd(n: number): string` — `"$12,60"` style: two decimals, comma decimal separator, `$` prefix; values below 0.01 but above 0 render `"<$0,01"`; exactly 0 renders `"$0,00"`.
  - `formatPct(n: number): string` — `"87%"`, rounded to a whole number.
  - `stackByDate(rows: DailyRow[]): { date: string; claude: number; opencode: number; codex: number; total: number }[]` — one entry per date, sorted ascending by date, token totals per agent, zero when an agent has no row that day.

- [ ] **Step 1: Write the failing tests** in `src/lib/cost.test.ts`:
1. `formatUsd` for `12.6` → `"$12,60"`, `0` → `"$0,00"`, `0.004` → `"<$0,01"`, `1234.5` → `"$1234,50"`.
2. `formatPct` for `87.4` → `"87%"`, `0` → `"0%"`.
3. `stackByDate` merges two agents on the same date into one entry, fills missing agents with 0, and sorts dates ascending.
- [ ] **Step 2: Run to verify it fails** — `bun test`.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run to verify it passes.**
- [ ] **Step 5: Commit** — `git commit -m "feat(ui): add history api and cost formatting"`

---

### Task 8: Token & Biaya screen

**Files:**
- Modify: `src/pages/TokenPage.tsx`
- Create: `src/components/TokenChart.tsx`, `src/components/ModelTable.tsx`, `src/components/RangeFilter.tsx`
- Modify: `package.json` (add `recharts`, `@tanstack/react-query`), `src/main.tsx` (wrap in `QueryClientProvider`)

**Interfaces:**
- Consumes Task 7's api module and `formatTokens` / `totalTokens` from P1.
- `RangeFilter` props: `{ value: number; onChange: (days: number) => void; options?: number[] }`,
  default options `[1, 7, 14, 30]`, labels `Hari ini`, `7 hari`, `14 hari`, `30 hari`.
- `TokenChart` props: `{ data: ReturnType<typeof stackByDate> }`, a recharts stacked `BarChart`
  with one `Bar` per agent using the CSS variables `--color-claude`, `--color-opencode`,
  `--color-codex`, a dark tooltip, and no grid lines except a subtle horizontal one.
- `ModelTable` props: `{ rows: ModelRow[] }`, columns Model, Agent, Input, Output, Cache, Biaya,
  sorted by cost descending, with an agent colour dot, numbers right-aligned and monospace.
- `TokenPage` uses `useQuery` for totals, daily and by-model with the selected range, shows the four
  KPIs (Total token, Estimasi biaya, Cache hit, Token reasoning), the chart, then the table. It must
  render a loading state, an error state showing the error string, and an empty state
  (`Belum ada data. Index sedang berjalan atau belum ada aktivitas.`).
- Layout, spacing and colours follow `docs/design/02-token-biaya.png`.

- [ ] **Step 1:** `bun add recharts @tanstack/react-query`, wrap the root in `QueryClientProvider`.
- [ ] **Step 2:** Write the three components and the page.
- [ ] **Step 3:** Verify — `bunx tsc --noEmit` exits 0, `bun test` passes, `bun run build` exits 0.
- [ ] **Step 4: Commit** — `git commit -m "feat(ui): add token and cost page"`

---

### Task 9: Pricing editor

**Files:**
- Create: `src/components/PricingDialog.tsx`
- Modify: `src/pages/TokenPage.tsx`

**Interfaces:**
- A dialog opened from a `Harga model` button in the Token page header. It lists the price entries in
  an editable table (model, input, output, cache read, cache write, all per million), allows adding
  and removing a row, and saves with `pricingSet`. On save it invalidates the `history` queries so
  costs refresh.
- Inputs accept decimals; a non-numeric value is rejected with an inline message
  `Harus angka` and the save button is disabled while any field is invalid.
- Include a visible note: `Harga ini perkiraan. Sesuaikan dengan tagihan asli kamu.`
- Use plain React state; do not add a dialog library. Render it as a fixed overlay.

- [ ] **Step 1:** Implement the dialog and wire it into the page.
- [ ] **Step 2:** Verify — `bunx tsc --noEmit`, `bun test`, `bun run build` all clean.
- [ ] **Step 3: Commit** — `git commit -m "feat(ui): add pricing editor dialog"`

---

## Done when

- `cd src-tauri && cargo test -p collector --lib` passes (25 from P1 + 27 new = 52).
- `bun test` passes (11 from P1 + 3 new = 14).
- `bunx tsc --noEmit`, `bun run build`, and `cargo check --workspace` are all clean.
- One commit per task, none with a `Co-Authored-By` line, nothing pushed.

## Self-Review

- **Spec coverage:** history index with mtime/size caching (Task 2-4), pricing table (Task 1, 9), per-day and per-model aggregates (Task 2, 5), Token & Biaya screen (Task 8), privacy rule that only tool names and usage are stored (Task 2 schema has no text column). Timeline is deliberately NOT in this plan; it follows in the P2b plan.
- **Type consistency:** `DailyRow`/`ModelRow`/`ProjectRow`/`Totals`/`PriceEntry` appear identically in the Command API section, Task 2's Rust aggregates, Task 5's commands and Task 7's TypeScript.
- **Known gap:** Claude subagent usage is attributed to the subagent file's own records, not merged into the parent session row. That matches P1's behaviour and is intentional.
