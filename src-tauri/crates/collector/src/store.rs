use crate::model::{Agent, TokenUsage};
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

fn agent_str(a: Agent) -> &'static str {
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn file_progress_round_trips_and_updates() {
        let tmp = tempfile::tempdir().unwrap();
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
}
