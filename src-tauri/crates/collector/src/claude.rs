use crate::model::Status;
use crate::process::ProcessTable;
use serde::Deserialize;
use std::path::Path;

const OBSERVER_MARKER: &str = "/.claude-mem/observer-sessions";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeLive {
    pub session_id: String,
    pub pid: u32,
    pub cwd: String,
    pub status: Status,
    pub updated_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub waiting_for: Option<String>,
    pub name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionFile {
    pid: u32,
    session_id: String,
    cwd: String,
    status: Option<String>,
    updated_at: Option<i64>,
    started_at: Option<i64>,
    waiting_for: Option<String>,
    name: Option<String>,
}

fn parse_status(s: Option<&str>) -> Status {
    match s {
        Some("busy") => Status::Busy,
        Some("waiting") => Status::Waiting,
        _ => Status::Idle,
    }
}

pub fn read_claude_sessions(dir: &Path, procs: &dyn ProcessTable) -> Vec<ClaudeLive> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    entries
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
        .filter_map(|e| {
            let raw = std::fs::read_to_string(e.path()).ok()?;
            let f: SessionFile = serde_json::from_str(&raw).ok()?;
            if f.cwd.contains(OBSERVER_MARKER) || !procs.is_alive(f.pid) {
                return None;
            }
            Some(ClaudeLive {
                session_id: f.session_id,
                pid: f.pid,
                cwd: f.cwd,
                status: parse_status(f.status.as_deref()),
                updated_at_ms: f.updated_at.unwrap_or(0),
                started_at_ms: f.started_at,
                waiting_for: f.waiting_for,
                name: f.name,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::FakeProcessTable;
    use std::fs;

    fn write(dir: &std::path::Path, name: &str, body: &str) {
        fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn keeps_live_sessions_and_filters_dead_observer_and_invalid() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        write(d, "100.json", r#"{"pid":100,"sessionId":"s1","cwd":"/Users/yolk/Dev/kirimi","status":"busy","updatedAt":1000,"startedAt":900,"name":"kirimi"}"#);
        write(d, "101.json", r#"{"pid":101,"sessionId":"s2","cwd":"/Users/yolk/Dev/noor","status":"idle","updatedAt":1}"#);
        write(d, "102.json", r#"{"pid":102,"sessionId":"s3","cwd":"/Users/yolk/.claude-mem/observer-sessions","status":"busy","updatedAt":1}"#);
        write(d, "103.json", "not json");
        write(d, "104.key", "secret");
        let mut procs = FakeProcessTable::default();
        procs.alive.extend([100, 102, 103]);

        let out = read_claude_sessions(d, &procs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].session_id, "s1");
        assert_eq!(out[0].pid, 100);
        assert_eq!(out[0].status, Status::Busy);
        assert_eq!(out[0].updated_at_ms, 1000);
        assert_eq!(out[0].started_at_ms, Some(900));
    }

    #[test]
    fn maps_waiting_and_unknown_status() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        write(d, "1.json", r#"{"pid":1,"sessionId":"a","cwd":"/x","status":"waiting","updatedAt":5,"waitingFor":"input needed"}"#);
        write(d, "2.json", r#"{"pid":2,"sessionId":"b","cwd":"/y","status":"weird","updatedAt":6}"#);
        let mut procs = FakeProcessTable::default();
        procs.alive.extend([1, 2]);
        let mut out = read_claude_sessions(d, &procs);
        out.sort_by_key(|s| s.pid);
        assert_eq!(out[0].status, Status::Waiting);
        assert_eq!(out[0].waiting_for.as_deref(), Some("input needed"));
        assert_eq!(out[1].status, Status::Idle);
    }

    #[test]
    fn missing_dir_returns_empty() {
        let procs = FakeProcessTable::default();
        assert!(read_claude_sessions(std::path::Path::new("/nonexistent/x"), &procs).is_empty());
    }
}
