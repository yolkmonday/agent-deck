use crate::model::{Status, TokenUsage};
use crate::process::ProcessTable;
use anyhow::Result;
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

const RECENT_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
const BUSY_WINDOW_MS: i64 = 15_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpencodeLive {
    pub id: String,
    pub directory: String,
    pub title: Option<String>,
    pub model: Option<String>,
    pub tokens: TokenUsage,
    pub status: Status,
    pub running_tool: Option<String>,
    pub pid: u32,
    pub updated_at_ms: i64,
}

pub fn running_dirs(procs: &dyn ProcessTable) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    for p in procs.list() {
        let mut parts = p.command.split_whitespace();
        let Some(exe) = parts.next() else { continue };
        if exe.rsplit('/').next() != Some("opencode") {
            continue;
        }
        let args: Vec<&str> = parts.collect();
        let dir = args
            .iter()
            .position(|a| *a == "--dir")
            .and_then(|i| args.get(i + 1))
            .map(|s| s.to_string())
            .or_else(|| procs.cwd(p.pid));
        if let Some(d) = dir {
            out.entry(d).or_insert(p.pid);
        }
    }
    out
}

fn model_label(raw: Option<String>) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(&raw?).ok()?;
    let id = v.get("id")?.as_str()?;
    match v.get("providerID").and_then(|p| p.as_str()) {
        Some(p) => Some(format!("{p}/{id}")),
        None => Some(id.to_string()),
    }
}

fn running_tool(conn: &Connection, session_id: &str) -> Option<String> {
    conn.query_row(
        "SELECT json_extract(data,'$.tool') FROM part \
         WHERE session_id = ?1 AND json_extract(data,'$.type') = 'tool' \
         AND json_extract(data,'$.state.status') = 'running' \
         ORDER BY time_created DESC LIMIT 1",
        [session_id],
        |r| r.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
}

pub fn read_active(db: &Path, procs: &dyn ProcessTable, now_ms: i64) -> Result<Vec<OpencodeLive>> {
    let dirs = running_dirs(procs);
    if dirs.is_empty() || !db.exists() {
        return Ok(vec![]);
    }
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_millis(500))?;
    let mut stmt = conn.prepare(
        "SELECT id, directory, title, model, tokens_input, tokens_output, tokens_reasoning, \
                tokens_cache_read, tokens_cache_write, time_updated \
         FROM session \
         WHERE time_archived IS NULL AND parent_id IS NULL AND time_updated >= ?1 \
         ORDER BY time_updated DESC LIMIT 200",
    )?;
    let rows = stmt.query_map([now_ms - RECENT_WINDOW_MS], |r| {
        let n = |i: usize| r.get::<_, Option<i64>>(i).map(|v| v.unwrap_or(0).max(0) as u64);
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            TokenUsage {
                input: n(4)?,
                output: n(5)?,
                reasoning: n(6)?,
                cache_read: n(7)?,
                cache_write: n(8)?,
            },
            r.get::<_, i64>(9)?,
        ))
    })?;

    let mut seen_dirs = std::collections::HashSet::new();
    let mut out = Vec::new();
    for row in rows.flatten() {
        let (id, directory, title, model, tokens, updated) = row;
        let Some(pid) = dirs.get(&directory).copied() else { continue };
        if !seen_dirs.insert(directory.clone()) {
            continue;
        }
        let tool = running_tool(&conn, &id);
        let status = if tool.is_some() || now_ms - updated < BUSY_WINDOW_MS {
            Status::Busy
        } else {
            Status::Idle
        };
        out.push(OpencodeLive {
            id,
            directory,
            title,
            model: model_label(model),
            tokens,
            status,
            running_tool: tool,
            pid,
            updated_at_ms: updated,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{FakeProcessTable, ProcInfo};
    use rusqlite::Connection;

    fn fixture(dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("opencode.db");
        let c = Connection::open(&path).unwrap();
        c.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, project_id TEXT, parent_id TEXT, directory TEXT, title TEXT, agent TEXT, model TEXT, cost REAL, tokens_input INTEGER, tokens_output INTEGER, tokens_reasoning INTEGER, tokens_cache_read INTEGER, tokens_cache_write INTEGER, time_created INTEGER, time_updated INTEGER, time_archived INTEGER);
             CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
        )
        .unwrap();
        path
    }

    fn insert_session(c: &Connection, id: &str, dir: &str, parent: Option<&str>, updated: i64, archived: Option<i64>) {
        c.execute(
            "INSERT INTO session VALUES (?1,'p',?2,?3,'title','build','{\"id\":\"deepseek-v4-1-flash\",\"providerID\":\"kn\"}',0,10,20,5,100,50,1,?4,?5)",
            rusqlite::params![id, parent, dir, updated, archived],
        )
        .unwrap();
    }

    fn procs_with(dir: &str) -> FakeProcessTable {
        let mut p = FakeProcessTable::default();
        p.procs.push(ProcInfo {
            pid: 200,
            command: format!("/Users/yolk/.opencode/bin/opencode run task -m kn/x --dir {dir}"),
        });
        p
    }

    #[test]
    fn running_dirs_uses_dir_flag_then_cwd() {
        let mut p = FakeProcessTable::default();
        p.procs.push(ProcInfo { pid: 1, command: "/x/opencode run t --dir /a".into() });
        p.procs.push(ProcInfo { pid: 2, command: "opencode".into() });
        p.procs.push(ProcInfo { pid: 3, command: "/Applications/OpenCode.app/Contents/MacOS/OpenCode".into() });
        p.cwds.insert(2, "/b".into());
        let dirs = running_dirs(&p);
        assert_eq!(dirs.get("/a"), Some(&1));
        assert_eq!(dirs.get("/b"), Some(&2));
        assert_eq!(dirs.len(), 2);
    }

    #[test]
    fn returns_only_sessions_with_a_running_process() {
        let tmp = tempfile::tempdir().unwrap();
        let db = fixture(tmp.path());
        let c = Connection::open(&db).unwrap();
        insert_session(&c, "sa", "/a", None, 990_000, None);
        insert_session(&c, "sb", "/b", None, 990_000, None);
        insert_session(&c, "sc", "/a", Some("sa"), 990_000, None);
        insert_session(&c, "sd", "/a", None, 900_000, Some(950_000));
        c.execute(
            "INSERT INTO part VALUES ('p1','m','sa',995000,'{\"type\":\"tool\",\"tool\":\"bash\",\"state\":{\"status\":\"running\"}}')",
            [],
        )
        .unwrap();

        let out = read_active(&db, &procs_with("/a"), 1_000_000).unwrap();
        assert_eq!(out.len(), 1);
        let s = &out[0];
        assert_eq!(s.id, "sa");
        assert_eq!(s.pid, 200);
        assert_eq!(s.model.as_deref(), Some("kn/deepseek-v4-1-flash"));
        assert_eq!(s.tokens.input, 10);
        assert_eq!(s.tokens.output, 20);
        assert_eq!(s.tokens.reasoning, 5);
        assert_eq!(s.tokens.cache_read, 100);
        assert_eq!(s.tokens.cache_write, 50);
        assert_eq!(s.running_tool.as_deref(), Some("bash"));
        assert_eq!(s.status, Status::Busy);
    }

    #[test]
    fn idle_when_no_running_tool_and_not_recently_updated() {
        let tmp = tempfile::tempdir().unwrap();
        let db = fixture(tmp.path());
        let c = Connection::open(&db).unwrap();
        insert_session(&c, "sa", "/a", None, 900_000, None);
        let out = read_active(&db, &procs_with("/a"), 1_000_000).unwrap();
        assert_eq!(out[0].status, Status::Idle);
        assert!(out[0].running_tool.is_none());
    }

    #[test]
    fn no_process_means_db_is_not_opened() {
        let procs = FakeProcessTable::default();
        let out = read_active(std::path::Path::new("/nonexistent/db"), &procs, 0).unwrap();
        assert!(out.is_empty());
    }
}
