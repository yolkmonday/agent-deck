use crate::claude::{read_claude_sessions, ClaudeLive};
use crate::model::{Activity, ActivityKind, Agent, LiveSnapshot, Session, Status};
use crate::opencode::{read_active, OpencodeLive};
use crate::process::ProcessTable;
use crate::transcript::{find_transcript, TranscriptState};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

pub struct Paths {
    pub claude_sessions: PathBuf,
    pub claude_projects: PathBuf,
    pub opencode_db: PathBuf,
    pub home: String,
}

impl Paths {
    pub fn for_home(home: &str) -> Paths {
        Paths {
            claude_sessions: PathBuf::from(format!("{home}/.claude/sessions")),
            claude_projects: PathBuf::from(format!("{home}/.claude/projects")),
            opencode_db: PathBuf::from(format!("{home}/.local/share/opencode/opencode.db")),
            home: home.to_string(),
        }
    }
}

pub fn project_name(cwd: &str, home: &str) -> String {
    let trimmed = cwd.trim_end_matches('/');
    if trimmed == home.trim_end_matches('/') {
        return "~".to_string();
    }
    trimmed
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(cwd)
        .to_string()
}

pub struct LiveCollector {
    paths: Paths,
    procs: Arc<dyn ProcessTable>,
    transcripts: HashMap<String, (PathBuf, TranscriptState)>,
}

fn claude_activity(c: &ClaudeLive, st: &TranscriptState) -> Option<Activity> {
    if c.status == Status::Waiting {
        return Some(Activity {
            kind: ActivityKind::Waiting,
            label: "waiting".into(),
            detail: c.waiting_for.clone(),
        });
    }
    if st.pending_tool_id.is_some() {
        return st.last_tool.as_ref().map(|t| Activity {
            kind: ActivityKind::Tool,
            label: t.name.clone(),
            detail: t.detail.clone(),
        });
    }
    if st.turn_ended {
        return Some(Activity { kind: ActivityKind::Done, label: "done".into(), detail: None });
    }
    (c.status == Status::Busy).then(|| Activity {
        kind: ActivityKind::Thinking,
        label: "thinking".into(),
        detail: None,
    })
}

fn opencode_session(o: OpencodeLive, home: &str) -> Session {
    let activity = match (&o.running_tool, o.status) {
        (Some(t), _) => Some(Activity { kind: ActivityKind::Tool, label: t.clone(), detail: None }),
        (None, Status::Busy) => Some(Activity {
            kind: ActivityKind::Thinking,
            label: "thinking".into(),
            detail: None,
        }),
        _ => None,
    };
    Session {
        id: o.id,
        agent: Agent::Opencode,
        pid: Some(o.pid),
        project: project_name(&o.directory, home),
        cwd: o.directory,
        model: o.model,
        branch: None,
        status: o.status,
        activity,
        tokens: o.tokens,
        started_at_ms: None,
        updated_at_ms: o.updated_at_ms,
    }
}

impl LiveCollector {
    pub fn new(paths: Paths, procs: Arc<dyn ProcessTable>) -> Self {
        Self { paths, procs, transcripts: HashMap::new() }
    }

    fn claude_session(&mut self, c: ClaudeLive) -> Session {
        let projects = self.paths.claude_projects.clone();
        let (path, state) = self
            .transcripts
            .entry(c.session_id.clone())
            .or_insert_with(|| (PathBuf::new(), TranscriptState::default()));
        if path.as_os_str().is_empty() {
            if let Some(p) = find_transcript(&projects, &c.cwd, &c.session_id) {
                *path = p;
            }
        }
        if !path.as_os_str().is_empty() {
            let _ = state.advance(path);
        }
        Session {
            id: c.session_id.clone(),
            agent: Agent::Claude,
            pid: Some(c.pid),
            project: project_name(&c.cwd, &self.paths.home),
            cwd: c.cwd.clone(),
            model: state.model.clone(),
            branch: state.branch.clone(),
            status: c.status,
            activity: claude_activity(&c, state),
            tokens: state.tokens.clone(),
            started_at_ms: c.started_at_ms,
            updated_at_ms: c.updated_at_ms,
        }
    }

    pub fn snapshot(&mut self, now_ms: i64) -> LiveSnapshot {
        let mut sessions = Vec::new();
        let mut warnings = Vec::new();

        let claude = read_claude_sessions(&self.paths.claude_sessions, self.procs.as_ref());
        let live_ids: HashSet<String> = claude.iter().map(|c| c.session_id.clone()).collect();
        self.transcripts.retain(|id, _| live_ids.contains(id));
        for c in claude {
            sessions.push(self.claude_session(c));
        }

        match read_active(&self.paths.opencode_db, self.procs.as_ref(), now_ms) {
            Ok(list) => sessions.extend(list.into_iter().map(|o| opencode_session(o, &self.paths.home))),
            Err(e) => warnings.push(format!("opencode: {e}")),
        }

        sessions.sort_by(|a, b| {
            let wa = a.status == Status::Waiting;
            let wb = b.status == Status::Waiting;
            wb.cmp(&wa).then(b.updated_at_ms.cmp(&a.updated_at_ms))
        });
        LiveSnapshot { sessions, warnings, generated_at_ms: now_ms }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ActivityKind, Agent};
    use crate::process::{FakeProcessTable, ProcInfo};
    use std::fs;

    fn setup() -> (tempfile::TempDir, Paths) {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap().to_string();
        let paths = Paths::for_home(&home);
        fs::create_dir_all(&paths.claude_sessions).unwrap();
        fs::create_dir_all(&paths.claude_projects).unwrap();
        (tmp, paths)
    }

    fn claude_session(paths: &Paths, pid: u32, sid: &str, cwd: &str, status: &str, updated: i64) {
        fs::write(
            paths.claude_sessions.join(format!("{pid}.json")),
            format!(r#"{{"pid":{pid},"sessionId":"{sid}","cwd":"{cwd}","status":"{status}","updatedAt":{updated}}}"#),
        )
        .unwrap();
    }

    #[test]
    fn project_name_uses_last_segment_and_tilde_for_home() {
        assert_eq!(project_name("/Users/yolk/Dev/kirimi", "/Users/yolk"), "kirimi");
        assert_eq!(project_name("/Users/yolk", "/Users/yolk"), "~");
        assert_eq!(project_name("/Users/yolk/Dev/kirimi/", "/Users/yolk"), "kirimi");
    }

    #[test]
    fn builds_claude_session_with_tokens_and_tool_activity() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 100, "s1", "/Users/yolk/Dev/kirimi", "busy", 1000);
        let dir = paths.claude_projects.join("-Users-yolk-Dev-kirimi");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("s1.jsonl"),
            "{\"type\":\"assistant\",\"gitBranch\":\"main\",\"message\":{\"id\":\"m1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"input_tokens\":1,\"output_tokens\":2},\"content\":[{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"Bash\",\"input\":{\"command\":\"bun test\"}}]}}\n",
        )
        .unwrap();
        let mut procs = FakeProcessTable::default();
        procs.alive.insert(100);
        let mut c = LiveCollector::new(paths, std::sync::Arc::new(procs));

        let snap = c.snapshot(2000);
        assert_eq!(snap.sessions.len(), 1);
        let s = &snap.sessions[0];
        assert_eq!(s.agent, Agent::Claude);
        assert_eq!(s.project, "kirimi");
        assert_eq!(s.model.as_deref(), Some("claude-sonnet-5"));
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(s.tokens.total(), 3);
        let a = s.activity.as_ref().unwrap();
        assert_eq!(a.kind, ActivityKind::Tool);
        assert_eq!(a.label, "Bash");
        assert_eq!(a.detail.as_deref(), Some("bun test"));
        assert!(snap.warnings.is_empty());
    }

    #[test]
    fn waiting_sessions_sort_first_then_most_recent() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "a", "/x/a", "busy", 500);
        claude_session(&paths, 2, "b", "/x/b", "waiting", 100);
        claude_session(&paths, 3, "c", "/x/c", "busy", 900);
        let mut procs = FakeProcessTable::default();
        procs.alive.extend([1, 2, 3]);
        let mut c = LiveCollector::new(paths, std::sync::Arc::new(procs));
        let ids: Vec<String> = c.snapshot(1000).sessions.into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec!["b", "c", "a"]);
    }

    #[test]
    fn waiting_activity_carries_waiting_for_text() {
        let (_tmp, paths) = setup();
        fs::write(
            paths.claude_sessions.join("9.json"),
            r#"{"pid":9,"sessionId":"w","cwd":"/x/w","status":"waiting","updatedAt":1,"waitingFor":"input needed"}"#,
        )
        .unwrap();
        let mut procs = FakeProcessTable::default();
        procs.alive.insert(9);
        let mut c = LiveCollector::new(paths, std::sync::Arc::new(procs));
        let a = c.snapshot(10).sessions[0].activity.clone().unwrap();
        assert_eq!(a.kind, ActivityKind::Waiting);
        assert_eq!(a.detail.as_deref(), Some("input needed"));
    }

    #[test]
    fn corrupt_opencode_db_yields_warning_but_keeps_other_sessions() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "a", "/x/a", "idle", 5);
        fs::create_dir_all(paths.opencode_db.parent().unwrap()).unwrap();
        fs::write(&paths.opencode_db, "garbage").unwrap();
        let mut procs = FakeProcessTable::default();
        procs.alive.insert(1);
        procs.procs.push(ProcInfo { pid: 50, command: "/x/opencode run t --dir /d".into() });
        let mut c = LiveCollector::new(paths, std::sync::Arc::new(procs));
        let snap = c.snapshot(10);
        assert_eq!(snap.sessions.len(), 1);
        assert_eq!(snap.warnings.len(), 1);
        assert!(snap.warnings[0].starts_with("opencode:"));
    }
}
