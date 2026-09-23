# Model Performance (TPS) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A "Performa" tab on Token & Biaya that ranks every provider/model by generation speed:
- TPS p50 and p10
- TTFT
- sample count

**Architecture:**
- The history indexer writes one `response_perf` row per model response into `agent-deck.db`.
  - opencode rows are precise, built from part timing.
  - Claude and Codex rows are estimated from jsonl timestamps.
- A pure Rust module filters noise, computes percentiles and normalizes model families.
- A Tauri command serves the aggregates. The React tab renders a sortable table and a daily TPS line chart.

**Tech Stack:**
- Rust: rusqlite, serde_json, Tauri v2 commands
- React 19, TanStack Query, recharts, Tailwind v4
- Tests: `cargo test`, `bun test`

**Spec:** `docs/superpowers/specs/2026-09-23-model-performance-design.md`

## Global Constraints

- **Working copy:** worktree `/Users/yolk/Dev/agent-deck-wt-perf`, branch `feat/model-perf`. Commit per task. Never push. Never merge.
- **Commit messages:**
  - English, lowercase, conventional prefix (`feat(collector): …`, `feat(ui): …`)
  - NO `Co-Authored-By` or any other trailer
- **Noise filter:** drop samples with `output_tokens < 20`, `gen_ms < 200`, or `gen_ms > 1_800_000`.
- **Low-sample threshold:** fewer than 5 samples means "Sampel sedikit" (dimmed).
- **Range values:** `"24h" | "7d" | "30d"`, default `"7d"`.
- **Copy:**
  - UI copy is Indonesian, verbatim from the spec: "Perkiraan dari timestamp sesi, termasuk waktu tunggu token pertama.", "Sampel sedikit", "Belum ada data performa. Data terisi saat indexer membaca riwayat sesi.", "Kelompokkan per model".
  - Identifiers and comments are English.
- **TS style:** `@/` imports, arrow functions, `const`.
- **Rust style:** module style of `collector` (see `store.rs`, `indexer.rs`).
- **Data access:** never read `~/.config/opencode/*` or `~/.local/share/opencode/auth.json`. Tests use fixtures only.
- **Test commands:**
  - collector: `cd src-tauri/crates/collector && cargo test`
  - app: `cd src-tauri && cargo test`
  - frontend: `bun test`, then `bunx tsc --noEmit`
  - First run `bun install --frozen-lockfile` in the worktree if `node_modules` is missing.

## Review Focus

1. **Incremental re-reads of a growing Claude jsonl must not create a second, wrong sample.**
   - Case: a message split across two read passes.
   - Expected: the upsert merge keeps the first `start_ms` and extends `end_ms`, and `gen_ms` is recomputed.
   - Test: Task 1 `upsert_merges_split_claude_message`.
2. **An opencode step whose tool call runs 60s must not show ~0 TPS.**
   - Tool execution time is excluded from `gen_ms`.
   - Test: Task 3 `opencode_step_excludes_tool_execution`.
3. **A model with a single huge outlier must not dominate.**
   - Median and p10 are used, never the mean.
   - Test: Task 2 `aggregate_uses_median_not_mean`.
4. **Reindex from scratch (store wiped) must give the same row count.**
   - Test: Task 3 `reindex_does_not_duplicate`, plus Claude and Codex variants in Tasks 4 and 5.
5. **Family grouping must never merge different sizes** (`deepseek-v4-pro` vs `deepseek-v4-1-flash`).
   - Test: Task 2 `family_keeps_distinct_models_apart`.

---

### Task 1: `response_perf` table and store API

**Files:**
- Modify: `src-tauri/crates/collector/src/store.rs`:
  - SCHEMA const (lines 8-56)
  - new struct next to `MessageRow` (59-67)
  - `delete_session` (338-344)
  - new methods after `upsert_spans` (~221)
- Test: `store.rs` `#[cfg(test)] mod tests` (existing module at the bottom)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq)]
  pub struct PerfRow {
      pub id: String,
      pub agent: Agent,
      pub session_id: String,
      pub model: String,
      pub start_ms: i64,
      pub end_ms: i64,
      pub gen_ms: i64,
      pub output_tokens: i64,
      pub ttft_ms: Option<i64>,
      pub precise: bool,
  }
  impl Store {
      pub fn upsert_perf(&mut self, rows: &[PerfRow]) -> Result<usize>;
      pub fn perf_samples(&self, since_ms: i64) -> Result<Vec<PerfRow>>; // end_ms >= since_ms, ordered by end_ms
  }
  ```

- [ ] **Step 1: Write the failing tests** (append to the tests module in `store.rs`; reuse whatever helper the module already uses to open a temp store — look at the existing tests' first lines and copy their setup)

```rust
fn perf(id: &str, start: i64, end: i64, gen: i64, out: i64) -> PerfRow {
    PerfRow {
        id: id.into(),
        agent: Agent::Claude,
        session_id: "s1".into(),
        model: "claude-opus-4-8".into(),
        start_ms: start,
        end_ms: end,
        gen_ms: gen,
        output_tokens: out,
        ttft_ms: None,
        precise: false,
    }
}

#[test]
fn upsert_perf_inserts_and_reads_back() {
    let mut s = test_store();
    s.upsert_perf(&[perf("claude:a", 1_000, 3_000, 2_000, 100)]).unwrap();
    let rows = s.perf_samples(0).unwrap();
    assert_eq!(rows, vec![perf("claude:a", 1_000, 3_000, 2_000, 100)]);
}

#[test]
fn upsert_merges_split_claude_message() {
    let mut s = test_store();
    s.upsert_perf(&[perf("claude:a", 1_000, 2_000, 1_000, 50)]).unwrap();
    // second pass: later chunk of the same message; its start anchor is unknown-ish (later)
    s.upsert_perf(&[perf("claude:a", 1_900, 4_000, 2_100, 130)]).unwrap();
    let rows = s.perf_samples(0).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].start_ms, 1_000);
    assert_eq!(rows[0].end_ms, 4_000);
    assert_eq!(rows[0].gen_ms, 3_000);
    assert_eq!(rows[0].output_tokens, 130);
}

#[test]
fn delete_session_removes_perf_rows() {
    let mut s = test_store();
    s.upsert_perf(&[perf("claude:a", 1_000, 3_000, 2_000, 100)]).unwrap();
    s.delete_session(Agent::Claude, "s1").unwrap();
    assert!(s.perf_samples(0).unwrap().is_empty());
}

#[test]
fn perf_samples_filters_by_since() {
    let mut s = test_store();
    s.upsert_perf(&[perf("claude:old", 0, 1_000, 1_000, 100), perf("claude:new", 5_000, 9_000, 4_000, 100)]).unwrap();
    let rows = s.perf_samples(2_000).unwrap();
    assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["claude:new"]);
}
```

If the module has no `test_store()` helper, add one that does `Store::open(&tempfile::tempdir().unwrap().into_path().join("t.db")).unwrap()`.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cd src-tauri/crates/collector && cargo test store::tests::upsert_perf`
Expected: compile error (`PerfRow` / `upsert_perf` not found).

- [ ] **Step 3: Implement**

Append to `SCHEMA`:

```sql
CREATE TABLE IF NOT EXISTS response_perf (
  id            TEXT PRIMARY KEY,
  agent         TEXT NOT NULL,
  session_id    TEXT NOT NULL,
  model         TEXT NOT NULL,
  start_ms      INTEGER NOT NULL,
  end_ms        INTEGER NOT NULL,
  gen_ms        INTEGER NOT NULL,
  output_tokens INTEGER NOT NULL,
  ttft_ms       INTEGER,
  precise       INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS response_perf_end ON response_perf (end_ms);
```

`upsert_perf` runs in one transaction, like `upsert_messages`. The merge rule keeps the earliest start and the latest end. For estimated rows, `gen_ms` is recomputed from the merged window. For precise rows, the incoming `gen_ms` is kept, because opencode rows are rewritten whole:

```rust
pub fn upsert_perf(&mut self, rows: &[PerfRow]) -> Result<usize> {
    let tx = self.conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO response_perf (id, agent, session_id, model, start_ms, end_ms, gen_ms, output_tokens, ttft_ms, precise)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               start_ms = MIN(response_perf.start_ms, excluded.start_ms),
               end_ms = MAX(response_perf.end_ms, excluded.end_ms),
               output_tokens = excluded.output_tokens,
               ttft_ms = excluded.ttft_ms,
               gen_ms = CASE WHEN excluded.precise = 1 THEN excluded.gen_ms
                             ELSE MAX(response_perf.end_ms, excluded.end_ms) - MIN(response_perf.start_ms, excluded.start_ms) END,
               model = excluded.model",
        )?;
        for r in rows {
            stmt.execute(rusqlite::params![
                r.id, agent_str(r.agent), r.session_id, r.model, r.start_ms, r.end_ms,
                r.gen_ms, r.output_tokens, r.ttft_ms, r.precise as i64
            ])?;
        }
    }
    tx.commit()?;
    Ok(rows.len())
}

pub fn perf_samples(&self, since_ms: i64) -> Result<Vec<PerfRow>> {
    let mut stmt = self.conn.prepare(
        "SELECT id, agent, session_id, model, start_ms, end_ms, gen_ms, output_tokens, ttft_ms, precise
         FROM response_perf WHERE end_ms >= ?1 ORDER BY end_ms",
    )?;
    let rows = stmt
        .query_map([since_ms], |r| {
            Ok(PerfRow {
                id: r.get(0)?,
                agent: agent_from_str(&r.get::<_, String>(1)?),
                session_id: r.get(2)?,
                model: r.get(3)?,
                start_ms: r.get(4)?,
                end_ms: r.get(5)?,
                gen_ms: r.get(6)?,
                output_tokens: r.get(7)?,
                ttft_ms: r.get(8)?,
                precise: r.get::<_, i64>(9)? == 1,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
```

Adapt `Result`, `agent_str`/`agent_from_str` and the error conversion to what `store.rs` already uses. In `delete_session`, add a second statement: `DELETE FROM response_perf WHERE agent = ?1 AND session_id = ?2`.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cd src-tauri/crates/collector && cargo test store::`
Expected: all store tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/collector/src/store.rs
git commit -m "feat(collector): store per-response performance samples"
```

---

### Task 2: Pure performance math (`perf.rs`)

**Files:**
- Create: `src-tauri/crates/collector/src/perf.rs`
- Modify: `src-tauri/crates/collector/src/lib.rs` (add `pub mod perf;`)

**Interfaces:**
- Consumes: `store::PerfRow`, `model::Agent`.
- Produces:
  ```rust
  pub const MIN_OUTPUT_TOKENS: i64 = 20;
  pub const MIN_GEN_MS: i64 = 200;
  pub const MAX_GEN_MS: i64 = 1_800_000;
  pub const LOW_SAMPLES: usize = 5;
  pub fn keep_sample(r: &PerfRow) -> bool;
  pub fn tps(r: &PerfRow) -> f64;                       // output_tokens / (gen_ms / 1000)
  pub fn percentile(sorted: &[f64], p: f64) -> f64;     // nearest-rank, p in 0..=100, sorted ascending, non-empty
  pub fn family(model: &str) -> String;
  #[derive(Debug, Clone, PartialEq, serde::Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct DailyTps { pub day: String, pub tps_p50: f64 }   // day = "YYYY-MM-DD" UTC
  #[derive(Debug, Clone, PartialEq, serde::Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct PerfAgg {
      pub key: String,              // "provider/model" or family
      pub agent: Agent,
      pub models: Vec<String>,      // distinct raw model strings in this row
      pub samples: usize,
      pub tps_p50: f64,
      pub tps_p10: f64,
      pub ttft_p50_ms: Option<i64>,
      pub precise: bool,
      pub daily: Vec<DailyTps>,
      pub children: Vec<PerfAgg>,   // family mode only: per-model rows sorted by tps_p50 desc; empty otherwise
  }
  pub fn aggregate(rows: &[PerfRow], group_by_family: bool) -> Vec<PerfAgg>; // sorted by tps_p50 desc
  ```
- Check `model::Agent` derives `Serialize` with lowercase (it does for `Session`; if not, add `#[serde(rename_all = "lowercase")]`, matching `model.rs:164` test expectations).
- Grouping key:
  - Non-family mode: `(agent, model)`.
  - Family mode: `family(model)`. The row's `agent` is the agent of its first sample. `children` holds the non-family aggregation of its rows.

- [ ] **Step 1: Write the failing tests** (bottom of `perf.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Agent;
    use crate::store::PerfRow;

    fn s(model: &str, end: i64, gen: i64, out: i64, ttft: Option<i64>, precise: bool) -> PerfRow {
        PerfRow {
            id: format!("{model}:{end}"),
            agent: Agent::Opencode,
            session_id: "s".into(),
            model: model.into(),
            start_ms: end - gen,
            end_ms: end,
            gen_ms: gen,
            output_tokens: out,
            ttft_ms: ttft,
            precise,
        }
    }

    #[test]
    fn noise_filter() {
        assert!(!keep_sample(&s("m", 10_000, 1_000, 19, None, true)));
        assert!(!keep_sample(&s("m", 10_000, 199, 100, None, true)));
        assert!(!keep_sample(&s("m", 10_000_000, 1_800_001, 100, None, true)));
        assert!(keep_sample(&s("m", 10_000, 1_000, 20, None, true)));
    }

    #[test]
    fn tps_is_tokens_per_second() {
        assert_eq!(tps(&s("m", 10_000, 2_000, 100, None, true)), 50.0);
    }

    #[test]
    fn percentile_nearest_rank() {
        let v = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert_eq!(percentile(&v, 50.0), 5.0);
        assert_eq!(percentile(&v, 10.0), 1.0);
        assert_eq!(percentile(&v, 100.0), 10.0);
        assert_eq!(percentile(&[7.0], 10.0), 7.0);
    }

    #[test]
    fn family_normalizes_provider_suffix_and_dots() {
        assert_eq!(family("kn/deepseek-v4-1-flash"), "deepseek-v4-1-flash");
        assert_eq!(family("aki/cbai/deepseek-v4.1-flash(high)"), "deepseek-v4-1-flash");
        assert_eq!(family("Sumo/deepseek-v4.1-flash:netra"), "deepseek-v4-1-flash");
        assert_eq!(family("claude-opus-4-8"), "claude-opus-4-8");
    }

    #[test]
    fn family_keeps_distinct_models_apart() {
        assert_ne!(family("kn/deepseek-v4-pro"), family("kn/deepseek-v4-1-flash"));
        assert_ne!(family("oa/claude-opus-4.8"), family("oa/claude-opus-5"));
    }

    #[test]
    fn aggregate_uses_median_not_mean() {
        // tps: 10,10,10,10,1000 -> median 10
        let rows = vec![
            s("kn/a", 1_000, 2_000, 20, None, true),   // 10 tps
            s("kn/a", 2_000, 2_000, 20, None, true),   // 10
            s("kn/a", 3_000, 2_000, 20, None, true),   // 10
            s("kn/a", 4_000, 2_000, 20, None, true),   // 10
            s("kn/a", 5_000, 1_000, 1_000, None, true), // 1000
        ];
        let out = aggregate(&rows, false);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].tps_p50, 10.0);
        assert_eq!(out[0].tps_p10, 10.0);
        assert_eq!(out[0].samples, 5);
    }

    #[test]
    fn aggregate_drops_noise_and_reports_ttft_and_precision() {
        let rows = vec![
            s("kn/a", 1_000, 1_000, 100, Some(300), true),
            s("kn/a", 2_000, 1_000, 100, Some(500), true),
            s("kn/a", 3_000, 1_000, 5, Some(1), true), // noise: < 20 tokens
        ];
        let out = aggregate(&rows, false);
        assert_eq!(out[0].samples, 2);
        assert_eq!(out[0].ttft_p50_ms, Some(300));
        assert!(out[0].precise);
    }

    #[test]
    fn aggregate_family_groups_providers_with_children() {
        let rows = vec![
            s("kn/deepseek-v4-1-flash", 1_000, 1_000, 40, None, true),
            s("aki/cbai/deepseek-v4.1-flash(high)", 2_000, 1_000, 25, None, true),
            s("kn/deepseek-v4-pro", 3_000, 1_000, 12 + 20, None, true),
        ];
        let out = aggregate(&rows, true);
        let flash = out.iter().find(|r| r.key == "deepseek-v4-1-flash").unwrap();
        assert_eq!(flash.samples, 2);
        assert_eq!(flash.children.len(), 2);
        assert_eq!(flash.children[0].key, "kn/deepseek-v4-1-flash"); // 40 tps > 25 tps
        assert!(out.iter().any(|r| r.key == "deepseek-v4-pro"));
    }

    #[test]
    fn daily_buckets_by_utc_day() {
        let day1 = 1_790_000_000_000; // some UTC ms
        let rows = vec![
            s("kn/a", day1, 1_000, 100, None, true),
            s("kn/a", day1 + 86_400_000, 1_000, 50, None, true),
        ];
        let out = aggregate(&rows, false);
        assert_eq!(out[0].daily.len(), 2);
        assert_eq!(out[0].daily[0].tps_p50, 100.0);
        assert_eq!(out[0].daily[1].tps_p50, 50.0);
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cd src-tauri/crates/collector && cargo test perf::`
Expected: compile errors, because the functions do not exist yet.

- [ ] **Step 3: Implement** (top of `perf.rs`)

```rust
//! Pure performance math for model responses: noise filter, TPS, percentiles,
//! model-family normalization and per-model aggregation.
use std::collections::BTreeMap;

use crate::model::Agent;
use crate::store::PerfRow;

pub const MIN_OUTPUT_TOKENS: i64 = 20;
pub const MIN_GEN_MS: i64 = 200;
pub const MAX_GEN_MS: i64 = 1_800_000;
pub const LOW_SAMPLES: usize = 5;

pub fn keep_sample(r: &PerfRow) -> bool {
    r.output_tokens >= MIN_OUTPUT_TOKENS && r.gen_ms >= MIN_GEN_MS && r.gen_ms <= MAX_GEN_MS
}

pub fn tps(r: &PerfRow) -> f64 {
    r.output_tokens as f64 / (r.gen_ms as f64 / 1000.0)
}

/// Nearest-rank percentile over an ascending, non-empty slice.
pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    let rank = ((p / 100.0) * n as f64).ceil().max(1.0) as usize;
    sorted[rank.min(n) - 1]
}

/// `aki/cbai/deepseek-v4.1-flash(high)` -> `deepseek-v4-1-flash`.
pub fn family(model: &str) -> String {
    let base = model.rsplit('/').next().unwrap_or(model);
    let base = base.split(['(', ':']).next().unwrap_or(base).trim();
    let chars: Vec<char> = base.to_lowercase().chars().collect();
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let between_digits = *c == '.'
                && i > 0
                && chars[i - 1].is_ascii_digit()
                && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit());
            if between_digits { '-' } else { *c }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyTps {
    pub day: String,
    pub tps_p50: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerfAgg {
    pub key: String,
    pub agent: Agent,
    pub models: Vec<String>,
    pub samples: usize,
    pub tps_p50: f64,
    pub tps_p10: f64,
    pub ttft_p50_ms: Option<i64>,
    pub precise: bool,
    pub daily: Vec<DailyTps>,
    pub children: Vec<PerfAgg>,
}

fn utc_day(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    // civil-from-days (Howard Hinnant), avoids a date crate dependency
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn sorted_tps(rows: &[&PerfRow]) -> Vec<f64> {
    let mut v: Vec<f64> = rows.iter().map(|r| tps(r)).collect();
    v.sort_by(|a, b| a.total_cmp(b));
    v
}

fn build(key: String, rows: &[&PerfRow]) -> PerfAgg {
    let all = sorted_tps(rows);
    let mut ttft: Vec<i64> = rows.iter().filter_map(|r| r.ttft_ms).collect();
    ttft.sort_unstable();
    let ttft_p50_ms = if ttft.is_empty() {
        None
    } else {
        let f: Vec<f64> = ttft.iter().map(|v| *v as f64).collect();
        Some(percentile(&f, 50.0) as i64)
    };
    let mut by_day: BTreeMap<String, Vec<&PerfRow>> = BTreeMap::new();
    for r in rows {
        by_day.entry(utc_day(r.end_ms)).or_default().push(r);
    }
    let daily = by_day
        .into_iter()
        .map(|(day, rs)| DailyTps { day, tps_p50: percentile(&sorted_tps(&rs), 50.0) })
        .collect();
    let mut models: Vec<String> = rows.iter().map(|r| r.model.clone()).collect();
    models.sort();
    models.dedup();
    PerfAgg {
        key,
        agent: rows[0].agent,
        models,
        samples: rows.len(),
        tps_p50: percentile(&all, 50.0),
        tps_p10: percentile(&all, 10.0),
        ttft_p50_ms,
        precise: rows.iter().all(|r| r.precise),
        daily,
        children: Vec::new(),
    }
}

fn by_model(rows: &[&PerfRow]) -> Vec<PerfAgg> {
    let mut groups: BTreeMap<(String, String), Vec<&PerfRow>> = BTreeMap::new();
    for r in rows {
        groups.entry((format!("{:?}", r.agent), r.model.clone())).or_default().push(r);
    }
    let mut out: Vec<PerfAgg> = groups.into_iter().map(|((_, m), rs)| build(m, &rs)).collect();
    out.sort_by(|a, b| b.tps_p50.total_cmp(&a.tps_p50));
    out
}

pub fn aggregate(rows: &[PerfRow], group_by_family: bool) -> Vec<PerfAgg> {
    let kept: Vec<&PerfRow> = rows.iter().filter(|r| keep_sample(r)).collect();
    if !group_by_family {
        return by_model(&kept);
    }
    let mut fams: BTreeMap<String, Vec<&PerfRow>> = BTreeMap::new();
    for r in &kept {
        fams.entry(family(&r.model)).or_default().push(r);
    }
    let mut out: Vec<PerfAgg> = fams
        .into_iter()
        .map(|(f, rs)| {
            let mut agg = build(f, &rs);
            agg.children = by_model(&rs);
            agg
        })
        .collect();
    out.sort_by(|a, b| b.tps_p50.total_cmp(&a.tps_p50));
    out
}
```

Add `pub mod perf;` to `collector/src/lib.rs`. `Agent` must be `Copy`, `Debug` and `Serialize`. If it is not `Copy`, use `.clone()`.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cd src-tauri/crates/collector && cargo test perf::`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/collector/src/perf.rs src-tauri/crates/collector/src/lib.rs
git commit -m "feat(collector): add tps percentiles and model family aggregation"
```

---

### Task 3: opencode samples (precise)

**Files:**
- Modify: `src-tauri/crates/collector/src/indexer.rs`
  - `index_opencode` (543-638)
  - its tool-span helper `upsert_opencode_spans` (487-540) is the pattern to copy
- Test: `indexer.rs` tests
  - fake DB builder `opencode_db` (992-1007)
  - `insert_message` (1035-1045)
  - `insert_tool_part` (1011-1033)

**Interfaces:**
- Consumes:
  - `Store::upsert_perf`
  - `PerfRow` (Task 1)
  - the model string rule already in `index_opencode` (`format!("{p}/{m}")`, lines 592-599)
- Produces: `fn upsert_opencode_perf(store: &mut Store, conn: &Connection, since: i64, report: &mut IndexReport)`. It is called from `index_opencode` with the same `since` that the message pass uses.

**Data facts (verified on real data):**
- A part row has the columns `id`, `message_id`, `session_id`, `time_created`, `time_updated`, `data`.
- `step-start` and `step-finish` parts have NO time inside `data`. `step-finish.data.tokens = {total,input,output,reasoning,cache{read,write}}`.
- `text` and `reasoning` parts have `data.time.start` and `data.time.end` (epoch ms).
- `tool` parts have `data.state.time.start`/`end`, which is tool execution. The row's `time_created` is when the tool call began streaming.

**Algorithm:**
1. Select the parts of messages with `message.time_created > since`, ordered by `message_id, part.time_created, part.id`: `SELECT p.id, p.message_id, p.session_id, p.time_created, p.data, m.data FROM part p JOIN message m ON m.id = p.message_id WHERE m.time_created > ?1 ORDER BY p.message_id, p.time_created, p.id`.
2. Walk each message.
   - `step-start` opens a step: record `start = time_created`, `first = None`, `gen = 0`.
   - A `text` or `reasoning` part adds `end - start` (clamped to at least 0) to `gen`, and sets `first = min(first, data.time.start)`.
   - A `tool` part adds `state.time.start - part.time_created` (clamped to at least 0) to `gen`, and sets `first = min(first, part.time_created)`.
   - `step-finish` closes the step:
     - If `gen > 0`, push a `PerfRow`:
       - `id: "opencode:{step-finish part id}"`, `precise: true`
       - `start_ms: start`, `end_ms: step-finish time_created`, `gen_ms: gen`
       - `output_tokens: tokens.output + tokens.reasoning`
       - `ttft_ms: first.map(|f| (f - start).max(0))`
       - model from the message JSON (`providerID`/`modelID`, same rule as the message pass)
     - Skip the step when the model is missing.
3. Call `store.upsert_perf(&rows)`. Push errors into `report.errors` as strings, the same way as the other upserts.

- [ ] **Step 1: Write the failing tests** (in the `indexer.rs` tests module)

Add a helper that inserts generic parts, right next to `insert_tool_part`:

```rust
fn insert_part(c: &Connection, id: &str, msg: &str, created: i64, data: serde_json::Value) {
    c.execute(
        "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) VALUES (?1, ?2, 'ses1', ?3, ?3, ?4)",
        rusqlite::params![id, msg, created, data.to_string()],
    ).unwrap();
}
```

Tests (reuse `opencode_db`, `insert_message` and the existing session setup those helpers expect; read one existing `index_opencode` test first and mirror it):

```rust
#[test]
fn opencode_step_yields_precise_sample() {
    let (tmp, db) = opencode_db();            // adapt to the helper's real return shape
    let c = Connection::open(&db).unwrap();
    insert_message(&c, "msg1", 1_000 /* adapt args */);
    insert_part(&c, "p1", "msg1", 1_000, serde_json::json!({"type":"step-start"}));
    insert_part(&c, "p2", "msg1", 1_400, serde_json::json!({"type":"reasoning","time":{"start":1_400,"end":1_900}}));
    insert_part(&c, "p3", "msg1", 1_900, serde_json::json!({"type":"text","time":{"start":1_900,"end":2_900}}));
    insert_part(&c, "p4", "msg1", 3_000, serde_json::json!({"type":"step-finish","tokens":{"output":120,"reasoning":30}}));
    let mut s = test_store();
    index_opencode(&mut s, &db, home());
    let rows = s.perf_samples(0).unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.id, "opencode:p4");
    assert_eq!(r.model, "kn/deepseek-v4-1-flash");
    assert_eq!(r.output_tokens, 150);
    assert_eq!(r.gen_ms, 1_500);
    assert_eq!(r.ttft_ms, Some(400));
    assert!(r.precise);
    drop(tmp);
}

#[test]
fn opencode_step_excludes_tool_execution() {
    // tool input streams 200ms (created 1_100 -> state.time.start 1_300), then executes 60s
    let (tmp, db) = opencode_db();
    let c = Connection::open(&db).unwrap();
    insert_message(&c, "msg1", 1_000);
    insert_part(&c, "p1", "msg1", 1_000, serde_json::json!({"type":"step-start"}));
    insert_part(&c, "p2", "msg1", 1_100, serde_json::json!({"type":"tool","state":{"time":{"start":1_300,"end":61_300}}}));
    insert_part(&c, "p3", "msg1", 61_400, serde_json::json!({"type":"step-finish","tokens":{"output":40,"reasoning":0}}));
    let mut s = test_store();
    index_opencode(&mut s, &db, home());
    let r = &s.perf_samples(0).unwrap()[0];
    assert_eq!(r.gen_ms, 200);
    assert_eq!(r.ttft_ms, Some(100));
    drop(tmp);
}

#[test]
fn opencode_step_without_timed_parts_is_skipped() {
    let (tmp, db) = opencode_db();
    let c = Connection::open(&db).unwrap();
    insert_message(&c, "msg1", 1_000);
    insert_part(&c, "p1", "msg1", 1_000, serde_json::json!({"type":"step-start"}));
    insert_part(&c, "p2", "msg1", 2_000, serde_json::json!({"type":"step-finish","tokens":{"output":40,"reasoning":0}}));
    let mut s = test_store();
    index_opencode(&mut s, &db, home());
    assert!(s.perf_samples(0).unwrap().is_empty());
    drop(tmp);
}

#[test]
fn reindex_does_not_duplicate() {
    let (tmp, db) = opencode_db();
    let c = Connection::open(&db).unwrap();
    insert_message(&c, "msg1", 1_000);
    insert_part(&c, "p1", "msg1", 1_000, serde_json::json!({"type":"step-start"}));
    insert_part(&c, "p2", "msg1", 1_100, serde_json::json!({"type":"text","time":{"start":1_100,"end":2_100}}));
    insert_part(&c, "p3", "msg1", 2_200, serde_json::json!({"type":"step-finish","tokens":{"output":40,"reasoning":0}}));
    let mut s = test_store();
    index_opencode(&mut s, &db, home());
    // force a full re-read: wipe progress the way other reindex tests do (look for an existing reindex test and copy its reset)
    index_opencode(&mut s, &db, home());
    assert_eq!(s.perf_samples(0).unwrap().len(), 1);
    drop(tmp);
}
```

Adjust the helper calls (`opencode_db()` return value, `insert_message` arguments, and `test_store`) to the real signatures in the file. Keep the assertions exactly as written.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cd src-tauri/crates/collector && cargo test indexer::tests::opencode_step`
Expected: FAIL, because no perf rows are produced.

- [ ] **Step 3: Implement** `upsert_opencode_perf` following the algorithm above. Call it from `index_opencode` right after the message upsert, with the same `since` value.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cd src-tauri/crates/collector && cargo test`
Expected: all PASS, including the pre-existing indexer tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/collector/src/indexer.rs
git commit -m "feat(collector): index precise opencode generation timing"
```

---

### Task 4: Claude samples (estimated)

**Files:**
- Modify: `src-tauri/crates/collector/src/indexer.rs`, `index_claude` (184-346), specifically the record loop (~229-300)
- Test: `indexer.rs` tests, reusing the fixtures `claude_assistant`, `claude_tool_use`, `claude_tool_result` (655-672)

**Interfaces:**
- Consumes: `PerfRow`, `Store::upsert_perf`, `parse_iso_ms`, `record_ts` (155-160).
- Produces: no new public API. `index_claude` now also writes perf rows.

**Data facts (verified):**
- One `message.id` can span several `assistant` records a few milliseconds apart, each with the same cumulative `usage.output_tokens`.
- The previous user record (a prompt or tool_result) is the request anchor.

**Algorithm (inside the existing per-file loop):**
- Keep `last_request_ts: Option<i64>`. It is updated on every record with `type == "user"`, which covers both a prompt and a tool_result.
- Keep `open: HashMap<String /*message.id*/, (start, end, out, model)>`.
- For an `assistant` record with `message.id` = `mid`, timestamp `ts` and `usage.output_tokens` = `out`:
  - If `mid` is not yet in `open` and `last_request_ts` is `Some(a)`, insert `(a, ts, out, model)`.
  - If `mid` is already in `open`, set `end = max(end, ts)` and `out = out`.
  - If `last_request_ts` is `None`, skip.
- After the loop, map every entry to a `PerfRow`:
  - `id: format!("claude:{mid}")`, `agent: Agent::Claude`, `session_id`
  - `start_ms: start`, `end_ms: end`, `gen_ms: end - start`, `output_tokens: out`
  - `ttft_ms: None`, `precise: false`
  - `model`: the same string the message row uses
- Upsert these rows next to `upsert_messages`. The store merge handles a message that continues in the next read pass.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn claude_split_message_yields_one_estimated_sample() {
    // write a jsonl with: user prompt @ t0, assistant(mid=m1, thinking) @ t0+2760, assistant(mid=m1, tool_use) @ t0+2765 (same usage.output_tokens=130)
    // use the existing fixture builders; set timestamps explicitly (RFC3339) and message.id = "m1"
    let (tmp, projects) = claude_projects_with(&[
        user_prompt_at("2026-09-23T02:02:19.282Z"),
        assistant_chunk_at("m1", "2026-09-23T02:02:22.038Z", 130),
        assistant_chunk_at("m1", "2026-09-23T02:02:22.043Z", 130),
    ]);
    let mut s = test_store();
    index_claude(&mut s, &projects, home());
    let rows = s.perf_samples(0).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "claude:m1");
    assert_eq!(rows[0].gen_ms, 2_761);
    assert_eq!(rows[0].output_tokens, 130);
    assert!(!rows[0].precise);
    assert_eq!(rows[0].ttft_ms, None);
    drop(tmp);
}

#[test]
fn claude_assistant_without_anchor_is_skipped() {
    let (tmp, projects) = claude_projects_with(&[assistant_chunk_at("m1", "2026-09-23T02:02:22.038Z", 130)]);
    let mut s = test_store();
    index_claude(&mut s, &projects, home());
    assert!(s.perf_samples(0).unwrap().is_empty());
    drop(tmp);
}

#[test]
fn claude_reindex_does_not_duplicate() {
    let (tmp, projects) = claude_projects_with(&[
        user_prompt_at("2026-09-23T02:02:19.282Z"),
        assistant_chunk_at("m1", "2026-09-23T02:02:22.038Z", 130),
    ]);
    let mut s = test_store();
    index_claude(&mut s, &projects, home());
    index_claude(&mut s, &projects, home());
    assert_eq!(s.perf_samples(0).unwrap().len(), 1);
    drop(tmp);
}
```

`claude_projects_with`, `user_prompt_at` and `assistant_chunk_at` are small new helpers in the tests module:
- They build the JSON lines the same way `claude_assistant` and `claude_tool_result` do (open those fixtures and copy their shape).
- They set `"timestamp"`, `"message":{"id":..., "model":"claude-opus-4-8", "usage":{"output_tokens":N, ...}}`.
- They write the lines to `<tmp>/projects/-tmp-proj/<uuid>.jsonl`, matching the directory layout the existing claude tests use.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cd src-tauri/crates/collector && cargo test indexer::tests::claude_`
Expected: the new tests FAIL.

- [ ] **Step 3: Implement** the algorithm above in `index_claude`.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cd src-tauri/crates/collector && cargo test`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/collector/src/indexer.rs
git commit -m "feat(collector): estimate claude response speed from transcript timestamps"
```

---

### Task 5: Codex samples (estimated)

**Files:**
- Modify: `src-tauri/crates/collector/src/indexer.rs`, `index_codex` (359-482), specifically the record loop (~410-465)
- Test: the `indexer.rs` tests, next to the existing codex tests (find the one that writes `rollout-*.jsonl` and copy its setup)

**Interfaces:** consumes `PerfRow`, `Store::upsert_perf`, `parse_iso_ms`.

**Algorithm:**
- Keep `last_request_ts: Option<i64>`. Set it on each of these records:
  - `response_item` whose `payload.type == "function_call_output"`
  - `response_item` whose `payload.type == "message"` and `payload.role == "user"`
  - `event_msg` whose `payload.type == "task_started"`
- On a `token_count` that produces a message row (same conditions as now, including a non-empty model), with timestamp `ts`:
  - If `last_request_ts` is `Some(a)` and `ts > a`, push a `PerfRow`:
    - `id: format!("codex:{stem}:{ts}")`, `agent: Agent::Codex`, `session_id: stem`, `model`
    - `start_ms: a`, `end_ms: ts`, `gen_ms: ts - a`
    - `output_tokens`: `last_token_usage.output_tokens + last_token_usage.reasoning_output_tokens`, each treated as 0 when absent
    - `ttft_ms: None`, `precise: false`
  - Then set `last_request_ts = None`, so two token_counts without a new request are not both anchored to the same request.
- Upsert the rows next to `upsert_messages`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn codex_token_count_yields_estimated_sample() {
    // lines: session_meta, turn_context(model "gpt-5.5"), event_msg task_started @ 04:55:06.846Z,
    // event_msg token_count @ 04:55:24.102Z with info.last_token_usage.output_tokens=567, reasoning_output_tokens=33
    let (tmp, sessions) = codex_sessions_with(&[
        codex_meta(), codex_turn_context("gpt-5.5"),
        codex_event_at("task_started", "2026-09-20T04:55:06.846Z", serde_json::json!({})),
        codex_token_count_at("2026-09-20T04:55:24.102Z", 567, 33),
    ]);
    let mut s = test_store();
    index_codex(&mut s, &sessions, home());
    let rows = s.perf_samples(0).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].gen_ms, 17_256);
    assert_eq!(rows[0].output_tokens, 600);
    assert_eq!(rows[0].model, "gpt-5.5");
    drop(tmp);
}

#[test]
fn codex_second_token_count_without_new_request_is_skipped() {
    let (tmp, sessions) = codex_sessions_with(&[
        codex_meta(), codex_turn_context("gpt-5.5"),
        codex_event_at("task_started", "2026-09-20T04:55:06.846Z", serde_json::json!({})),
        codex_token_count_at("2026-09-20T04:55:24.102Z", 567, 0),
        codex_token_count_at("2026-09-20T04:55:28.355Z", 95, 0),
    ]);
    let mut s = test_store();
    index_codex(&mut s, &sessions, home());
    assert_eq!(s.perf_samples(0).unwrap().len(), 1);
    drop(tmp);
}
```

The helper functions (`codex_sessions_with`, `codex_meta`, `codex_turn_context`, `codex_event_at`, `codex_token_count_at`) build JSON lines in the shape the existing codex tests use, with `{"timestamp":..,"type":..,"payload":{..}}`. Write them to `<tmp>/sessions/2026/09/20/rollout-2026-09-20T11-55-06-test.jsonl`.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cd src-tauri/crates/collector && cargo test indexer::tests::codex_`
Expected: FAIL.

- [ ] **Step 3: Implement** the algorithm above.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cd src-tauri/crates/collector && cargo test`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/collector/src/indexer.rs
git commit -m "feat(collector): estimate codex response speed from session timestamps"
```

---

### Task 6: `perf_by_model` command and TS API

**Files:**
- Modify: `src-tauri/src/lib.rs`:
  - add the command near `history_by_model` (461-477)
  - register it in `invoke_handler` (1986-2028)
- Modify: `src/lib/api.ts` (near `historyByModel`, line 309)
- Test: `src-tauri/src/lib.rs` has no command tests. The contract is covered by Task 2, so this task is verified by `cargo test` plus `tsc`.

**Interfaces:**
- Consumes:
  - `collector::perf::{aggregate, PerfAgg}`
  - `Store::perf_samples`
  - the existing `since_ms(days)` helper (lib.rs:355)
  - `state.store.lock()`
- Produces:
  - Rust:
    ```rust
    #[tauri::command]
    fn perf_by_model(range: String, group_by_family: bool, state: State<'_, Arc<AppState>>) -> Result<Vec<PerfAgg>, String>
    ```
    `range` maps `"24h"` to `since_ms(1)`, `"30d"` to `since_ms(30)`, and anything else to `since_ms(7)`.
  - TS (`src/lib/api.ts`):
    ```ts
    export type PerfRange = "24h" | "7d" | "30d";
    export interface DailyTps { day: string; tpsP50: number }
    export interface PerfAgg {
      key: string;
      agent: Agent;
      models: string[];
      samples: number;
      tpsP50: number;
      tpsP10: number;
      ttftP50Ms: number | null;
      precise: boolean;
      daily: DailyTps[];
      children: PerfAgg[];
    }
    export const perfByModel = (range: PerfRange, groupByFamily: boolean) =>
      invoke<PerfAgg[]>("perf_by_model", { range, groupByFamily });
    ```
    Import `Agent` the same way the other api.ts types do.

- [ ] **Step 1: Implement the command**

```rust
#[tauri::command]
fn perf_by_model(
    range: String,
    group_by_family: bool,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<collector::perf::PerfAgg>, String> {
    let days = match range.as_str() {
        "24h" => 1,
        "30d" => 30,
        _ => 7,
    };
    let rows = state
        .store
        .lock()
        .map_err(|e| e.to_string())?
        .perf_samples(since_ms(days))
        .map_err(|e| e.to_string())?;
    Ok(collector::perf::aggregate(&rows, group_by_family))
}
```

Add `perf_by_model` to the `tauri::generate_handler![…]` list. Match how the neighbouring commands lock the store; if they use `.lock().unwrap()`, keep that style.

- [ ] **Step 2: Add the TS wrapper and types** shown above to `src/lib/api.ts`.

- [ ] **Step 3: Verify**

Run: `cd src-tauri && cargo test && cd .. && bunx tsc --noEmit`
Expected: PASS and no type errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs src/lib/api.ts
git commit -m "feat(app): expose per-model performance command"
```

---

### Task 7: Performa tab UI

**Files:**
- Create: `src/lib/perf.ts` (pure helpers) and `src/lib/perf.test.ts`
- Create: `src/components/PageTabs.tsx`. It is a generic segmented tab strip styled like `RangeFilter.tsx:19-33`, since the repo has no tab component yet.
- Create: `src/components/PerfTable.tsx`
- Create: `src/components/PerfChart.tsx`. Copy the container, grid and tooltip conventions from `src/components/TokenChart.tsx:49-55`, using `LineChart` instead of `BarChart`.
- Create: `src/components/PerfPanel.tsx`, which holds the range, toggle, search, table and chart for the tab.
- Modify: `src/pages/TokenPage.tsx`:
  - wrap the existing body (KPIs, `TokenChart` section, `ModelTable` section, lines ~63-108) as the "Biaya" tab
  - add "Performa", which renders `<PerfPanel />`
  - hide the header `RangeFilter` when the Performa tab is active, because it has its own range control

**Interfaces:**
- Consumes: `perfByModel`, `PerfAgg`, `PerfRange` (Task 6), `AgentIcon` from `@/components/BrandIcon`, `useQuery`.
- Produces (in `src/lib/perf.ts`):
  ```ts
  export type PerfSortKey = "tpsP50" | "tpsP10" | "ttftP50Ms" | "samples";
  export const LOW_SAMPLES = 5;
  export const ESTIMATE_HINT = "Perkiraan dari timestamp sesi, termasuk waktu tunggu token pertama.";
  export const formatTps: (v: number, precise: boolean) => string; // "41,2" | "~41,2" (id-ID, 1 decimal)
  export const formatTtft: (ms: number | null) => string;          // null -> "-", <1000 -> "850 ms", else "1,1 dtk"
  export const sortPerf: (rows: PerfAgg[], key: PerfSortKey, desc: boolean) => PerfAgg[]; // nulls last, stable
  export const filterPerf: (rows: PerfAgg[], q: string) => PerfAgg[]; // case-insensitive match on key or any models[]; family rows also match on children
  ```

- [ ] **Step 1: Write the failing tests** (`src/lib/perf.test.ts`)

```ts
import { describe, expect, it } from "bun:test";
import type { PerfAgg } from "@/lib/api";
import { filterPerf, formatTps, formatTtft, sortPerf } from "@/lib/perf";

const row = (key: string, tpsP50: number, ttftP50Ms: number | null, samples = 10, models = [key]): PerfAgg => ({
  key, agent: "opencode", models, samples, tpsP50, tpsP10: tpsP50 / 2, ttftP50Ms, precise: true, daily: [], children: [],
});

describe("formatTps", () => {
  it("formats with one decimal, id-ID", () => expect(formatTps(41.23, true)).toBe("41,2"));
  it("prefixes estimates with ~", () => expect(formatTps(41.23, false)).toBe("~41,2"));
});

describe("formatTtft", () => {
  it("dash when unknown", () => expect(formatTtft(null)).toBe("-"));
  it("ms under a second", () => expect(formatTtft(850)).toBe("850 ms"));
  it("seconds above", () => expect(formatTtft(1100)).toBe("1,1 dtk"));
});

describe("sortPerf", () => {
  it("sorts desc by tps", () => {
    const out = sortPerf([row("a", 10, 1), row("b", 40, 1)], "tpsP50", true);
    expect(out.map((r) => r.key)).toEqual(["b", "a"]);
  });
  it("puts null ttft last in both directions", () => {
    const rows = [row("a", 1, null), row("b", 1, 500), row("c", 1, 200)];
    expect(sortPerf(rows, "ttftP50Ms", false).map((r) => r.key)).toEqual(["c", "b", "a"]);
    expect(sortPerf(rows, "ttftP50Ms", true).map((r) => r.key)).toEqual(["b", "c", "a"]);
  });
});

describe("filterPerf", () => {
  it("matches key or models case-insensitively", () => {
    const rows = [row("deepseek-v4-1-flash", 1, null, 10, ["kn/deepseek-v4-1-flash", "Sumo/deepseek-v4.1-flash:netra"]), row("gpt-5.5", 1, null)];
    expect(filterPerf(rows, "SUMO").map((r) => r.key)).toEqual(["deepseek-v4-1-flash"]);
    expect(filterPerf(rows, "").length).toBe(2);
  });
});
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `bun test src/lib/perf.test.ts`
Expected: FAIL, because the module is missing.

- [ ] **Step 3: Implement `src/lib/perf.ts`**

```ts
import type { PerfAgg } from "@/lib/api";

export type PerfSortKey = "tpsP50" | "tpsP10" | "ttftP50Ms" | "samples";
export const LOW_SAMPLES = 5;
export const ESTIMATE_HINT = "Perkiraan dari timestamp sesi, termasuk waktu tunggu token pertama.";

const oneDecimal = new Intl.NumberFormat("id-ID", { minimumFractionDigits: 1, maximumFractionDigits: 1 });

export const formatTps = (v: number, precise: boolean): string => `${precise ? "" : "~"}${oneDecimal.format(v)}`;

export const formatTtft = (ms: number | null): string => {
  if (ms === null) return "-";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${oneDecimal.format(ms / 1000)} dtk`;
};

export const sortPerf = (rows: PerfAgg[], key: PerfSortKey, desc: boolean): PerfAgg[] =>
  rows
    .map((r, i) => ({ r, i }))
    .sort((a, b) => {
      const av = a.r[key];
      const bv = b.r[key];
      if (av === null && bv === null) return a.i - b.i;
      if (av === null) return 1;
      if (bv === null) return -1;
      const d = desc ? bv - av : av - bv;
      return d !== 0 ? d : a.i - b.i;
    })
    .map(({ r }) => r);

const matches = (r: PerfAgg, q: string): boolean =>
  r.key.toLowerCase().includes(q) ||
  r.models.some((m) => m.toLowerCase().includes(q)) ||
  r.children.some((c) => matches(c, q));

export const filterPerf = (rows: PerfAgg[], q: string): PerfAgg[] => {
  const needle = q.trim().toLowerCase();
  return needle === "" ? rows : rows.filter((r) => matches(r, needle));
};
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `bun test src/lib/perf.test.ts`
Expected: PASS.

- [ ] **Step 5: Build the components**

- **`PageTabs.tsx`:** `({ tabs, value, onChange }: { tabs: { id: string; label: string }[]; value: string; onChange: (id: string) => void })`. It uses the same container and button classes as `RangeFilter` (`flex items-center gap-1 rounded-lg border border-border bg-surface p-0.75`, and for the active button `bg-surface-2 text-fg`, otherwise `text-fg-3 hover:text-fg-2`, plus `ad-interactive ad-press`). Add `role="tablist"`, `role="tab"` and `aria-selected`.
- **`PerfPanel.tsx`:**
  - State: `range: PerfRange = "7d"`, `grouped = false`, `query = ""`, `sort: { key: PerfSortKey; desc: boolean } = { key: "tpsP50", desc: true }`, `selected: string[]`.
  - Data: `useQuery({ queryKey: ["perf", range, grouped], queryFn: () => perfByModel(range, grouped) })`.
  - Controls row:
    - `RangeFilter` with `options={[1, 7, 30]}` and `labels={{ 1: "24 jam", 7: "7 hari", 30: "30 hari" }}`. Map 1, 7 and 30 to `"24h"`, `"7d"` and `"30d"`.
    - A checkbox-style toggle button labelled "Kelompokkan per model".
    - A search `<input>` using the field class used elsewhere (`rounded-md border border-border bg-bg px-3 py-2 text-[13px]`).
  - Below the controls, `<PerfTable>` and then `<PerfChart>`.
  - Default selection: once data loads and `selected` is empty, set it to the keys of the top 3 rows by `samples`.
  - Selection rules: clicking a row toggles its key, with a maximum of 5 selected. Selecting a 6th row drops the oldest.
  - When there are no rows, show the empty-state text: "Belum ada data performa. Data terisi saat indexer membaca riwayat sesi."
  - Loading and error states follow `TokenPage` (`text-sm text-fg-3`).
- **`PerfTable.tsx`:**
  - Table classes: copy from `ModelTable.tsx` (`w-full border-collapse`, header `text-[11px] font-medium text-fg-3`, numeric cells `py-3 text-right font-mono text-[12.5px] text-fg-2`, and TPS p50 `font-semibold text-fg`).
  - Columns: Model (`AgentIcon` + key), TPS p50, TPS p10, TTFT p50, Sampel.
  - Numeric headers are buttons that call `onSort(key)`. The same key toggles the direction, and the active column shows `↓`/`↑`.
  - When the row is not `precise`, the TPS cells get `title={ESTIMATE_HINT}`.
  - When `samples < LOW_SAMPLES`, the row gets `opacity-50` and `title="Sampel sedikit"`.
  - Selected rows get `bg-surface-2`.
  - In grouped mode, each family row has a chevron that expands `children` as indented sub-rows (`pl-6`) using the same cells.
- **`PerfChart.tsx`:**
  - `ResponsiveContainer`, then `LineChart` with `data` merged by day: `{ day, [key]: tpsP50 }` for each selected row.
  - One `<Line type="monotone" dataKey={key} dot={false} strokeWidth={2} />` per selected key. Colors cycle through the agent color tokens or chart tokens already used in `TokenChart.tsx`; reuse its color source.
  - `CartesianGrid vertical={false} stroke="var(--color-border)" strokeDasharray="3 4"`, and the same axis and tooltip styling as `TokenChart`. Height `h-64`.
  - Put the merge logic in `src/lib/perf.ts` as `export const chartSeries = (rows: PerfAgg[], keys: string[]) => Array<Record<string, string | number>>` (sorted by day, with missing days left undefined) and unit-test it in `perf.test.ts`:

```ts
import { chartSeries } from "@/lib/perf";
describe("chartSeries", () => {
  it("merges daily points by day", () => {
    const a = { ...row("a", 1, null), daily: [{ day: "2026-09-21", tpsP50: 10 }, { day: "2026-09-22", tpsP50: 12 }] };
    const b = { ...row("b", 1, null), daily: [{ day: "2026-09-22", tpsP50: 30 }] };
    expect(chartSeries([a, b], ["a", "b"])).toEqual([
      { day: "2026-09-21", a: 10 },
      { day: "2026-09-22", a: 12, b: 30 },
    ]);
  });
});
```

```ts
export const chartSeries = (rows: PerfAgg[], keys: string[]): Array<Record<string, string | number>> => {
  const byDay = new Map<string, Record<string, string | number>>();
  for (const r of rows.filter((x) => keys.includes(x.key))) {
    for (const d of r.daily) {
      const point = byDay.get(d.day) ?? { day: d.day };
      point[r.key] = d.tpsP50;
      byDay.set(d.day, point);
    }
  }
  return [...byDay.values()].sort((x, y) => String(x.day).localeCompare(String(y.day)));
};
```

- **`TokenPage.tsx`:**
  - Add `const [tab, setTab] = useState<"biaya" | "performa">("biaya")`.
  - Render `<PageTabs tabs={[{ id: "biaya", label: "Biaya" }, { id: "performa", label: "Performa" }]} … />` in the header next to the title.
  - Show the existing `RangeFilter` only when `tab === "biaya"`.
  - Body: `tab === "biaya" ? <existing content unchanged> : <PerfPanel />`.

- [ ] **Step 6: Verify**

Run: `bun test && bunx tsc --noEmit && bun run build`
Expected: all PASS and the build succeeds.

- [ ] **Step 7: Commit**

```bash
git add src/lib/perf.ts src/lib/perf.test.ts src/components/PageTabs.tsx src/components/PerfTable.tsx src/components/PerfChart.tsx src/components/PerfPanel.tsx src/pages/TokenPage.tsx
git commit -m "feat(ui): add performa tab with tps ranking and trend chart"
```

---

## Final verification (after all tasks)

- `cd src-tauri/crates/collector && cargo test`
- `cd src-tauri && cargo test`
- `bun test`, then `bunx tsc --noEmit`, then `bun run build`
- `bun run tauri build --bundles app` must succeed.
- Manual check: open the Performa tab. `kn/deepseek-v4-1-flash` shows a precise (no `~`) TPS p50 above the ~40 tok/s end-to-end figure from the brainstorm query, and family mode groups `kn/`, `aki/cbai/` and `Sumo/` flash rows under `deepseek-v4-1-flash`.
