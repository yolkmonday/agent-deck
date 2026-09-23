# Model performance (TPS) — design spec

## 1. Goal

Help the user pick the fastest model/provider. The main case is comparing the same model served by different providers, e.g. `kn/deepseek-v4-1-flash` vs `aki/cbai/deepseek-v4.1-flash(high)` vs `Sumo/deepseek-v4.1-flash:netra`.

Non-goals:
- live "provider is slow right now" alerts
- cost per token
- benchmarking by sending requests (the app stays read-only on agent data)

## 2. Where

A new **Performa** tab on the Token & Biaya page (`src/pages/TokenPage.tsx`), next to the existing content, which becomes the **Biaya** tab. No new nav item.

## 3. Data: one sample per model response

The history indexer (`src-tauri/crates/collector/src/indexer.rs`, store in `store.rs`) gains a table in `agent-deck.db`:

```sql
CREATE TABLE IF NOT EXISTS response_perf (
  id            TEXT PRIMARY KEY,  -- stable per response, so reindexing never duplicates
  agent         TEXT NOT NULL,     -- claude | opencode | codex
  session_id    TEXT NOT NULL,
  model         TEXT NOT NULL,     -- same model string the message table uses (provider/model for opencode)
  ts_ms         INTEGER NOT NULL,  -- response end time
  output_tokens INTEGER NOT NULL,  -- output + reasoning
  gen_ms        INTEGER NOT NULL,  -- generation window
  ttft_ms       INTEGER,           -- NULL when unknown
  precise       INTEGER NOT NULL   -- 1 = measured from stream timing, 0 = estimated
);
CREATE INDEX IF NOT EXISTS response_perf_ts ON response_perf (ts_ms);
```

Samples are written by the same incremental passes that already fill `message`, reusing their file-progress / mtime cache. `delete_session` also deletes that session's perf rows.

### 3.1 Sample extraction per agent

- **opencode (precise).** One sample per assistant step, built from the `part` table:
  - A step runs from `step-start` to `step-finish`. The step's output tokens come from the `step-finish` tokens, or the message tokens when a message has a single step.
  - `ttft_ms` = first `text`/`reasoning` part `time.start` − step start.
  - `gen_ms` = step end − first `text`/`reasoning` part `time.start`.
  - If step boundaries have no timestamps, fall back to the message: `ttft` = first part start − `time.created`, and `gen` = `time.completed` − first part start.
  - The implementer verifies against real rows (read-only sqlite on `~/.local/share/opencode/opencode.db`, `part` and `message` tables only) and documents the rule used in a code comment.
- **Claude Code (estimated).**
  - One sample per assistant `message.id`. The jsonl splits one API message into several records with the same `message.id`.
  - The window runs from the timestamp of the record before that message's first record to the timestamp of its last record. The previous record is the user prompt or the tool_result.
  - `output_tokens` = `usage.output_tokens`, counted once per `message.id` using the existing dedupe.
  - `ttft_ms` = NULL, `precise` = 0.
- **Codex (estimated).**
  - One sample per `event_msg` `token_count` that has `info.last_token_usage.output_tokens`, counting reasoning tokens as well if present.
  - The window runs from the previous request boundary to the `token_count` timestamp. The boundary is the latest `function_call_output`, user `message` or `task_started` before it.
  - `ttft_ms` = NULL, `precise` = 0.

### 3.2 Noise filter

Drop samples with `output_tokens < 20` or `gen_ms < 200`. Also drop samples with `gen_ms > 30 min`, which usually means a stuck stream or a clock gap.

## 4. Aggregation

Command `perf_by_model(range: "24h" | "7d" | "30d", group_by_family: bool)` returns rows:

```
{ key, agent, models[], samples, tpsP50, tpsP10, ttftP50Ms|null, precise: bool, daily: [{ day, tpsP50 }] }
```

- **TPS** per sample = `output_tokens / (gen_ms / 1000)`.
- `tpsP50` is the median. `tpsP10` is the 10th percentile, the "slow case". `ttftP50Ms` is the median over samples with a TTFT.
- `precise` is true only if every sample in the row is precise.
- Rows with fewer than 5 samples are returned but marked (`samples` shows it). The UI greys them out.
- **Model family** (`group_by_family`): the normalized model id.
  - Strip the provider prefix up to the last `/`.
  - Strip a trailing `(…)` or `:…` suffix.
  - Lowercase.
  - Replace `.` with `-` between digits (`v4.1` → `v4-1`).
  - Example: `kn/deepseek-v4-1-flash`, `aki/cbai/deepseek-v4.1-flash(high)` and `Sumo/deepseek-v4.1-flash:netra` all map to `deepseek-v4-1-flash`.
  - In family mode, each row is one family. It expands to show its provider/model rows, sorted by `tpsP50` descending.
- Percentiles use the nearest-rank method, computed in Rust. It is a pure function with unit tests.

## 5. UI (Performa tab)

- Range selector: 24 jam / 7 hari / 30 hari (default 7 hari). Toggle: **Kelompokkan per model**. Search box filtering by model text.
- Table columns:
  - Model: agent icon + `provider/model`
  - TPS p50: bold
  - TPS p10
  - TTFT p50: `-` if unknown
  - Sampel
- The table is sortable by any numeric column, and defaults to TPS p50 descending.
- Estimated rows show `~` before the TPS values, with the tooltip "Perkiraan dari timestamp sesi, termasuk waktu tunggu token pertama."
- Rows with fewer than 5 samples are dimmed, with the tooltip "Sampel sedikit".
- Clicking a row selects it (multi-select up to 5). A recharts line chart below shows `tpsP50` per day for the selected rows. Default: the top 3 by samples.
- Empty state: "Belum ada data performa. Data terisi saat indexer membaca riwayat sesi."
- Style follows `ModelTable` and the existing Token page (same KPI/table classes, Indonesian copy, `ad-interactive`).

## 6. Testing

- **Rust** (`cargo test` in collector):
  - sample extraction fixtures for opencode, Claude and Codex, including the noise filter and Claude multi-record dedupe
  - reindexing does not duplicate
  - `delete_session` removes perf rows
  - percentiles
  - family normalization
  - aggregation with and without family grouping
- **Frontend** (`bun test`): formatting helpers for TPS/TTFT and the sort comparator.
- **Manual:** the numbers roughly match the ad-hoc query from the brainstorm (e.g. `kn/deepseek-v4-1-flash` ≈ 40 tok/s end-to-end over 7 days, and higher when measured from the stream).

## 7. Open items

- The exact opencode step boundary fields are verified during implementation (§3.1).
