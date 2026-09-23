use crate::billing::{split_cost, BillingMode, BillingTable};
use crate::model::{Agent, TokenUsage};
use crate::pricing::PriceTable;
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

const SCHEMA: &str = "
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
CREATE TABLE IF NOT EXISTS project (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  path TEXT NOT NULL UNIQUE,
  default_profile TEXT,
  color TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  last_used_ms INTEGER
);
CREATE TABLE IF NOT EXISTS setting (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
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
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRow {
    pub id: String,
    pub agent: Agent,
    pub session_id: String,
    pub project: String,
    pub model: String,
    pub ts_ms: i64,
    pub tokens: TokenUsage,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanRow {
    pub id: String,
    pub agent: Agent,
    pub session_id: String,
    pub project: String,
    pub model: Option<String>,
    pub tool: String,
    pub detail: Option<String>,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub status: String,
    pub tokens: Option<TokenUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileProgress {
    pub path: String,
    pub mtime_ms: i64,
    pub size: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyAgg {
    pub date: String,
    pub agent: Agent,
    pub tokens: TokenUsage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelAgg {
    pub model: String,
    pub agent: Agent,
    pub tokens: TokenUsage,
    pub messages: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub path: String,
    pub default_profile: Option<String>,
    pub color: Option<String>,
    pub sort_order: i64,
    pub last_used_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectAgg {
    pub project: String,
    pub tokens: TokenUsage,
    pub messages: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotalsAgg {
    pub tokens: TokenUsage,
    pub messages: i64,
}

/// One billing account's share of a window. `spend_usd` is money that moved and
/// `notional_usd` is what the same tokens would have cost at API rates; they are
/// equal for everything except a subscription, where spend is zero.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountAgg {
    pub account_id: String,
    pub label: String,
    pub mode: BillingMode,
    pub tokens: TokenUsage,
    pub spend_usd: f64,
    pub notional_usd: f64,
}

/// The label of the synthetic account that collects models no account claims.
/// These are paid per token by default, exactly as before billing existed.
pub const UNMATCHED_LABEL: &str = "Tanpa akun";

pub(crate) fn agent_str(a: Agent) -> &'static str {
    match a {
        Agent::Claude => "claude",
        Agent::Opencode => "opencode",
        Agent::Codex => "codex",
    }
}

fn agent_from_str(s: &str) -> Agent {
    match s {
        "opencode" => Agent::Opencode,
        "codex" => Agent::Codex,
        _ => Agent::Claude,
    }
}

pub struct Store {
    conn: Connection,
}

fn usage_from_row(r: &rusqlite::Row<'_>, base: usize) -> rusqlite::Result<TokenUsage> {
    let n = |i: usize| r.get::<_, i64>(base + i).map(|v| v.max(0) as u64);
    Ok(TokenUsage {
        input: n(0)?,
        output: n(1)?,
        cache_read: n(2)?,
        cache_write: n(3)?,
        reasoning: n(4)?,
    })
}

impl Store {
    pub fn open(path: &Path) -> Result<Store> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn upsert_messages(&mut self, rows: &[MessageRow]) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO message \
                 (id, agent, session_id, project, model, ts_ms, input, output, cache_read, cache_write, reasoning) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for r in rows {
                stmt.execute(rusqlite::params![
                    r.id,
                    agent_str(r.agent),
                    r.session_id,
                    r.project,
                    r.model,
                    r.ts_ms,
                    r.tokens.input as i64,
                    r.tokens.output as i64,
                    r.tokens.cache_read as i64,
                    r.tokens.cache_write as i64,
                    r.tokens.reasoning as i64,
                ])?;
            }
        }
        tx.commit()?;
        Ok(rows.len())
    }

    pub fn upsert_spans(&mut self, rows: &[SpanRow]) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO tool_span \
                 (id, agent, session_id, project, model, tool, detail, start_ms, end_ms, status, \
                  input, output, cache_read, cache_write, reasoning) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )?;
            for r in rows {
                let t = r.tokens.as_ref();
                stmt.execute(rusqlite::params![
                    r.id,
                    agent_str(r.agent),
                    r.session_id,
                    r.project,
                    r.model,
                    r.tool,
                    r.detail,
                    r.start_ms,
                    r.end_ms,
                    r.status,
                    t.map(|u| u.input as i64),
                    t.map(|u| u.output as i64),
                    t.map(|u| u.cache_read as i64),
                    t.map(|u| u.cache_write as i64),
                    t.map(|u| u.reasoning as i64),
                ])?;
            }
        }
        tx.commit()?;
        Ok(rows.len())
    }

    /// Upserts per-response performance samples. Merging keeps the earliest
    /// `start_ms` and latest `end_ms` seen for an id; estimated (non-`precise`)
    /// rows recompute `gen_ms` from the merged window, while precise rows (whole
    /// opencode rewrites) keep the incoming `gen_ms` as-is.
    pub fn upsert_perf(&mut self, rows: &[PerfRow]) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO response_perf (id, agent, session_id, model, start_ms, end_ms, gen_ms, output_tokens, ttft_ms, precise) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
                 ON CONFLICT(id) DO UPDATE SET \
                   start_ms = MIN(response_perf.start_ms, excluded.start_ms), \
                   end_ms = MAX(response_perf.end_ms, excluded.end_ms), \
                   output_tokens = excluded.output_tokens, \
                   ttft_ms = excluded.ttft_ms, \
                   gen_ms = CASE WHEN excluded.precise = 1 THEN excluded.gen_ms \
                                 ELSE MAX(response_perf.end_ms, excluded.end_ms) - MIN(response_perf.start_ms, excluded.start_ms) END, \
                   model = excluded.model, \
                   precise = excluded.precise",
            )?;
            for r in rows {
                stmt.execute(rusqlite::params![
                    r.id,
                    agent_str(r.agent),
                    r.session_id,
                    r.model,
                    r.start_ms,
                    r.end_ms,
                    r.gen_ms,
                    r.output_tokens,
                    r.ttft_ms,
                    r.precise as i64
                ])?;
            }
        }
        tx.commit()?;
        Ok(rows.len())
    }

    /// Perf samples whose `end_ms >= since_ms`, ordered by `end_ms`.
    pub fn perf_samples(&self, since_ms: i64) -> Result<Vec<PerfRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, agent, session_id, model, start_ms, end_ms, gen_ms, output_tokens, ttft_ms, precise \
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

    fn span_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SpanRow> {
        // All five token columns are written together, so any non-null one means
        // the span carries usage; a span without usage stores five NULLs.
        let tokens = if r.get::<_, Option<i64>>(10)?.is_some() {
            Some(usage_from_row(r, 10)?)
        } else {
            None
        };
        Ok(SpanRow {
            id: r.get(0)?,
            agent: agent_from_str(&r.get::<_, String>(1)?),
            session_id: r.get(2)?,
            project: r.get(3)?,
            model: r.get(4)?,
            tool: r.get(5)?,
            detail: r.get(6)?,
            start_ms: r.get(7)?,
            end_ms: r.get(8)?,
            status: r.get(9)?,
            tokens,
        })
    }

    const SPAN_COLS: &'static str = "id, agent, session_id, project, model, tool, detail, \
                                     start_ms, end_ms, status, \
                                     input, output, cache_read, cache_write, reasoning";

    /// Every span that OVERLAPS `[from_ms, to_ms)`. A span with no `end_ms` is
    /// still running and therefore overlaps everything from its start onwards.
    pub fn spans(&self, from_ms: i64, to_ms: i64) -> Result<Vec<SpanRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM tool_span \
             WHERE start_ms < ?1 AND (end_ms IS NULL OR end_ms > ?2) \
             ORDER BY session_id ASC, start_ms ASC",
            Self::SPAN_COLS
        ))?;
        let rows = stmt.query_map([to_ms, from_ms], Self::span_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn span_by_id(&self, id: &str) -> Result<Option<SpanRow>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {} FROM tool_span WHERE id = ?1", Self::SPAN_COLS))?;
        let mut rows = stmt.query_map([id], Self::span_from_row)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn file_progress(&self, path: &str) -> Option<FileProgress> {
        self.conn
            .query_row(
                "SELECT path, mtime_ms, size, offset FROM indexed_file WHERE path = ?1",
                [path],
                |r| {
                    Ok(FileProgress {
                        path: r.get(0)?,
                        mtime_ms: r.get(1)?,
                        size: r.get(2)?,
                        offset: r.get(3)?,
                    })
                },
            )
            .optional()
            .ok()
            .flatten()
    }

    pub fn set_file_progress(&mut self, p: &FileProgress) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO indexed_file (path, mtime_ms, size, offset) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![p.path, p.mtime_ms, p.size, p.offset],
        )?;
        Ok(())
    }

    /// Drops every row for a session. Used when a file shrank or was rewritten,
    /// so the index does not keep usage for records the file no longer contains.
    pub fn delete_session(&mut self, agent: Agent, session_id: &str) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM message WHERE agent = ?1 AND session_id = ?2",
            rusqlite::params![agent_str(agent), session_id],
        )?;
        self.conn.execute(
            "DELETE FROM response_perf WHERE agent = ?1 AND session_id = ?2",
            rusqlite::params![agent_str(agent), session_id],
        )?;
        Ok(n)
    }

    pub fn counts(&self) -> Result<(i64, i64)> {        let files = self.conn.query_row("SELECT COUNT(*) FROM indexed_file", [], |r| r.get(0))?;
        let messages = self.conn.query_row("SELECT COUNT(*) FROM message", [], |r| r.get(0))?;
        Ok((files, messages))
    }

    pub fn daily(&self, since_ms: i64) -> Result<Vec<DailyAgg>> {
        let mut stmt = self.conn.prepare(
            "SELECT strftime('%Y-%m-%d', ts_ms/1000, 'unixepoch', 'localtime') AS d, agent, \
                    SUM(input), SUM(output), SUM(cache_read), SUM(cache_write), SUM(reasoning) \
             FROM message WHERE ts_ms >= ?1 GROUP BY d, agent ORDER BY d ASC",
        )?;
        let rows = stmt.query_map([since_ms], |r| {
            Ok(DailyAgg {
                date: r.get(0)?,
                agent: agent_from_str(&r.get::<_, String>(1)?),
                tokens: usage_from_row(r, 2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn by_model(&self, since_ms: i64) -> Result<Vec<ModelAgg>> {
        let mut stmt = self.conn.prepare(
            "SELECT model, agent, SUM(input), SUM(output), SUM(cache_read), SUM(cache_write), \
                    SUM(reasoning), COUNT(*) \
             FROM message WHERE ts_ms >= ?1 GROUP BY model, agent \
             ORDER BY SUM(input + output + cache_read + cache_write) DESC, model ASC",
        )?;
        let rows = stmt.query_map([since_ms], |r| {
            Ok(ModelAgg {
                model: r.get(0)?,
                agent: agent_from_str(&r.get::<_, String>(1)?),
                tokens: usage_from_row(r, 2)?,
                messages: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Models this agent has actually been run with, most recently used first.
    pub fn distinct_models(&self, agent: Agent) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT model FROM message WHERE agent = ?1 AND model <> '' \
             GROUP BY model ORDER BY MAX(ts_ms) DESC, model ASC",
        )?;
        let rows = stmt.query_map([agent_str(agent)], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn by_project(&self, since_ms: i64) -> Result<Vec<ProjectAgg>> {
        let mut stmt = self.conn.prepare(
            "SELECT project, SUM(input), SUM(output), SUM(cache_read), SUM(cache_write), \
                    SUM(reasoning), COUNT(*) \
             FROM message WHERE ts_ms >= ?1 GROUP BY project ORDER BY SUM(input + output + cache_read + cache_write) DESC",
        )?;
        let rows = stmt.query_map([since_ms], |r| {
            Ok(ProjectAgg {
                project: r.get(0)?,
                tokens: usage_from_row(r, 1)?,
                messages: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Per-(local date, agent, model) buckets. The cost of a day/agent bucket is the
    /// sum of its per-model costs, which is why the caller needs model detail.
    pub fn daily_by_model(&self, since_ms: i64) -> Result<Vec<(String, Agent, String, TokenUsage)>> {
        let mut stmt = self.conn.prepare(
            "SELECT strftime('%Y-%m-%d', ts_ms/1000, 'unixepoch', 'localtime') AS d, agent, model, \
                    SUM(input), SUM(output), SUM(cache_read), SUM(cache_write), SUM(reasoning) \
             FROM message WHERE ts_ms >= ?1 GROUP BY d, agent, model",
        )?;
        let rows = stmt.query_map([since_ms], |r| {
            Ok((
                r.get::<_, String>(0)?,
                agent_from_str(&r.get::<_, String>(1)?),
                r.get::<_, String>(2)?,
                usage_from_row(r, 3)?,
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Per-(project, model) buckets, for the same cost reason as `daily_by_model`.
    pub fn by_project_model(&self, since_ms: i64) -> Result<Vec<(String, String, TokenUsage)>> {
        let mut stmt = self.conn.prepare(
            "SELECT project, model, SUM(input), SUM(output), SUM(cache_read), SUM(cache_write), SUM(reasoning) \
             FROM message WHERE ts_ms >= ?1 GROUP BY project, model",
        )?;
        let rows = stmt.query_map([since_ms], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                usage_from_row(r, 2)?,
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Per-billing-account totals for `[from_ms, to_ms)`, with spend and notional
    /// kept apart. Cost is summed per (model, account) rather than per account, so
    /// each model is priced at its own rate. Order follows the table, with the
    /// synthetic pay-as-you-go group last.
    pub fn by_account(
        &self,
        from_ms: i64,
        to_ms: i64,
        table: &BillingTable,
        prices: &PriceTable,
    ) -> Result<Vec<AccountAgg>> {
        let mut stmt = self.conn.prepare(
            "SELECT model, SUM(input), SUM(output), SUM(cache_read), SUM(cache_write), SUM(reasoning) \
             FROM message WHERE ts_ms >= ?1 AND ts_ms < ?2 GROUP BY model",
        )?;
        let rows = stmt.query_map([from_ms, to_ms], |r| {
            Ok((r.get::<_, String>(0)?, usage_from_row(r, 1)?))
        })?;

        let mut out: Vec<AccountAgg> = Vec::new();
        let mut unmatched: Option<AccountAgg> = None;
        let mut add = |id: &str, label: &str, mode: BillingMode, tokens: &TokenUsage, spend: f64, notional: f64| {
            match out.iter_mut().find(|a| a.account_id == id) {
                Some(a) => {
                    a.tokens.add(tokens);
                    a.spend_usd += spend;
                    a.notional_usd += notional;
                }
                None => out.push(AccountAgg {
                    account_id: id.to_string(),
                    label: label.to_string(),
                    mode,
                    tokens: tokens.clone(),
                    spend_usd: spend,
                    notional_usd: notional,
                }),
            }
        };

        for row in rows {
            let (model, tokens) = row?;
            let notional = prices.cost_usd(&model, &tokens);
            match table.match_account(&model) {
                Some(a) => {
                    let (spend, notional) = split_cost(Some(a), notional);
                    add(&a.id, &a.label, a.mode, &tokens, spend, notional);
                }
                None => {
                    let (spend, notional) = split_cost(None, notional);
                    let group = unmatched.get_or_insert_with(|| AccountAgg {
                        account_id: String::new(),
                        label: UNMATCHED_LABEL.to_string(),
                        mode: BillingMode::Payg,
                        tokens: TokenUsage::default(),
                        spend_usd: 0.0,
                        notional_usd: 0.0,
                    });
                    group.tokens.add(&tokens);
                    group.spend_usd += spend;
                    group.notional_usd += notional;
                }
            }
        }

        // Accounts the table declares but that saw no traffic still get a row, so
        // the monthly view can show a plan whose period had no messages.
        for a in table.accounts() {
            if !out.iter().any(|g| g.account_id == a.id) {
                out.push(AccountAgg {
                    account_id: a.id.clone(),
                    label: a.label.clone(),
                    mode: a.mode,
                    tokens: TokenUsage::default(),
                    spend_usd: 0.0,
                    notional_usd: 0.0,
                });
            }
        }

        if let Some(group) = unmatched {
            out.push(group);
        }
        Ok(out)
    }

    pub fn totals(&self, since_ms: i64) -> Result<TotalsAgg> {        self.conn.query_row(
            "SELECT COALESCE(SUM(input),0), COALESCE(SUM(output),0), COALESCE(SUM(cache_read),0), \
                    COALESCE(SUM(cache_write),0), COALESCE(SUM(reasoning),0), COUNT(*) \
             FROM message WHERE ts_ms >= ?1",
            [since_ms],
            |r| {
                Ok(TotalsAgg {
                    tokens: usage_from_row(r, 0)?,
                    messages: r.get(5)?,
                })
            },
        ).map_err(Into::into)
    }

    const PROJECT_COLS: &'static str =
        "id, name, path, default_profile, color, sort_order, last_used_ms";

    fn project_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRow> {
        Ok(ProjectRow {
            id: r.get(0)?,
            name: r.get(1)?,
            path: r.get(2)?,
            default_profile: r.get(3)?,
            color: r.get(4)?,
            sort_order: r.get(5)?,
            last_used_ms: r.get(6)?,
        })
    }

    pub fn projects(&self) -> Result<Vec<ProjectRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM project ORDER BY sort_order ASC, name ASC",
            Self::PROJECT_COLS
        ))?;
        let rows = stmt.query_map([], Self::project_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn project_by_path(&self, path: &str) -> Result<Option<ProjectRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM project WHERE path = ?1",
            Self::PROJECT_COLS
        ))?;
        let mut rows = stmt.query_map([path], Self::project_from_row)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn upsert_project(&mut self, row: &ProjectRow) -> Result<()> {
        // `path` is UNIQUE and `id` is the key. A plain `INSERT OR REPLACE` would
        // delete the row that owns a colliding path, so the caller would never see
        // the collision; an explicit conflict target keeps the unique constraint.
        self.conn.execute(
            "INSERT INTO project \
             (id, name, path, default_profile, color, sort_order, last_used_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT(id) DO UPDATE SET \
               name = excluded.name, path = excluded.path, \
               default_profile = excluded.default_profile, color = excluded.color, \
               sort_order = excluded.sort_order, last_used_ms = excluded.last_used_ms",
            rusqlite::params![
                row.id,
                row.name,
                row.path,
                row.default_profile,
                row.color,
                row.sort_order,
                row.last_used_ms,
            ],
        )?;
        Ok(())
    }

    pub fn delete_project(&mut self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM project WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn touch_project(&mut self, id: &str, now_ms: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE project SET last_used_ms = ?2 WHERE id = ?1",
            rusqlite::params![id, now_ms],
        )?;
        Ok(())
    }

    /// Distinct project NAMES seen in the message index with their message counts,
    /// busiest first. The index does not store the directory, so a caller that
    /// needs a path has to resolve the name itself.
    pub fn known_project_dirs(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT project, COUNT(*) AS n FROM message WHERE project <> '' \
             GROUP BY project ORDER BY n DESC, project ASC",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| {
                r.get::<_, String>(0)
            })
            .optional()?)
    }

    pub fn set_setting(&mut self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO setting (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub fn settings_all(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM setting")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::BillingAccount;

    fn row(id: &str, agent: Agent, model: &str, ts_ms: i64, u: TokenUsage) -> MessageRow {
        MessageRow {
            id: id.into(),
            agent,
            session_id: "s1".into(),
            project: "kirimi".into(),
            model: model.into(),
            ts_ms,
            tokens: u,
        }
    }

    fn usage(i: u64, o: u64, cr: u64, cw: u64) -> TokenUsage {
        TokenUsage { input: i, output: o, cache_read: cr, cache_write: cw, reasoning: 0 }
    }

    fn local_date(ts_ms: i64) -> String {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.query_row(
            "SELECT strftime('%Y-%m-%d', ?1/1000, 'unixepoch', 'localtime')",
            [ts_ms],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn local_ts_for(date: &str, hour: i64) -> i64 {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.query_row(
            "SELECT strftime('%s', ?1 || ' ' || ?2, 'utc')",
            rusqlite::params![date, format!("{hour:02}:00:00")],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
        .parse::<i64>()
        .unwrap()
            * 1000
    }

    #[test]
    fn open_creates_schema_and_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("agent-deck.db");
        let s1 = Store::open(&path).unwrap();
        assert_eq!(s1.counts().unwrap(), (0, 0));
        drop(s1);
        let s2 = Store::open(&path).unwrap();
        assert_eq!(s2.counts().unwrap(), (0, 0));
    }

    #[test]
    fn upsert_is_idempotent_by_id() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let r = row("claude:m1", Agent::Claude, "claude-sonnet-5", 1_000, usage(1, 2, 3, 4));
        s.upsert_messages(&[r.clone()]).unwrap();
        s.upsert_messages(&[r]).unwrap();
        assert_eq!(s.counts().unwrap().1, 1);
        let t = s.totals(0).unwrap();
        assert_eq!(t.messages, 1);
        assert_eq!(t.tokens, usage(1, 2, 3, 4));
    }

    #[test]
    fn upsert_replaces_usage_for_same_id() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        s.upsert_messages(&[row("x", Agent::Claude, "m", 1, usage(0, 10, 0, 0))]).unwrap();
        s.upsert_messages(&[row("x", Agent::Claude, "m", 1, usage(0, 50, 0, 0))]).unwrap();
        let t = s.totals(0).unwrap();
        assert_eq!(t.messages, 1);
        assert_eq!(t.tokens.output, 50);
    }

    #[test]
    fn distinct_models_are_scoped_to_the_agent_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        s.upsert_messages(&[
            row("claude:a", Agent::Claude, "claude-old", 1_000, usage(1, 1, 0, 0)),
            row("claude:b", Agent::Claude, "claude-new", 9_000, usage(1, 1, 0, 0)),
            row("claude:c", Agent::Claude, "claude-new", 10_000, usage(1, 1, 0, 0)),
            row("codex:a", Agent::Codex, "gpt-5", 20_000, usage(1, 1, 0, 0)),
        ])
        .unwrap();

        assert_eq!(s.distinct_models(Agent::Claude).unwrap(), vec!["claude-new", "claude-old"]);
        assert_eq!(s.distinct_models(Agent::Codex).unwrap(), vec!["gpt-5"]);
        assert!(s.distinct_models(Agent::Opencode).unwrap().is_empty());
    }

    #[test]
    fn daily_groups_by_local_date_and_agent() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let day_a = local_ts_for("2026-03-10", 10);
        let day_b = local_ts_for("2026-03-11", 10);
        s.upsert_messages(&[
            row("claude:a", Agent::Claude, "m", day_a, usage(1, 1, 0, 0)),
            row("opencode:a", Agent::Opencode, "m", day_a + 3_600_000, usage(10, 0, 0, 0)),
            row("codex:a", Agent::Codex, "m", day_b, usage(100, 0, 0, 0)),
        ])
        .unwrap();

        let rows = s.daily(0).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].date, "2026-03-10");
        assert_eq!(rows[0].agent, Agent::Claude);
        assert_eq!(rows[0].tokens.input, 1);
        assert_eq!(rows[1].date, "2026-03-10");
        assert_eq!(rows[1].agent, Agent::Opencode);
        assert_eq!(rows[1].tokens.input, 10);
        assert_eq!(rows[2].date, "2026-03-11");
        assert_eq!(rows[2].agent, Agent::Codex);
        assert_eq!(rows[2].tokens.input, 100);
        assert_eq!(rows[0].date, local_date(day_a));
    }

    #[test]
    fn by_model_and_by_project_aggregate_and_count() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let mut a = row("claude:a", Agent::Claude, "claude-sonnet-5", 100, usage(1, 2, 3, 4));
        a.project = "kirimi".into();
        let mut b = row("claude:b", Agent::Claude, "claude-sonnet-5", 200, usage(10, 20, 30, 40));
        b.project = "kirimi".into();
        let mut c = row("opencode:c", Agent::Opencode, "kn/deepseek-v4-1-flash", 300, usage(1000, 0, 0, 0));
        c.project = "submo".into();
        s.upsert_messages(&[a, b, c]).unwrap();

        let models = s.by_model(0).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].model, "kn/deepseek-v4-1-flash");
        assert_eq!(models[0].tokens.input, 1000);
        assert_eq!(models[0].messages, 1);
        assert_eq!(models[1].model, "claude-sonnet-5");
        assert_eq!(models[1].tokens, usage(11, 22, 33, 44));
        assert_eq!(models[1].messages, 2);

        let projects = s.by_project(0).unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].project, "submo");
        assert_eq!(projects[0].messages, 1);
        assert_eq!(projects[1].project, "kirimi");
        assert_eq!(projects[1].tokens, usage(11, 22, 33, 44));
        assert_eq!(projects[1].messages, 2);
    }

    #[test]
    fn totals_respects_since_ms() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        s.upsert_messages(&[
            row("old", Agent::Claude, "m", 1_000, usage(1, 0, 0, 0)),
            row("new", Agent::Claude, "m", 5_000, usage(7, 0, 0, 0)),
        ])
        .unwrap();
        let t = s.totals(2_000).unwrap();
        assert_eq!(t.messages, 1);
        assert_eq!(t.tokens.input, 7);
        let all = s.totals(0).unwrap();
        assert_eq!(all.messages, 2);
        assert_eq!(all.tokens.input, 8);
    }

    fn span(id: &str, start_ms: i64, end_ms: Option<i64>) -> SpanRow {
        SpanRow {
            id: id.into(),
            agent: Agent::Claude,
            session_id: "s1".into(),
            project: "kirimi".into(),
            model: Some("claude-sonnet-5".into()),
            tool: "Bash".into(),
            detail: Some("bun test".into()),
            start_ms,
            end_ms,
            status: "ok".into(),
            tokens: None,
        }
    }

    #[test]
    fn spans_upsert_is_idempotent_by_id() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let r = span("claude:t1", 1_000, Some(2_000));
        assert_eq!(s.upsert_spans(&[r.clone()]).unwrap(), 1);
        s.upsert_spans(&[r]).unwrap();
        assert_eq!(s.spans(0, 10_000).unwrap().len(), 1);
    }

    #[test]
    fn spans_returns_overlapping_only() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        s.upsert_spans(&[
            span("before", 100, Some(200)),
            span("after", 9_000, Some(9_500)),
            span("straddles-start", 900, Some(1_100)),
            span("inside", 1_500, Some(1_600)),
        ])
        .unwrap();
        let ids: Vec<String> = s.spans(1_000, 5_000).unwrap().into_iter().map(|r| r.id).collect();
        assert_eq!(ids, vec!["straddles-start".to_string(), "inside".to_string()]);
    }

    #[test]
    fn spans_with_null_end_are_treated_as_open() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        s.upsert_spans(&[span("open", 500, None)]).unwrap();
        let rows = s.spans(1_000, 5_000).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "open");
        assert_eq!(rows[0].end_ms, None);
    }

    #[test]
    fn spans_round_trip_tokens_and_status() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let mut failed = span("err", 1_000, Some(2_000));
        failed.status = "error".into();
        failed.tokens = Some(usage(1, 2, 3, 4));
        failed.model = None;
        failed.detail = None;
        s.upsert_spans(&[failed.clone(), span("bare", 3_000, Some(3_100))]).unwrap();

        let rows = s.spans(0, 10_000).unwrap();
        let err = rows.iter().find(|r| r.id == "err").unwrap();
        assert_eq!(err, &failed);
        assert_eq!(err.status, "error");
        assert_eq!(err.tokens, Some(usage(1, 2, 3, 4)));
        let bare = rows.iter().find(|r| r.id == "bare").unwrap();
        assert_eq!(bare.tokens, None);
    }

    #[test]
    fn file_progress_round_trips_and_updates() {        let tmp = tempfile::tempdir().unwrap();
        let mut s = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        assert!(s.file_progress("/x/a.jsonl").is_none());
        s.set_file_progress(&FileProgress {
            path: "/x/a.jsonl".into(),
            mtime_ms: 10,
            size: 100,
            offset: 20,
        })
        .unwrap();
        let p = s.file_progress("/x/a.jsonl").unwrap();
        assert_eq!(p.path, "/x/a.jsonl");
        assert_eq!(p.mtime_ms, 10);
        assert_eq!(p.size, 100);
        assert_eq!(p.offset, 20);

        s.set_file_progress(&FileProgress {
            path: "/x/a.jsonl".into(),
            mtime_ms: 30,
            size: 200,
            offset: 150,
        })
        .unwrap();
        let p = s.file_progress("/x/a.jsonl").unwrap();
        assert_eq!(p.offset, 150);
        assert_eq!(p.size, 200);
        assert_eq!(s.counts().unwrap().0, 1);
    }

    fn project(id: &str, name: &str, path: &str, sort_order: i64) -> ProjectRow {
        ProjectRow {
            id: id.into(),
            name: name.into(),
            path: path.into(),
            default_profile: None,
            color: None,
            sort_order,
            last_used_ms: None,
        }
    }

    fn open_store() -> (tempfile::TempDir, Store) {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        (tmp, store)
    }

    fn test_store() -> Store {
        Store::open(&tempfile::tempdir().unwrap().into_path().join("t.db")).unwrap()
    }

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
    fn upsert_perf_merge_updates_precise_flag() {
        let mut s = test_store();
        let mut estimated = perf("claude:a", 1_000, 2_000, 1_000, 50);
        estimated.precise = false;
        s.upsert_perf(&[estimated]).unwrap();

        let mut precise = perf("claude:a", 1_000, 2_000, 1_000, 50);
        precise.precise = true;
        s.upsert_perf(&[precise]).unwrap();

        let rows = s.perf_samples(0).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].precise, "merged row must reflect the latest precise flag");
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

    #[test]
    fn projects_round_trip_and_order() {
        let (_tmp, mut s) = open_store();
        s.upsert_project(&project("c", "zulu", "/x/c", 5)).unwrap();
        s.upsert_project(&project("a", "beta", "/x/a", 1)).unwrap();
        s.upsert_project(&project("b", "alpha", "/x/b", 1)).unwrap();

        let rows = s.projects().unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["b", "a", "c"]);
        assert_eq!(rows[0].name, "alpha");
        assert_eq!(rows[2].path, "/x/c");
    }

    #[test]
    fn upsert_project_updates_in_place() {
        let (_tmp, mut s) = open_store();
        s.upsert_project(&project("a", "before", "/x/a", 0)).unwrap();
        s.upsert_project(&project("a", "after", "/x/a", 0)).unwrap();
        let rows = s.projects().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "after");
    }

    #[test]
    fn project_path_is_unique() {
        let (_tmp, mut s) = open_store();
        s.upsert_project(&project("a", "first", "/x/shared", 0)).unwrap();
        let err = s
            .upsert_project(&project("b", "second", "/x/shared", 1))
            .unwrap_err();
        assert!(!err.to_string().is_empty());
        assert_eq!(s.projects().unwrap().len(), 1);
    }

    #[test]
    fn delete_project_removes_only_that_row() {
        let (_tmp, mut s) = open_store();
        s.upsert_project(&project("a", "one", "/x/a", 0)).unwrap();
        s.upsert_project(&project("b", "two", "/x/b", 1)).unwrap();
        s.delete_project("a").unwrap();
        let rows = s.projects().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "b");
    }

    #[test]
    fn touch_project_sets_last_used() {
        let (_tmp, mut s) = open_store();
        s.upsert_project(&project("a", "one", "/x/a", 0)).unwrap();
        assert_eq!(s.projects().unwrap()[0].last_used_ms, None);
        s.touch_project("a", 1_700_000_000_000).unwrap();
        assert_eq!(s.projects().unwrap()[0].last_used_ms, Some(1_700_000_000_000));
    }

    #[test]
    fn known_project_dirs_counts_messages_descending() {
        let (_tmp, mut s) = open_store();
        let mut a = row("claude:a", Agent::Claude, "m", 1, usage(1, 0, 0, 0));
        a.project = "quiet".into();
        let mut b = row("claude:b", Agent::Claude, "m", 2, usage(1, 0, 0, 0));
        b.project = "busy".into();
        let mut c = row("claude:c", Agent::Claude, "m", 3, usage(1, 0, 0, 0));
        c.project = "busy".into();
        s.upsert_messages(&[a, b, c]).unwrap();

        let dirs = s.known_project_dirs().unwrap();
        assert_eq!(
            dirs,
            vec![("busy".to_string(), 2), ("quiet".to_string(), 1)]
        );
    }

    #[test]
    fn setting_round_trips_and_upserts() {
        let (_tmp, mut s) = open_store();
        s.set_setting("a", "1").unwrap();
        assert_eq!(s.setting("a").unwrap(), Some("1".to_string()));
        s.set_setting("a", "2").unwrap();
        assert_eq!(s.setting("a").unwrap(), Some("2".to_string()));
        assert_eq!(s.settings_all().unwrap().len(), 1);
    }

    #[test]
    fn missing_setting_is_none() {
        let (_tmp, s) = open_store();
        assert_eq!(s.setting("nope").unwrap(), None);
    }

    fn account(id: &str, label: &str, mode: BillingMode, matches: &[&str]) -> BillingAccount {
        BillingAccount {
            id: id.into(),
            label: label.into(),
            mode,
            matches: matches.iter().map(|s| s.to_string()).collect(),
            monthly_usd: (mode == BillingMode::Subscription).then_some(200.0),
            renewal_day: (mode == BillingMode::Subscription).then_some(14),
            credit_usd: (mode == BillingMode::Prepaid).then_some(20.0),
            started_on: (mode == BillingMode::Prepaid).then(|| "2026-03-01".to_string()),
            expires_on: None,
        }
    }

    #[test]
    fn by_account_groups_messages_and_sums_both_numbers() {
        let (_tmp, mut s) = open_store();
        // 1M output on claude-sonnet-5 costs $15; 1M on kn/deepseek costs $1.10.
        s.upsert_messages(&[
            row("claude:a", Agent::Claude, "claude-sonnet-5", 100, usage(0, 1_000_000, 0, 0)),
            row("claude:b", Agent::Claude, "claude-sonnet-5", 200, usage(0, 1_000_000, 0, 0)),
            row("oc:a", Agent::Opencode, "kn/deepseek-v4-1-flash", 300, usage(0, 1_000_000, 0, 0)),
        ])
        .unwrap();
        let table = BillingTable::new(vec![
            account("sub", "Claude Max", BillingMode::Subscription, &["claude-"]),
            account("pre", "Kenari topup", BillingMode::Prepaid, &["kn/"]),
        ]);

        let groups = s.by_account(0, 10_000, &table, &PriceTable::defaults()).unwrap();
        assert_eq!(groups.len(), 2);

        let sub = groups.iter().find(|g| g.account_id == "sub").unwrap();
        assert_eq!(sub.tokens.output, 2_000_000);
        assert_eq!(sub.spend_usd, 0.0, "subscription usage never reaches spend");
        assert!((sub.notional_usd - 30.0).abs() < 1e-9);

        let pre = groups.iter().find(|g| g.account_id == "pre").unwrap();
        assert_eq!(pre.tokens.output, 1_000_000);
        assert!((pre.spend_usd - 1.10).abs() < 1e-9);
        assert!((pre.notional_usd - 1.10).abs() < 1e-9);
    }

    #[test]
    fn by_account_puts_unmatched_models_in_the_fallback_group() {
        let (_tmp, mut s) = open_store();
        s.upsert_messages(&[
            row("claude:a", Agent::Claude, "claude-sonnet-5", 100, usage(0, 1_000_000, 0, 0)),
            row("codex:a", Agent::Codex, "gpt-5.4", 200, usage(0, 1_000_000, 0, 0)),
            row("weird", Agent::Claude, "totally-unknown", 300, usage(0, 1_000_000, 0, 0)),
        ])
        .unwrap();
        let table = BillingTable::new(vec![account(
            "sub",
            "Claude Max",
            BillingMode::Subscription,
            &["claude-"],
        )]);

        let groups = s.by_account(0, 10_000, &table, &PriceTable::defaults()).unwrap();
        let fallback = groups
            .iter()
            .find(|g| g.account_id.is_empty())
            .expect("a fallback group");
        assert_eq!(fallback.label, UNMATCHED_LABEL);
        assert_eq!(fallback.mode, BillingMode::Payg);
        assert_eq!(fallback.tokens.output, 2_000_000);
        // gpt-5.4 output is $10/M; the unknown model is not priced at all.
        assert!((fallback.spend_usd - 10.0).abs() < 1e-9);
        assert!((fallback.notional_usd - 10.0).abs() < 1e-9);
        assert_eq!(groups.iter().find(|g| g.account_id == "sub").unwrap().tokens.output, 1_000_000);
    }

    #[test]
    fn by_account_respects_the_time_window() {
        let (_tmp, mut s) = open_store();
        s.upsert_messages(&[
            row("old", Agent::Claude, "claude-sonnet-5", 100, usage(0, 1_000_000, 0, 0)),
            row("in", Agent::Claude, "claude-sonnet-5", 5_000, usage(0, 1_000_000, 0, 0)),
            row("edge", Agent::Claude, "claude-sonnet-5", 9_000, usage(0, 1_000_000, 0, 0)),
        ])
        .unwrap();
        let table = BillingTable::new(vec![account(
            "sub",
            "Claude Max",
            BillingMode::Subscription,
            &["claude-"],
        )]);

        let groups = s.by_account(1_000, 9_000, &table, &PriceTable::defaults()).unwrap();
        let sub = groups.iter().find(|g| g.account_id == "sub").unwrap();
        assert_eq!(sub.tokens.output, 1_000_000, "from is inclusive, to is exclusive");
        assert!(sub.notional_usd > 0.0);

        let all = s.by_account(0, 10_000, &table, &PriceTable::defaults()).unwrap();
        assert_eq!(
            all.iter().find(|g| g.account_id == "sub").unwrap().tokens.output,
            3_000_000
        );
    }
}
