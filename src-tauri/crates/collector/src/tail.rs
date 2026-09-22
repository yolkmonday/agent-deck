use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// A transcript on this machine reaches 130 MB. Only the last window is ever read,
/// so the cost of opening a viewer does not grow with the age of the session.
pub const TAIL_BYTES: u64 = 256 * 1024;
const TEXT_MAX: usize = 4000;
const PREVIEW_MAX: usize = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolStatus {
    Running,
    Ok,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TailEntry {
    #[serde(rename_all = "camelCase")]
    User { ms: i64, text: String },
    #[serde(rename_all = "camelCase")]
    Assistant {
        ms: i64,
        text: String,
        model: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Thinking { ms: i64, text: String },
    #[serde(rename_all = "camelCase")]
    Tool {
        ms: i64,
        name: String,
        input: String,
        status: ToolStatus,
    },
    #[serde(rename_all = "camelCase")]
    Result {
        ms: i64,
        tool_name: String,
        preview: String,
        is_error: bool,
    },
}

/// A parsed entry plus the tool it belongs to. The id is what links a `tool_use`
/// to its `tool_result`, which may sit before the window for the former or after
/// it for the latter.
struct Raw {
    tool_id: Option<String>,
    entry: TailEntry,
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

fn ms_of(v: &Value) -> i64 {
    // Claude writes `timestamp` as an ISO 8601 string, never as a number.
    v.get("timestamp")
        .and_then(|t| match t {
            Value::String(s) => crate::indexer::parse_iso_ms(s),
            other => other.as_i64(),
        })
        .unwrap_or(0)
}

fn text_block(item: &Value) -> Option<&str> {
    item.get("text")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
}

/// A `user` record carries either a plain string or content blocks. Only a text
/// block makes it a user turn; a record holding nothing but `tool_result` is the
/// wrapper for a `result` entry and must not be shown twice.
fn user_text(v: &Value) -> Option<String> {
    match v.get("message").and_then(|m| m.get("content")) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Array(items)) => {
            let joined = items
                .iter()
                .filter(|i| i.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(text_block)
                .collect::<Vec<_>>()
                .join("\n");
            (!joined.is_empty()).then_some(joined)
        }
        _ => None,
    }
}

fn result_preview(item: &Value) -> String {
    match item.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|i| match i.get("type").and_then(Value::as_str) {
                Some("text") => i.get("text").and_then(Value::as_str).map(str::to_string),
                Some("image") => Some("[image]".to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn push_tool_result(
    ms: i64,
    item: &Value,
    raws: &mut Vec<Raw>,
    errors: &mut HashMap<String, bool>,
) {
    let id = item.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
    let is_error = item.get("is_error").and_then(Value::as_bool).unwrap_or(false);
    if !id.is_empty() {
        errors.insert(id.to_string(), is_error);
    }
    raws.push(Raw {
        tool_id: (!id.is_empty()).then(|| id.to_string()),
        entry: TailEntry::Result {
            ms,
            tool_name: String::new(),
            preview: truncate(&result_preview(item), PREVIEW_MAX),
            is_error,
        },
    });
}

fn collect_assistant(v: &Value, ms: i64, raws: &mut Vec<Raw>, names: &mut HashMap<String, String>) {
    let Some(msg) = v.get("message") else { return };
    let model = msg
        .get("model")
        .and_then(Value::as_str)
        .filter(|m| *m != "<synthetic>")
        .map(str::to_string);
    let Some(items) = msg.get("content").and_then(Value::as_array) else { return };
    for item in items {
        match item.get("type").and_then(Value::as_str) {
            Some("text") => {
                let Some(text) = text_block(item) else { continue };
                raws.push(Raw {
                    tool_id: None,
                    entry: TailEntry::Assistant {
                        ms,
                        text: truncate(text, TEXT_MAX),
                        model: model.clone(),
                    },
                });
            }
            Some("thinking") => {
                let text = item
                    .get("thinking")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("text").and_then(Value::as_str))
                    .unwrap_or("");
                if text.is_empty() {
                    continue;
                }
                raws.push(Raw {
                    tool_id: None,
                    entry: TailEntry::Thinking {
                        ms,
                        text: truncate(text, TEXT_MAX),
                    },
                });
            }
            Some("tool_use") => {
                let name = item.get("name").and_then(Value::as_str).unwrap_or("");
                let id = item.get("id").and_then(Value::as_str).unwrap_or("");
                if !id.is_empty() {
                    names.insert(id.to_string(), name.to_string());
                }
                let input = item
                    .get("input")
                    .map(|i| serde_json::to_string(i).unwrap_or_default())
                    .unwrap_or_default();
                raws.push(Raw {
                    tool_id: (!id.is_empty()).then(|| id.to_string()),
                    entry: TailEntry::Tool {
                        ms,
                        name: name.to_string(),
                        input: truncate(&input, PREVIEW_MAX),
                        status: ToolStatus::Running,
                    },
                });
            }
            _ => {}
        }
    }
}

fn collect_user(v: &Value, ms: i64, raws: &mut Vec<Raw>, errors: &mut HashMap<String, bool>) {
    if let Some(text) = user_text(v) {
        raws.push(Raw {
            tool_id: None,
            entry: TailEntry::User {
                ms,
                text: truncate(&text, TEXT_MAX),
            },
        });
    }
    let Some(items) = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    else {
        return;
    };
    for item in items.iter().filter(|i| i.get("type").and_then(Value::as_str) == Some("tool_result")) {
        push_tool_result(ms, item, raws, errors);
    }
}

/// Reads at most the last [`TAIL_BYTES`] of `path` and returns the last
/// `max_entries` interesting records, oldest first, with the file's length. The
/// window usually opens mid-record, so everything before the first newline is
/// dropped rather than parsed as a broken line.
pub fn read_tail(path: &Path, max_entries: usize) -> Result<(Vec<TailEntry>, u64)> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    let body_start = if start > 0 {
        buf.iter()
            .position(|b| *b == b'\n')
            .map(|i| i + 1)
            .unwrap_or(buf.len())
    } else {
        0
    };
    // A line without its trailing newline may still be being written.
    let body_end = buf
        .iter()
        .rposition(|b| *b == b'\n')
        .map(|i| i + 1)
        .unwrap_or(0);

    let mut raws: Vec<Raw> = Vec::new();
    let mut names: HashMap<String, String> = HashMap::new();
    let mut errors: HashMap<String, bool> = HashMap::new();
    if body_start < body_end {
        for line in buf[body_start..body_end].split(|b| *b == b'\n') {
            if line.is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_slice::<Value>(line) else { continue };
            let ms = ms_of(&v);
            match v.get("type").and_then(Value::as_str) {
                Some("assistant") => collect_assistant(&v, ms, &mut raws, &mut names),
                Some("user") => collect_user(&v, ms, &mut raws, &mut errors),
                _ => {}
            }
        }
    }

    let mut entries: Vec<TailEntry> = Vec::with_capacity(raws.len());
    for raw in raws {
        entries.push(match raw.entry {
            TailEntry::Tool { ms, name, input, .. } => {
                let status = raw
                    .tool_id
                    .as_ref()
                    .and_then(|id| errors.get(id))
                    .map(|e| if *e { ToolStatus::Error } else { ToolStatus::Ok })
                    .unwrap_or(ToolStatus::Running);
                TailEntry::Tool { ms, name, input, status }
            }
            TailEntry::Result { ms, preview, is_error, .. } => {
                // The `tool_use` this answers may be older than the window, in
                // which case the id is all we can honestly show.
                let tool_name = raw
                    .tool_id
                    .as_ref()
                    .and_then(|id| names.get(id).cloned())
                    .or_else(|| raw.tool_id.clone())
                    .unwrap_or_default();
                TailEntry::Result { ms, tool_name, preview, is_error }
            }
            other => other,
        });
    }
    if entries.len() > max_entries {
        entries.drain(..entries.len() - max_entries);
    }
    Ok((entries, len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const USER: &str = r#"{"type":"user","message":{"role":"user","content":"tolong cek build"},"uuid":"u1","timestamp":"2026-09-22T07:38:37.824Z"}"#;
    const ASSISTANT: &str = r#"{"type":"assistant","message":{"id":"msg_1","model":"claude-sonnet-5","content":[{"type":"text","text":"Baik, saya cek sekarang."}]},"uuid":"a1","timestamp":"2026-09-22T07:38:40.100Z"}"#;
    const TOOL_USE: &str = r#"{"type":"assistant","message":{"id":"msg_2","model":"claude-sonnet-5","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"bun test"}}]},"uuid":"a2","timestamp":"2026-09-22T07:38:41.000Z"}"#;
    const TOOL_RESULT: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"3 pass\n","is_error":false}]},"uuid":"u2","timestamp":"2026-09-22T07:38:42.000Z"}"#;
    const TOOL_USE_FAIL: &str = r#"{"type":"assistant","message":{"id":"msg_3","model":"claude-sonnet-5","content":[{"type":"tool_use","id":"toolu_2","name":"Read","input":{"file_path":"/tmp/x"}}]},"uuid":"a3","timestamp":"2026-09-22T07:38:42.500Z"}"#;
    const TOOL_RESULT_ERROR: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_2","content":"boom","is_error":true}]},"uuid":"u3","timestamp":"2026-09-22T07:38:43.000Z"}"#;

    fn write(path: &Path, s: &str) {
        std::fs::write(path, s).unwrap();
    }

    fn ms(iso: &str) -> i64 {
        crate::indexer::parse_iso_ms(iso).unwrap()
    }

    #[test]
    fn reads_user_and_assistant_text() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        write(&p, &format!("{USER}\n{ASSISTANT}\n"));

        let (entries, size) = read_tail(&p, 60).unwrap();
        assert_eq!(size, std::fs::metadata(&p).unwrap().len());
        assert_eq!(entries.len(), 2);
        match &entries[0] {
            TailEntry::User { ms: t, text } => {
                assert_eq!(*t, ms("2026-09-22T07:38:37.824Z"));
                assert_eq!(text, "tolong cek build");
            }
            other => panic!("expected user, got {other:?}"),
        }
        match &entries[1] {
            TailEntry::Assistant { ms: t, text, model } => {
                assert_eq!(*t, ms("2026-09-22T07:38:40.100Z"));
                assert_eq!(text, "Baik, saya cek sekarang.");
                assert_eq!(model.as_deref(), Some("claude-sonnet-5"));
            }
            other => panic!("expected assistant, got {other:?}"),
        }
    }

    #[test]
    fn maps_tool_use_and_tool_result() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        write(&p, &format!("{TOOL_USE}\n{TOOL_RESULT}\n{TOOL_USE_FAIL}\n{TOOL_RESULT_ERROR}\n"));

        let (entries, _) = read_tail(&p, 60).unwrap();
        assert_eq!(entries.len(), 4);

        match &entries[0] {
            TailEntry::Tool { name, input, status, .. } => {
                assert_eq!(name, "Bash");
                assert!(input.contains("bun test"), "input was {input}");
                assert_eq!(*status, ToolStatus::Ok);
            }
            other => panic!("expected tool, got {other:?}"),
        }
        match &entries[1] {
            TailEntry::Result { tool_name, preview, is_error, .. } => {
                assert_eq!(tool_name, "Bash");
                assert_eq!(preview, "3 pass\n");
                assert!(!is_error);
            }
            other => panic!("expected result, got {other:?}"),
        }
        match &entries[2] {
            TailEntry::Tool { status, .. } => assert_eq!(*status, ToolStatus::Error),
            other => panic!("expected tool, got {other:?}"),
        }
        match &entries[3] {
            TailEntry::Result { is_error, .. } => assert!(is_error),
            other => panic!("expected result, got {other:?}"),
        }
    }

    #[test]
    fn skips_a_user_record_that_is_only_a_tool_result_wrapper() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        write(&p, &format!("{TOOL_USE}\n{TOOL_RESULT}\n"));

        let (entries, _) = read_tail(&p, 60).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[0], TailEntry::Tool { .. }));
        assert!(matches!(entries[1], TailEntry::Result { .. }));
        assert!(!entries.iter().any(|e| matches!(e, TailEntry::User { .. })));
    }

    #[test]
    fn keeps_only_the_last_max_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        let mut body = String::new();
        for i in 0..50 {
            body.push_str(&format!(
                "{{\"type\":\"assistant\",\"message\":{{\"id\":\"m{i}\",\"model\":\"claude-sonnet-5\",\"content\":[{{\"type\":\"text\",\"text\":\"m{i}\"}}]}},\"timestamp\":\"2026-09-22T07:38:37.824Z\"}}\n"
            ));
        }
        write(&p, &body);

        let (entries, _) = read_tail(&p, 10).unwrap();
        assert_eq!(entries.len(), 10);
        let texts: Vec<&str> = entries
            .iter()
            .map(|e| match e {
                TailEntry::Assistant { text, .. } => text.as_str(),
                other => panic!("expected assistant, got {other:?}"),
            })
            .collect();
        assert_eq!(texts.first().copied(), Some("m40"));
        assert_eq!(texts.last().copied(), Some("m49"));
    }

    #[test]
    fn reads_only_the_tail_of_a_large_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        let padding = "x".repeat(5 * 1024 * 1024);
        let mut body = format!(
            "{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"{padding}\"}},\"timestamp\":\"2026-09-22T07:00:00.000Z\"}}\n"
        );
        body.push_str(&format!("{USER}\n{ASSISTANT}\n{TOOL_USE}\n"));
        write(&p, &body);

        let started = std::time::Instant::now();
        let (entries, size) = read_tail(&p, 60).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(2));

        assert_eq!(size, body.len() as u64);
        assert_eq!(entries.len(), 3, "the padding record must be outside the window");
        assert!(matches!(entries[0], TailEntry::User { .. }));
        assert!(matches!(entries[1], TailEntry::Assistant { .. }));
        assert!(matches!(entries[2], TailEntry::Tool { .. }));
    }

    #[test]
    fn drops_a_partial_first_line() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        let padding = "y".repeat(TAIL_BYTES as usize + 4096);
        let mut body = format!(
            "{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"{padding}\"}},\"timestamp\":\"2026-09-22T07:00:00.000Z\"}}\n"
        );
        body.push_str(&format!("{ASSISTANT}\n{USER}\n"));
        write(&p, &body);

        let (entries, _) = read_tail(&p, 60).unwrap();
        assert_eq!(entries.len(), 2, "the cut record must not become a broken entry");
        match &entries[0] {
            TailEntry::Assistant { text, .. } => assert_eq!(text, "Baik, saya cek sekarang."),
            other => panic!("expected assistant, got {other:?}"),
        }
        match &entries[1] {
            TailEntry::User { text, .. } => assert_eq!(text, "tolong cek build"),
            other => panic!("expected user, got {other:?}"),
        }
    }

    #[test]
    fn truncates_long_text_with_an_ellipsis() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("s.jsonl");
        let long = "a".repeat(9000);
        write(
            &p,
            &format!(
                "{{\"type\":\"assistant\",\"message\":{{\"id\":\"m1\",\"model\":\"claude-sonnet-5\",\"content\":[{{\"type\":\"text\",\"text\":\"{long}\"}}]}},\"timestamp\":\"2026-09-22T07:38:37.824Z\"}}\n"
            ),
        );

        let (entries, _) = read_tail(&p, 60).unwrap();
        match &entries[0] {
            TailEntry::Assistant { text, .. } => {
                assert_eq!(text.chars().count(), 4001);
                assert!(text.ends_with('…'));
            }
            other => panic!("expected assistant, got {other:?}"),
        }
    }

    #[test]
    fn missing_file_is_an_error_not_a_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("nope.jsonl");
        assert!(read_tail(&p, 60).is_err());
    }
}
