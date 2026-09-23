use crate::model::TokenUsage;
use crate::task_text::one_line;
use anyhow::Result;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const CHUNK: u64 = 16 * 1024 * 1024;
const DETAIL_MAX: usize = 80;
const TASK_MAX: usize = 120;

/// Prompt-shaped user text that is not the user's own words. Claude writes a
/// slash command, a hook, or injected context as a `user` record too.
const PROMPT_SKIP_PREFIXES: [&str; 4] =
    ["<command-", "<local-command-", "<system-reminder>", "Caveat:"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub detail: Option<String>,
}

#[derive(Debug, Default)]
pub struct TranscriptState {
    offset: u64,
    per_message: HashMap<String, TokenUsage>,
    pub tokens: TokenUsage,
    pub model: Option<String>,
    pub branch: Option<String>,
    pub last_tool: Option<ToolCall>,
    pub pending_tool_id: Option<String>,
    pub turn_ended: bool,
    /// Timestamp of the most recent record this tailer parsed. `None` until the
    /// transcript has any record with a timestamp.
    pub last_record_ms: Option<i64>,
    /// Timestamp of the record that opened the still-pending tool. Cleared when
    /// the tool closes, so it is only ever a duration while a tool is open.
    pub tool_started_ms: Option<i64>,
    /// Agent ids whose result has landed in this transcript. A subagent listed
    /// here has finished; anything else in `subagents/` may still be running.
    pub agents_done: HashSet<String>,
    /// Timestamp of the very first record this tailer ever parsed, so a
    /// subagent can be ordered by when it was spawned.
    pub started_ms: Option<i64>,
    /// Claude's own summary of the session, from an `ai-title` record. The
    /// latest one wins, because the title is rewritten as the work moves on.
    pub ai_title: Option<String>,
    /// The first real user prompt, kept as the fallback title. Written once and
    /// never overwritten.
    pub first_prompt: Option<String>,
}

impl TranscriptState {
    /// What the session is for: Claude's own title when it has one, otherwise
    /// the first thing the user actually asked.
    pub fn task(&self) -> Option<String> {
        self.ai_title.clone().or_else(|| self.first_prompt.clone())
    }
}

pub fn encode_cwd(cwd: &str) -> String {
    cwd.chars().map(|c| if c == '/' || c == '.' { '-' } else { c }).collect()
}

pub fn find_transcript(projects_dir: &Path, cwd: &str, session_id: &str) -> Option<PathBuf> {
    let file = format!("{session_id}.jsonl");
    let direct = projects_dir.join(encode_cwd(cwd)).join(&file);
    if direct.exists() {
        return Some(direct);
    }
    std::fs::read_dir(projects_dir)
        .ok()?
        .flatten()
        .map(|e| e.path().join(&file))
        .find(|p| p.exists())
}

fn usage_from(u: &Value) -> TokenUsage {
    let get = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
    TokenUsage {
        input: get("input_tokens"),
        output: get("output_tokens"),
        cache_read: get("cache_read_input_tokens"),
        cache_write: get("cache_creation_input_tokens"),
        reasoning: u
            .get("output_tokens_details")
            .and_then(|d| d.get("thinking_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
    }
}

fn first_line_truncated(s: &str) -> String {
    s.lines().next().unwrap_or("").chars().take(DETAIL_MAX).collect()
}

pub(crate) fn tool_detail(input: Option<&Value>) -> Option<String> {
    let input = input?;
    ["command", "file_path", "pattern", "description"]
        .iter()
        .find_map(|k| input.get(*k).and_then(Value::as_str))
        .map(first_line_truncated)
}

impl TranscriptState {
    pub fn advance(&mut self, path: &Path) -> Result<()> {
        let mut file = File::open(path)?;
        if file.metadata()?.len() < self.offset {
            *self = TranscriptState::default();
        }
        loop {
            let mut read_size = CHUNK;
            let buf = loop {
                file.seek(SeekFrom::Start(self.offset))?;
                let mut buf = Vec::new();
                (&mut file).take(read_size).read_to_end(&mut buf)?;
                if buf.iter().rposition(|b| *b == b'\n').is_some() {
                    break buf;
                }
                if (buf.len() as u64) < read_size {
                    return Ok(());
                }
                read_size = read_size.saturating_mul(2);
            };
            let last_newline = buf.iter().rposition(|b| *b == b'\n').unwrap();
            let complete = last_newline + 1;
            for line in buf[..complete].split(|b| *b == b'\n') {
                if line.is_empty() {
                    continue;
                }
                if let Ok(v) = serde_json::from_slice::<Value>(line) {
                    self.apply(&v);
                }
            }
            self.offset += complete as u64;
            if (buf.len() as u64) < read_size {
                break;
            }
        }
        Ok(())
    }

    fn apply(&mut self, v: &Value) {
        // Claude writes `timestamp` as an ISO 8601 string, e.g. "2026-09-22T07:38:37.824Z".
        // Reading it as a number silently yields None on every real record, which leaves
        // `last_record_ms` empty and makes the caller fall back to a stale session file.
        if let Some(ts) = v.get("timestamp").and_then(|t| match t {
            Value::String(s) => crate::indexer::parse_iso_ms(s),
            other => other.as_i64(),
        }) {
            self.last_record_ms = Some(ts);
            if self.started_ms.is_none() {
                self.started_ms = Some(ts);
            }
        }
        match v.get("type").and_then(Value::as_str) {
            Some("assistant") => self.apply_assistant(v),
            Some("user") => self.apply_user(v),
            Some("ai-title") => {
                let title = v.get("aiTitle").and_then(Value::as_str).map(str::trim).unwrap_or("");
                if !title.is_empty() {
                    self.ai_title = Some(title.to_string());
                }
            }
            Some("system") => {
                if v.get("subtype").and_then(Value::as_str) == Some("turn_duration") {
                    self.turn_ended = true;
                    self.pending_tool_id = None;
                    self.tool_started_ms = None;
                }
            }
            _ => {}
        }
    }

    fn apply_assistant(&mut self, v: &Value) {
        self.turn_ended = false;
        if let Some(b) = v.get("gitBranch").and_then(Value::as_str).filter(|b| !b.is_empty()) {
            self.branch = Some(b.to_string());
        }
        let Some(msg) = v.get("message") else { return };
        if let Some(m) = msg.get("model").and_then(Value::as_str).filter(|m| *m != "<synthetic>") {
            self.model = Some(m.to_string());
        }
        if let (Some(id), Some(usage)) = (msg.get("id").and_then(Value::as_str), msg.get("usage")) {
            let new = usage_from(usage);
            if let Some(old) = self.per_message.insert(id.to_string(), new.clone()) {
                self.tokens.sub(&old);
            }
            self.tokens.add(&new);
        }
        let Some(items) = msg.get("content").and_then(Value::as_array) else { return };
        for item in items.iter().filter(|i| i.get("type").and_then(Value::as_str) == Some("tool_use")) {
            let (Some(id), Some(name)) = (
                item.get("id").and_then(Value::as_str),
                item.get("name").and_then(Value::as_str),
            ) else {
                continue;
            };
            self.pending_tool_id = Some(id.to_string());
            self.tool_started_ms = self.last_record_ms;
            self.last_tool = Some(ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                detail: tool_detail(item.get("input")),
            });
        }
    }

    fn apply_user(&mut self, v: &Value) {
        self.turn_ended = false;
        self.capture_first_prompt(v);
        if let Some(id) = v
            .get("toolUseResult")
            .and_then(|r| r.get("agentId"))
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        {
            self.agents_done.insert(id.to_string());
        }
        let Some(items) = v
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(Value::as_array)
        else {
            return;
        };
        for item in items.iter().filter(|i| i.get("type").and_then(Value::as_str) == Some("tool_result")) {
            let id = item.get("tool_use_id").and_then(Value::as_str);
            if id.is_some() && self.pending_tool_id.as_deref() == id {
                self.pending_tool_id = None;
                self.tool_started_ms = None;
            }
        }
    }

    /// The first thing the user typed, used when Claude has not titled the
    /// session yet. Only ever set once: a later prompt is a follow-up, not the
    /// task.
    fn capture_first_prompt(&mut self, v: &Value) {
        if self.first_prompt.is_some() {
            return;
        }
        if v.get("isMeta").and_then(Value::as_bool).unwrap_or(false) {
            return;
        }
        let Some(content) = v.get("message").and_then(|m| m.get("content")) else { return };
        let text = match content {
            Value::String(s) => s.as_str(),
            Value::Array(items) => match items
                .iter()
                .find(|i| i.get("type").and_then(Value::as_str) == Some("text"))
                .and_then(|i| i.get("text"))
                .and_then(Value::as_str)
            {
                Some(t) => t,
                None => return,
            },
            _ => return,
        };
        if PROMPT_SKIP_PREFIXES.iter().any(|p| text.trim_start().starts_with(p)) {
            return;
        }
        let task = one_line(text, TASK_MAX);
        if !task.is_empty() {
            self.first_prompt = Some(task);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn append(path: &Path, s: &str) {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
        f.write_all(s.as_bytes()).unwrap();
    }

    const ASSISTANT_TOOL: &str = r#"{"type":"assistant","gitBranch":"main","message":{"id":"m1","model":"claude-sonnet-5","usage":{"input_tokens":2,"output_tokens":598,"cache_read_input_tokens":27951,"cache_creation_input_tokens":50316,"output_tokens_details":{"thinking_tokens":345}},"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"bun test --filter fare"}}]}}"#;

    #[test]
    fn encodes_cwd_like_claude_does() {
        assert_eq!(encode_cwd("/Users/yolk/Dev/kirimi"), "-Users-yolk-Dev-kirimi");
        assert_eq!(
            encode_cwd("/Users/yolk/.claude-mem/observer-sessions"),
            "-Users-yolk--claude-mem-observer-sessions"
        );
    }

    #[test]
    fn counts_usage_once_per_message_id_and_tracks_tool() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        append(&p, &format!("{ASSISTANT_TOOL}\n{ASSISTANT_TOOL}\n"));
        let mut st = TranscriptState::default();
        st.advance(&p).unwrap();
        assert_eq!(st.tokens.input, 2);
        assert_eq!(st.tokens.output, 598);
        assert_eq!(st.tokens.cache_read, 27951);
        assert_eq!(st.tokens.cache_write, 50316);
        assert_eq!(st.tokens.reasoning, 345);
        assert_eq!(st.model.as_deref(), Some("claude-sonnet-5"));
        assert_eq!(st.branch.as_deref(), Some("main"));
        assert_eq!(st.pending_tool_id.as_deref(), Some("t1"));
        let tool = st.last_tool.clone().unwrap();
        assert_eq!(tool.name, "Bash");
        assert_eq!(tool.detail.as_deref(), Some("bun test --filter fare"));
    }

    #[test]
    fn later_usage_for_same_message_replaces_earlier() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        append(&p, "{\"type\":\"assistant\",\"message\":{\"id\":\"m2\",\"usage\":{\"output_tokens\":10}}}\n");
        append(&p, "{\"type\":\"assistant\",\"message\":{\"id\":\"m2\",\"usage\":{\"output_tokens\":50}}}\n");
        let mut st = TranscriptState::default();
        st.advance(&p).unwrap();
        assert_eq!(st.tokens.output, 50);
    }

    #[test]
    fn tool_result_and_turn_end_update_state_incrementally() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        append(&p, &format!("{ASSISTANT_TOOL}\n"));
        let mut st = TranscriptState::default();
        st.advance(&p).unwrap();
        assert!(st.pending_tool_id.is_some());

        append(&p, "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"t1\"}]}}\n");
        st.advance(&p).unwrap();
        assert!(st.pending_tool_id.is_none());
        assert!(!st.turn_ended);

        append(&p, "{\"type\":\"system\",\"subtype\":\"turn_duration\",\"durationMs\":10}\n");
        st.advance(&p).unwrap();
        assert!(st.turn_ended);
        assert_eq!(st.tokens.output, 598);
    }

    #[test]
    fn partial_line_is_consumed_only_when_complete() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        append(&p, "{\"type\":\"assistant\",\"message\":{\"id\":\"m3\",\"usage\":{\"output_tokens\":7}");
        let mut st = TranscriptState::default();
        st.advance(&p).unwrap();
        assert_eq!(st.tokens.output, 0);
        append(&p, "}}\n");
        st.advance(&p).unwrap();
        assert_eq!(st.tokens.output, 7);
    }

    #[test]
    fn line_larger_than_chunk_is_consumed_and_does_not_stall() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        let big = "x".repeat(CHUNK as usize + 1024);
        let huge_line = format!(
            "{{\"type\":\"user\",\"message\":{{\"content\":[{{\"type\":\"tool_result\",\"tool_use_id\":\"t0\",\"content\":\"{big}\"}}]}}}}\n"
        );
        append(&p, &huge_line);
        append(&p, "{\"type\":\"assistant\",\"message\":{\"id\":\"after\",\"usage\":{\"output_tokens\":42}}}\n");

        let mut st = TranscriptState::default();
        st.advance(&p).unwrap();
        assert_eq!(st.tokens.output, 42, "line after the oversized record must be read");

        append(&p, "{\"type\":\"assistant\",\"message\":{\"id\":\"later\",\"usage\":{\"output_tokens\":8}}}\n");
        st.advance(&p).unwrap();
        assert_eq!(st.tokens.output, 50, "tailer must keep progressing afterwards");
    }

    #[test]
    fn truncated_file_resets_state() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        append(&p, &format!("{ASSISTANT_TOOL}\n"));
        let mut st = TranscriptState::default();
        st.advance(&p).unwrap();
        std::fs::write(&p, "{\"type\":\"assistant\",\"message\":{\"id\":\"n1\",\"usage\":{\"output_tokens\":1}}}\n").unwrap();
        st.advance(&p).unwrap();
        assert_eq!(st.tokens.output, 1);
        assert!(st.pending_tool_id.is_none());
    }

    #[test]
    fn agents_done_collects_agent_ids_from_tool_use_result() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        append(
            &p,
            "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"t1\"}]},\"toolUseResult\":{\"agentId\":\"a1\",\"status\":\"completed\"}}\n",
        );
        let mut st = TranscriptState::default();
        st.advance(&p).unwrap();
        assert!(st.agents_done.contains("a1"));
    }

    #[test]
    fn agents_done_ignores_a_tool_result_without_an_agent_id() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        append(
            &p,
            "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"t1\"}]},\"toolUseResult\":{\"stdout\":\"hello\"}}\n",
        );
        let mut st = TranscriptState::default();
        st.advance(&p).unwrap();
        assert!(st.agents_done.is_empty());
    }

    #[test]
    fn agents_done_survives_incremental_reads() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        append(
            &p,
            "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"t1\"}]},\"toolUseResult\":{\"agentId\":\"a1\"}}\n",
        );
        let mut st = TranscriptState::default();
        st.advance(&p).unwrap();
        append(
            &p,
            "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"t2\"}]},\"toolUseResult\":{\"agentId\":\"a2\"}}\n",
        );
        st.advance(&p).unwrap();
        assert!(st.agents_done.contains("a1"));
        assert!(st.agents_done.contains("a2"));
    }

    #[test]
    fn find_transcript_uses_encoded_dir_then_scans() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path();
        std::fs::create_dir_all(projects.join("-Users-yolk-Dev-kirimi")).unwrap();
        std::fs::write(projects.join("-Users-yolk-Dev-kirimi/s1.jsonl"), "").unwrap();
        std::fs::create_dir_all(projects.join("-other")).unwrap();
        std::fs::write(projects.join("-other/s2.jsonl"), "").unwrap();
        assert!(find_transcript(projects, "/Users/yolk/Dev/kirimi", "s1").is_some());
        assert!(find_transcript(projects, "/Users/yolk/Dev/kirimi", "s2").is_some());
        assert!(find_transcript(projects, "/Users/yolk/Dev/kirimi", "nope").is_none());
    }

    fn advance_lines(lines: &[&str]) -> TranscriptState {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        append(&p, &format!("{}\n", lines.join("\n")));
        let mut st = TranscriptState::default();
        st.advance(&p).unwrap();
        st
    }

    #[test]
    fn ai_title_becomes_the_task() {
        let st = advance_lines(&[r#"{"type":"ai-title","aiTitle":"Perbaiki halaman login"}"#]);
        assert_eq!(st.task().as_deref(), Some("Perbaiki halaman login"));
    }

    #[test]
    fn the_latest_ai_title_wins() {
        let st = advance_lines(&[
            r#"{"type":"ai-title","aiTitle":"Judul lama"}"#,
            r#"{"type":"assistant","message":{"id":"m1","usage":{"output_tokens":1}}}"#,
            r#"{"type":"ai-title","aiTitle":"Judul baru"}"#,
        ]);
        assert_eq!(st.task().as_deref(), Some("Judul baru"));
    }

    #[test]
    fn ai_title_wins_over_the_first_prompt() {
        let st = advance_lines(&[
            r#"{"type":"user","message":{"content":"tolong tambah tombol"}}"#,
            r#"{"type":"ai-title","aiTitle":"Tambah tombol"}"#,
        ]);
        assert_eq!(st.task().as_deref(), Some("Tambah tombol"));
    }

    #[test]
    fn first_string_prompt_becomes_the_task_without_an_ai_title() {
        let st = advance_lines(&[r#"{"type":"user","message":{"content":"  tambah  tombol \n di header "}}"#]);
        assert_eq!(st.task().as_deref(), Some("tambah tombol di header"));
    }

    #[test]
    fn first_text_block_of_an_array_prompt_becomes_the_task() {
        let st = advance_lines(&[
            r#"{"type":"user","message":{"content":[{"type":"text","text":"apa ini?\njelaskan"}]}}"#,
        ]);
        assert_eq!(st.task().as_deref(), Some("apa ini? jelaskan"));
    }

    #[test]
    fn only_the_first_prompt_is_kept() {
        let st = advance_lines(&[
            r#"{"type":"user","message":{"content":"pertama"}}"#,
            r#"{"type":"user","message":{"content":"kedua"}}"#,
        ]);
        assert_eq!(st.task().as_deref(), Some("pertama"));
    }

    #[test]
    fn a_tool_result_is_not_a_prompt() {
        let st = advance_lines(&[
            "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"t1\"}]}}",
        ]);
        assert_eq!(st.task(), None);
    }

    #[test]
    fn command_and_system_prompts_are_skipped() {
        for line in [
            r#"{"type":"user","message":{"content":"<command-name>/clear</command-name>"}}"#,
            r#"{"type":"user","message":{"content":"<local-command-stdout>x</local-command-stdout>"}}"#,
            r#"{"type":"user","message":{"content":"<system-reminder>hi</system-reminder>"}}"#,
            r#"{"type":"user","message":{"content":"Caveat: the messages below"}}"#,
        ] {
            assert_eq!(advance_lines(&[line]).task(), None, "{line}");
        }
    }

    #[test]
    fn a_meta_record_is_not_a_prompt() {
        let st = advance_lines(&[r#"{"type":"user","isMeta":true,"message":{"content":"caveat-ish"}}"#]);
        assert_eq!(st.task(), None);
    }

    #[test]
    fn a_skipped_line_lets_a_later_real_prompt_through() {
        let st = advance_lines(&[
            r#"{"type":"user","message":{"content":"<system-reminder>ctx</system-reminder>"}}"#,
            r#"{"type":"user","message":{"content":"yang asli"}}"#,
        ]);
        assert_eq!(st.task().as_deref(), Some("yang asli"));
    }

    #[test]
    fn a_long_task_is_cut_and_marked() {
        let long = "a".repeat(200);
        let st = advance_lines(&[&format!(r#"{{"type":"user","message":{{"content":"{long}"}}}}"#)]);
        let task = st.task().unwrap();
        assert_eq!(task.chars().count(), 121);
        assert!(task.ends_with('…'));
    }
}
