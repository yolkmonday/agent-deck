use crate::billing::{BillingMode, BillingTable};
use crate::claude::{read_claude_sessions, ClaudeLive};
use crate::health::{self, Thresholds};
use crate::model::{Activity, ActivityKind, Agent, LiveSnapshot, Orphan, Session, Status};
use crate::opencode::{read_active, running_dirs, OpencodeLive};
use crate::process::ProcessTable;
use crate::pricing::PriceTable;
use crate::subagent::{self, SubAgent, SubAgentMeta};
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
    transcripts: HashMap<(String, String), (PathBuf, TranscriptState)>,
    /// Keyed by (session id + cwd, agent id) so two sessions that share an id in
    /// different directories tail separate files.
    subagents: HashMap<(String, String), (PathBuf, TranscriptState)>,
    /// `meta.json` is tiny but parsed on every pass otherwise; the mtime decides
    /// whether a re-read is needed at all.
    meta_cache: HashMap<PathBuf, (i64, SubAgentMeta)>,
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

fn opencode_session(o: OpencodeLive, home: &str, prices: &PriceTable, billing: &BillingTable, thresholds: &Thresholds, now_ms: i64) -> Session {
    let activity = match (&o.running_tool, o.status) {
        (Some(t), _) => Some(Activity { kind: ActivityKind::Tool, label: t.clone(), detail: None }),
        (None, Status::Busy) => Some(Activity {
            kind: ActivityKind::Thinking,
            label: "thinking".into(),
            detail: None,
        }),
        _ => None,
    };
    let priced = o.model.as_deref().is_some_and(|m| prices.matches(m));
    let cost_usd = match o.model.as_deref() {
        Some(m) if priced => prices.cost_usd(m, &o.tokens),
        _ => 0.0,
    };
    let quiet_ms = (now_ms - o.updated_at_ms).max(0);
    let tool_running_ms = o.running_tool.as_ref().and(o.tool_started_ms).map(|t| (now_ms - t).max(0));
    let tool = o.running_tool.as_deref().zip(tool_running_ms);
    let (health, health_reason) = health::evaluate(o.status, quiet_ms, tool, thresholds);
    let own_tokens = o.tokens.clone();
    let billing_mode = o
        .model
        .as_deref()
        .and_then(|m| billing.match_account(m))
        .map(|a| a.mode)
        .unwrap_or(BillingMode::Payg);
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
        tokens: own_tokens.clone(),
        own_tokens,
        subagents: Vec::new(),
        cost_usd,
        billing_mode,
        priced,
        started_at_ms: None,
        updated_at_ms: o.updated_at_ms,
        quiet_ms,
        tool_running_ms,
        health,
        health_reason,
    }
}

impl LiveCollector {
    pub fn new(paths: Paths, procs: Arc<dyn ProcessTable>) -> Self {
        Self { paths, procs, transcripts: HashMap::new(), subagents: HashMap::new(), meta_cache: HashMap::new() }
    }

    fn claude_session(&mut self, c: ClaudeLive, prices: &PriceTable, billing: &BillingTable, thresholds: &Thresholds, now_ms: i64) -> Session {
        let projects = self.paths.claude_projects.clone();
        let (path, state) = self
            .transcripts
            .entry((c.session_id.clone(), c.cwd.clone()))
            .or_insert_with(|| (PathBuf::new(), TranscriptState::default()));
        if path.as_os_str().is_empty() {
            if let Some(p) = find_transcript(&projects, &c.cwd, &c.session_id) {
                *path = p;
            }
        }
        if !path.as_os_str().is_empty() {
            let _ = state.advance(path);
        }
        let model = state.model.clone();
        let branch = state.branch.clone();
        let last_record_ms = state.last_record_ms;
        let pending_tool_id = state.pending_tool_id.clone();
        let tool_started_ms = state.tool_started_ms;
        let last_tool = state.last_tool.clone();
        let parent_path = path.clone();
        let agents_done = state.agents_done.clone();
        let activity = claude_activity(&c, state);

        let priced = model.as_deref().is_some_and(|m| prices.matches(m));
        let own_tokens = state.tokens.clone();
        let own_cost = match model.as_deref() {
            Some(m) if priced => prices.cost_usd(m, &own_tokens),
            _ => 0.0,
        };
        let subagents =
            self.running_subagents(&c.session_id, &c.cwd, &parent_path, &agents_done, prices);
        let mut tokens = own_tokens.clone();
        let mut cost_usd = own_cost;
        for s in &subagents {
            tokens.add(&s.tokens);
            cost_usd += s.cost_usd;
        }
        // A transcript record is the better signal when there is one; the session
        // file's stamp is only a fallback for a session that never wrote one.
        let quiet_ms = (now_ms - last_record_ms.unwrap_or(c.updated_at_ms)).max(0);
        let tool_running_ms = pending_tool_id.as_ref().and(tool_started_ms).map(|t| (now_ms - t).max(0));
        let tool = last_tool.as_ref().map(|t| t.name.as_str()).zip(tool_running_ms);
        let (health, health_reason) = health::evaluate(c.status, quiet_ms, tool, thresholds);
        let billing_mode = model
            .as_deref()
            .and_then(|m| billing.match_account(m))
            .map(|a| a.mode)
            .unwrap_or(BillingMode::Payg);
        Session {
            id: c.session_id.clone(),
            agent: Agent::Claude,
            pid: Some(c.pid),
            project: project_name(&c.cwd, &self.paths.home),
            cwd: c.cwd.clone(),
            model,
            branch,
            status: c.status,
            activity,
            tokens,
            own_tokens,
            subagents,
            cost_usd,
            billing_mode,
            priced,
            started_at_ms: c.started_at_ms,
            updated_at_ms: c.updated_at_ms,
            quiet_ms,
            tool_running_ms,
            health,
            health_reason,
        }
    }

    fn session_subagents_dir(&self, session_id: &str, cwd: &str) -> PathBuf {
        let encoded = crate::transcript::encode_cwd(cwd);
        self.paths.claude_projects.join(encoded).join(session_id).join("subagents")
    }

    /// The subagents a session is still running, newest first. A parent
    /// transcript names each agent whose result has landed, so anything left in
    /// `subagents/` that is not in `agents_done` is still in flight.
    fn running_subagents(
        &mut self,
        session_id: &str,
        cwd: &str,
        parent_path: &PathBuf,
        agents_done: &HashSet<String>,
        prices: &PriceTable,
    ) -> Vec<SubAgent> {
        if parent_path.as_os_str().is_empty() {
            return Vec::new();
        }
        let session_dir = parent_path.with_extension("");
        let mut out: Vec<SubAgent> = Vec::new();
        for entry in subagent::scan(&session_dir) {
            if agents_done.contains(&entry.id) {
                continue;
            }
            let key = (format!("{session_id}|{cwd}"), entry.id.clone());
            let (_, state) = self
                .subagents
                .entry(key)
                .or_insert_with(|| (entry.jsonl.clone(), TranscriptState::default()));
            let _ = state.advance(&entry.jsonl);
            let tokens = state.tokens.clone();
            let transcript_model = state.model.clone();
            let started_ms = state.started_ms;
            let meta = self.read_meta_cached(&entry.meta_path);
            let model = meta.model.clone().or(transcript_model);
            let priced = model.as_deref().is_some_and(|m| prices.matches(m));
            let cost_usd = match model.as_deref() {
                Some(m) if priced => prices.cost_usd(m, &tokens),
                _ => 0.0,
            };
            out.push(SubAgent {
                id: entry.id,
                agent_type: meta.agent_type,
                description: meta.description,
                model,
                tokens,
                cost_usd,
                priced,
                started_ms,
            });
        }
        out.sort_by(|a, b| match (a.started_ms, b.started_ms) {
            (Some(x), Some(y)) => y.cmp(&x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => b.id.cmp(&a.id),
        });
        out
    }

    fn read_meta_cached(&mut self, path: &std::path::Path) -> SubAgentMeta {
        let mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64);
        let key = path.to_path_buf();
        if let Some((cached_mtime, meta)) = self.meta_cache.get(&key) {
            if Some(*cached_mtime) == mtime {
                return meta.clone();
            }
        }
        let meta = subagent::read_meta(path).unwrap_or(SubAgentMeta {
            agent_type: String::new(),
            description: String::new(),
            model: None,
        });
        self.meta_cache.insert(key, (mtime.unwrap_or(-1), meta.clone()));
        meta
    }

    pub fn snapshot(
        &mut self,
        now_ms: i64,
        prices: &PriceTable,
        billing: &BillingTable,
        thresholds: &Thresholds,
    ) -> LiveSnapshot {
        let mut sessions = Vec::new();
        let mut warnings = Vec::new();

        let claude = read_claude_sessions(&self.paths.claude_sessions, self.procs.as_ref());
        let live_ids: HashSet<(String, String)> =
            claude.iter().map(|c| (c.session_id.clone(), c.cwd.clone())).collect();
        self.transcripts.retain(|key, _| live_ids.contains(key));
        let live_prefixes: Vec<String> =
            live_ids.iter().map(|(sid, cwd)| format!("{sid}|{cwd}")).collect();
        self.subagents.retain(|(prefix, _), _| live_prefixes.iter().any(|p| p == prefix));
        let live_metas: HashSet<PathBuf> =
            live_ids.iter().map(|(sid, cwd)| self.session_subagents_dir(sid, cwd)).collect();
        self.meta_cache.retain(|path, _| live_metas.contains(path.parent().unwrap_or(path)));
        for c in claude {
            sessions.push(self.claude_session(c, prices, billing, thresholds, now_ms));
        }

        let mut session_dirs: HashSet<String> = sessions.iter().map(|s| s.cwd.clone()).collect();
        match read_active(&self.paths.opencode_db, self.procs.as_ref(), now_ms) {
            Ok(list) => {
                for o in list {
                    session_dirs.insert(o.directory.clone());
                    sessions.push(opencode_session(o, &self.paths.home, prices, billing, thresholds, now_ms));
                }
            }
            Err(e) => warnings.push(format!("opencode: {e}")),
        }

        let orphans = self.orphans(now_ms, &session_dirs);

        sessions.sort_by(|a, b| {
            let wa = a.status == Status::Waiting;
            let wb = b.status == Status::Waiting;
            wb.cmp(&wa).then(b.updated_at_ms.cmp(&a.updated_at_ms))
        });
        let cost_usd = sessions.iter().map(|s| s.cost_usd).sum();
        let unpriced = sessions.iter().filter(|s| !s.priced).count();
        LiveSnapshot { sessions, warnings, generated_at_ms: now_ms, cost_usd, unpriced, orphans }
    }

    /// An agent process whose directory has no session is the shape seen when a
    /// delegated run stays alive without ever starting one. Only a process whose
    /// age is known counts: guessing an age would invent a duration.
    fn orphans(&self, now_ms: i64, session_dirs: &HashSet<String>) -> Vec<Orphan> {
        let mut out: Vec<Orphan> = running_dirs(self.procs.as_ref())
            .into_iter()
            .filter(|(dir, _)| !session_dirs.contains(dir))
            .filter_map(|(cwd, pid)| {
                let age_ms = (now_ms - self.procs.start_time_ms(pid)?).max(0);
                Some(Orphan { agent: Agent::Opencode, pid, cwd, age_ms })
            })
            .collect();
        out.sort_by(|a, b| a.pid.cmp(&b.pid));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::BillingAccount;
    use crate::health::{Health, Thresholds};
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

        let snap = c.snapshot(2000, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
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
        let ids: Vec<String> = c.snapshot(1000, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults()).sessions.into_iter().map(|s| s.id).collect();
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
        let a = c.snapshot(10, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults()).sessions[0].activity.clone().unwrap();
        assert_eq!(a.kind, ActivityKind::Waiting);
        assert_eq!(a.detail.as_deref(), Some("input needed"));
    }

    #[test]
    fn same_session_id_in_two_cwds_keeps_separate_transcript_state() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "dup", "/Users/yolk/Dev/a", "busy", 10);
        claude_session(&paths, 2, "dup", "/Users/yolk/Dev/b", "busy", 20);
        for (enc, out) in [("-Users-yolk-Dev-a", 11u64), ("-Users-yolk-Dev-b", 22u64)] {
            let dir = paths.claude_projects.join(enc);
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("dup.jsonl"),
                format!("{{\"type\":\"assistant\",\"message\":{{\"id\":\"m-{enc}\",\"usage\":{{\"output_tokens\":{out}}}}}}}\n"),
            )
            .unwrap();
        }
        let mut procs = FakeProcessTable::default();
        procs.alive.extend([1, 2]);
        let mut c = LiveCollector::new(paths, std::sync::Arc::new(procs));

        let snap = c.snapshot(100, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        let a = snap.sessions.iter().find(|s| s.cwd.ends_with("/a")).unwrap();
        let b = snap.sessions.iter().find(|s| s.cwd.ends_with("/b")).unwrap();
        assert_eq!(a.tokens.output, 11);
        assert_eq!(b.tokens.output, 22);
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
        let snap = c.snapshot(10, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        assert_eq!(snap.sessions.len(), 1);
        assert_eq!(snap.warnings.len(), 1);
        assert!(snap.warnings[0].starts_with("opencode:"));
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn claude_transcript(paths: &Paths, cwd: &str, sid: &str, body: &str) {
        let dir = paths.claude_projects.join(cwd.replace('/', "-"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{sid}.jsonl")), format!("{body}\n")).unwrap();
    }

    fn append_line(path: &std::path::Path, body: &str) {
        use std::io::Write;
        let mut f = fs::OpenOptions::new().append(true).open(path).unwrap();
        writeln!(f, "{body}").unwrap();
    }

    fn collector(paths: Paths, pids: &[u32]) -> LiveCollector {
        let mut procs = FakeProcessTable::default();
        procs.alive.extend(pids.iter().copied());
        LiveCollector::new(paths, std::sync::Arc::new(procs))
    }

    #[test]
    fn session_cost_uses_the_price_table() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", "/Users/yolk/Dev/kirimi", "busy", 10);
        claude_transcript(
            &paths,
            "/Users/yolk/Dev/kirimi",
            "s1",
            "{\"type\":\"assistant\",\"message\":{\"id\":\"m1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":1000000}}}",
        );
        let mut c = collector(paths, &[1]);

        let snap = c.snapshot(20, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        let s = &snap.sessions[0];
        assert!(close(s.cost_usd, 15.0));
        assert!(s.priced);
        assert!(close(snap.cost_usd, 15.0));
        assert_eq!(snap.unpriced, 0);
    }

    #[test]
    fn unknown_model_is_zero_and_unpriced() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", "/Users/yolk/Dev/kirimi", "busy", 10);
        claude_transcript(
            &paths,
            "/Users/yolk/Dev/kirimi",
            "s1",
            "{\"type\":\"assistant\",\"message\":{\"id\":\"m1\",\"model\":\"weird-model-9\",\"usage\":{\"output_tokens\":1000000}}}",
        );
        let mut c = collector(paths, &[1]);

        let snap = c.snapshot(20, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        let s = &snap.sessions[0];
        assert_eq!(s.cost_usd, 0.0);
        assert!(!s.priced);
        assert_eq!(snap.unpriced, 1);
    }

    #[test]
    fn session_without_a_model_is_unpriced() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", "/Users/yolk/Dev/kirimi", "busy", 10);
        let mut c = collector(paths, &[1]);

        let snap = c.snapshot(20, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        assert_eq!(snap.sessions[0].model, None);
        assert!(!snap.sessions[0].priced);
        assert_eq!(snap.unpriced, 1);
    }

    #[test]
    fn snapshot_total_sums_session_costs() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", "/Users/yolk/Dev/a", "busy", 10);
        claude_session(&paths, 2, "s2", "/Users/yolk/Dev/b", "busy", 20);
        claude_transcript(
            &paths,
            "/Users/yolk/Dev/a",
            "s1",
            "{\"type\":\"assistant\",\"message\":{\"id\":\"m1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":1000000}}}",
        );
        claude_transcript(
            &paths,
            "/Users/yolk/Dev/b",
            "s2",
            "{\"type\":\"assistant\",\"message\":{\"id\":\"m2\",\"model\":\"claude-opus-5\",\"usage\":{\"output_tokens\":1000000}}}",
        );
        let mut c = collector(paths, &[1, 2]);

        let snap = c.snapshot(30, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        let expected: f64 = snap.sessions.iter().map(|s| s.cost_usd).sum();
        assert!(close(snap.cost_usd, expected));
        assert!(close(snap.cost_usd, 90.0));
        assert_eq!(snap.unpriced, 0);
    }

    #[test]
    fn same_content_notices_a_cost_change() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", "/Users/yolk/Dev/a", "busy", 10);
        claude_transcript(
            &paths,
            "/Users/yolk/Dev/a",
            "s1",
            "{\"type\":\"assistant\",\"message\":{\"id\":\"m1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":1000000}}}",
        );
        let mut c = collector(paths, &[1]);

        let a = c.snapshot(20, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        let mut b = a.clone();
        b.cost_usd += 1.0;
        assert!(!a.same_content(&b));
    }

    const MIN: i64 = 60_000;

    /// Emits the ISO 8601 string Claude actually writes, not a bare number.
    /// The earlier numeric fixture let a real parsing bug pass unnoticed.
    fn iso(ms: i64) -> String {
        let t = time::OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000).unwrap();
        t.format(&time::format_description::well_known::Rfc3339).unwrap()
    }

    fn assistant_at(ms: i64, body: &str) -> String {
        format!("{{\"type\":\"assistant\",\"timestamp\":\"{}\",\"message\":{{{body}}}}}", iso(ms))
    }

    #[test]
    fn quiet_ms_comes_from_the_last_transcript_record() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", "/Users/yolk/Dev/a", "busy", 1);
        claude_transcript(
            &paths,
            "/Users/yolk/Dev/a",
            "s1",
            &assistant_at(500_000, "\"id\":\"m1\",\"usage\":{\"output_tokens\":1}"),
        );
        let mut c = collector(paths, &[1]);

        let snap = c.snapshot(620_000, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        assert_eq!(snap.sessions[0].quiet_ms, 120_000);
    }

    #[test]
    fn quiet_ms_falls_back_to_the_session_file() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", "/Users/yolk/Dev/a", "busy", 500_000);
        let mut c = collector(paths, &[1]);

        let snap = c.snapshot(560_000, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        assert_eq!(snap.sessions[0].quiet_ms, 60_000);
    }

    #[test]
    fn tool_running_ms_is_set_while_a_tool_is_open_and_cleared_after() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", "/Users/yolk/Dev/a", "busy", 1);
        claude_transcript(
            &paths,
            "/Users/yolk/Dev/a",
            "s1",
            "{\"type\":\"assistant\",\"timestamp\":100000,\"message\":{\"id\":\"m1\",\"content\":[{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"Bash\",\"input\":{\"command\":\"bun test\"}}]}}",
        );
        let transcript = paths.claude_projects.join("-Users-yolk-Dev-a").join("s1.jsonl");
        let mut c = collector(paths, &[1]);

        let open = c.snapshot(160_000, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        assert_eq!(open.sessions[0].tool_running_ms, Some(60_000));

        append_line(
            &transcript,
            "{\"type\":\"user\",\"timestamp\":200000,\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"t1\"}]}}",
        );
        let closed = c.snapshot(260_000, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        assert_eq!(closed.sessions[0].tool_running_ms, None);
    }

    #[test]
    fn a_long_quiet_busy_session_is_reported_stalled() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", "/Users/yolk/Dev/a", "busy", 1);
        claude_transcript(
            &paths,
            "/Users/yolk/Dev/a",
            "s1",
            &assistant_at(100_000, "\"id\":\"m1\",\"usage\":{\"output_tokens\":1}"),
        );
        let mut c = collector(paths, &[1]);

        let snap = c.snapshot(100_000 + 6 * MIN, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        let s = &snap.sessions[0];
        assert_eq!(s.health, Health::Stalled);
        assert!(s.health_reason.as_deref().is_some_and(|r| !r.is_empty()));
    }

    #[test]
    fn an_orphan_opencode_process_is_reported() {
        let (_tmp, paths) = setup();
        let mut procs = FakeProcessTable::default();
        procs.procs.push(ProcInfo { pid: 77, command: "/x/opencode run t --dir /x/y".into() });
        procs.start_times.insert(77, 400_000);
        let mut c = LiveCollector::new(paths, std::sync::Arc::new(procs));

        let snap = c.snapshot(1_000_000, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        assert_eq!(snap.orphans.len(), 1);
        let o = &snap.orphans[0];
        assert_eq!(o.pid, 77);
        assert_eq!(o.cwd, "/x/y");
        assert_eq!(o.agent, Agent::Opencode);
        assert_eq!(o.age_ms, 600_000);
    }

    const CWD: &str = "/Users/yolk/Dev/kirimi";

    /// Writes a subagent transcript plus its meta beside the parent's, which is
    /// how Claude lays them out: `subagents/agent-<id>.{jsonl,meta.json}`.
    fn subagent_files(
        paths: &Paths,
        cwd: &str,
        sid: &str,
        id: &str,
        meta: &str,
        body: &str,
    ) -> std::path::PathBuf {
        let dir = paths
            .claude_projects
            .join(cwd.replace('/', "-"))
            .join(sid)
            .join("subagents");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("agent-{id}.meta.json")), meta).unwrap();
        let jsonl = dir.join(format!("agent-{id}.jsonl"));
        fs::write(&jsonl, format!("{body}\n")).unwrap();
        jsonl
    }

    fn parent_transcript(paths: &Paths, cwd: &str, sid: &str, body: &str) {
        let dir = paths.claude_projects.join(cwd.replace('/', "-"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{sid}.jsonl")), format!("{body}\n")).unwrap();
    }

    const SCOUT_META: &str = r#"{"agentType":"scout","description":"Find icon names","model":"claude-sonnet-5","spawnDepth":1}"#;

    #[test]
    fn running_subagents_appear_on_the_parent() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", CWD, "busy", 10);
        parent_transcript(
            &paths,
            CWD,
            "s1",
            "{\"type\":\"assistant\",\"message\":{\"id\":\"m1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":10}}}",
        );
        subagent_files(
            &paths,
            CWD,
            "s1",
            "a1",
            SCOUT_META,
            "{\"type\":\"assistant\",\"message\":{\"id\":\"sa1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":5}}}",
        );
        let mut c = collector(paths, &[1]);

        let snap = c.snapshot(20, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        let subs = &snap.sessions[0].subagents;
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].id, "a1");
        assert_eq!(subs[0].agent_type, "scout");
        assert_eq!(subs[0].description, "Find icon names");
        assert_eq!(subs[0].model.as_deref(), Some("claude-sonnet-5"));
    }

    #[test]
    fn finished_subagents_are_hidden() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", CWD, "busy", 10);
        parent_transcript(
            &paths,
            CWD,
            "s1",
            &assistant_at(10_000, "\"id\":\"m1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":10}"),
        );
        append_line(
            &paths.claude_projects.join(CWD.replace('/', "-")).join("s1.jsonl"),
            "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"t1\"}]},\"toolUseResult\":{\"agentId\":\"a1\",\"status\":\"completed\"}}",
        );
        subagent_files(
            &paths,
            CWD,
            "s1",
            "a1",
            SCOUT_META,
            "{\"type\":\"assistant\",\"message\":{\"id\":\"sa1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":5}}}",
        );
        let mut c = collector(paths, &[1]);

        let snap = c.snapshot(20, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        assert!(snap.sessions[0].subagents.is_empty());
        assert_eq!(snap.sessions[0].tokens.output, 10, "a finished agent is not folded in");
    }

    #[test]
    fn subagent_tokens_are_added_to_the_parent() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", CWD, "busy", 10);
        parent_transcript(
            &paths,
            CWD,
            "s1",
            "{\"type\":\"assistant\",\"message\":{\"id\":\"m1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":10}}}",
        );
        subagent_files(
            &paths,
            CWD,
            "s1",
            "a1",
            SCOUT_META,
            "{\"type\":\"assistant\",\"message\":{\"id\":\"sa1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":5}}}",
        );
        let mut c = collector(paths, &[1]);

        let s = &c.snapshot(20, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults()).sessions[0];
        assert_eq!(s.tokens.output, 15);
        assert_eq!(s.own_tokens.output, 10);
        assert_eq!(s.subagents[0].tokens.output, 5);
    }

    #[test]
    fn subagent_cost_is_added_to_the_parent() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", CWD, "busy", 10);
        parent_transcript(
            &paths,
            CWD,
            "s1",
            "{\"type\":\"assistant\",\"message\":{\"id\":\"m1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":1000}}}",
        );
        subagent_files(
            &paths,
            CWD,
            "s1",
            "a1",
            SCOUT_META,
            "{\"type\":\"assistant\",\"message\":{\"id\":\"sa1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":2000}}}",
        );
        let mut c = collector(paths, &[1]);

        let prices = PriceTable::defaults();
        let s = &c.snapshot(20, &prices, &BillingTable::defaults(), &Thresholds::defaults()).sessions[0];
        let own = prices.cost_usd("claude-sonnet-5", &s.own_tokens);
        let sub = s.subagents[0].cost_usd;
        assert!(sub > 0.0);
        assert!(close(s.cost_usd, own + sub));
        assert!(s.priced);
        assert!(s.subagents[0].priced);
    }

    #[test]
    fn a_session_without_a_subagent_directory_has_an_empty_list() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", CWD, "busy", 10);
        parent_transcript(
            &paths,
            CWD,
            "s1",
            "{\"type\":\"assistant\",\"message\":{\"id\":\"m1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":10}}}",
        );
        let mut c = collector(paths, &[1]);

        let s = &c.snapshot(20, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults()).sessions[0];
        assert!(s.subagents.is_empty());
        assert_eq!(s.tokens.output, 10);
        assert_eq!(s.own_tokens.output, 10);
    }

    #[test]
    fn subagents_sort_newest_first() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", CWD, "busy", 10);
        parent_transcript(
            &paths,
            CWD,
            "s1",
            &assistant_at(10_000, "\"id\":\"m1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":1}"),
        );
        subagent_files(
            &paths,
            CWD,
            "s1",
            "old",
            SCOUT_META,
            &assistant_at(20_000, "\"id\":\"sa-old\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":1}"),
        );
        subagent_files(
            &paths,
            CWD,
            "s1",
            "new",
            SCOUT_META,
            &assistant_at(90_000, "\"id\":\"sa-new\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":1}"),
        );
        let mut c = collector(paths, &[1]);

        let subs = &c.snapshot(100_000, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults()).sessions[0].subagents;
        let ids: Vec<&str> = subs.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["new", "old"]);
        assert_eq!(subs[0].started_ms, Some(90_000));
        assert_eq!(subs[1].started_ms, Some(20_000));
    }

    #[test]
    fn billing_mode_follows_the_matched_account() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", "/Users/yolk/Dev/kirimi", "busy", 10);
        claude_transcript(
            &paths,
            "/Users/yolk/Dev/kirimi",
            "s1",
            "{\"type\":\"assistant\",\"message\":{\"id\":\"m1\",\"model\":\"claude-sonnet-5\",\"usage\":{\"output_tokens\":1}}}",
        );
        let mut c = collector(paths, &[1]);
        let billing = BillingTable::new(vec![BillingAccount {
            id: "sub".into(),
            label: "Claude Max".into(),
            mode: BillingMode::Subscription,
            matches: vec!["claude-".into()],
            monthly_usd: Some(200.0),
            renewal_day: Some(14),
            credit_usd: None,
            started_on: None,
            expires_on: None,
        }]);

        let snap = c.snapshot(20, &PriceTable::defaults(), &billing, &Thresholds::defaults());
        let s = &snap.sessions[0];
        assert_eq!(s.billing_mode, BillingMode::Subscription);
        // The token figure still reports what these tokens would have cost.
        assert!(s.cost_usd > 0.0);
    }

    #[test]
    fn unmatched_model_is_payg() {
        let (_tmp, paths) = setup();
        claude_session(&paths, 1, "s1", "/Users/yolk/Dev/kirimi", "busy", 10);
        let mut c = collector(paths, &[1]);
        let snap = c.snapshot(20, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        assert_eq!(snap.sessions[0].billing_mode, BillingMode::Payg);
    }

    #[test]
    fn no_orphan_when_the_directory_has_a_session() {
        let (_tmp, paths) = setup();
        fs::create_dir_all(paths.opencode_db.parent().unwrap()).unwrap();
        let db = &paths.opencode_db;
        let conn = rusqlite::Connection::open(db).unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, project_id TEXT, parent_id TEXT, directory TEXT, title TEXT, agent TEXT, model TEXT, cost REAL, tokens_input INTEGER, tokens_output INTEGER, tokens_reasoning INTEGER, tokens_cache_read INTEGER, tokens_cache_write INTEGER, time_created INTEGER, time_updated INTEGER, time_archived INTEGER);
             CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, data TEXT);
             INSERT INTO session VALUES ('sa','p',NULL,'/x/y','t','build','{\"id\":\"m\",\"providerID\":\"kn\"}',0,1,1,0,0,0,1,990000,NULL);",
        )
        .unwrap();
        drop(conn);

        let mut procs = FakeProcessTable::default();
        procs.procs.push(ProcInfo { pid: 77, command: "/x/opencode run t --dir /x/y".into() });
        procs.start_times.insert(77, 400_000);
        let mut c = LiveCollector::new(paths, std::sync::Arc::new(procs));

        let snap = c.snapshot(1_000_000, &PriceTable::defaults(), &BillingTable::defaults(), &Thresholds::defaults());
        assert_eq!(snap.sessions.len(), 1);
        assert!(snap.orphans.is_empty());
    }
}
