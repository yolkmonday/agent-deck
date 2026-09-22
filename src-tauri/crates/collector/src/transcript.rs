use crate::model::TokenUsage;
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const CHUNK: u64 = 16 * 1024 * 1024;
const DETAIL_MAX: usize = 80;

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
        match v.get("type").and_then(Value::as_str) {
            Some("assistant") => self.apply_assistant(v),
            Some("user") => self.apply_user(v),
            Some("system") => {
                if v.get("subtype").and_then(Value::as_str) == Some("turn_duration") {
                    self.turn_ended = true;
                    self.pending_tool_id = None;
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
            self.last_tool = Some(ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                detail: tool_detail(item.get("input")),
            });
        }
    }

    fn apply_user(&mut self, v: &Value) {
        self.turn_ended = false;
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
            }
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
}
