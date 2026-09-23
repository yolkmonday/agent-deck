use crate::live::project_name;
use crate::model::{Agent, TokenUsage};
use crate::store::{FileProgress, MessageRow, PerfRow, SpanRow, Store};
use crate::transcript::tool_detail;
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const OBSERVER_MARKER: &str = "/.claude-mem/observer-sessions";
const READ_CHUNK: u64 = 16 * 1024 * 1024;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IndexReport {
    pub files_scanned: usize,
    pub files_skipped: usize,
    pub messages_upserted: usize,
    pub spans_upserted: usize,
    pub errors: Vec<String>,
}

pub fn parse_iso_ms(s: &str) -> Option<i64> {
    let dt = time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()?;
    i64::try_from(dt.unix_timestamp_nanos() / 1_000_000).ok()
}

fn file_meta(path: &Path) -> Option<(i64, i64)> {
    let m = std::fs::metadata(path).ok()?;
    let modified = m.modified().ok()?;
    let ms = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Some((ms, m.len() as i64))
}

fn read_new_lines(path: &Path, progress: Option<&FileProgress>) -> Option<(Vec<String>, i64, i64)> {
    let (mtime_ms, size) = file_meta(path)?;
    let start = match progress {
        Some(p) if size >= p.size => p.offset.max(0) as u64,
        _ => 0,
    };
    let mut file = File::open(path).ok()?;
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::new();
    let mut read_size = READ_CHUNK;
    let complete = loop {
        let mut chunk = Vec::new();
        (&mut file).take(read_size).read_to_end(&mut chunk).ok()?;
        buf.extend_from_slice(&chunk);
        if let Some(i) = buf.iter().rposition(|b| *b == b'\n') {
            break i + 1;
        }
        if (chunk.len() as u64) < read_size {
            break buf.len();
        }
        read_size = read_size.saturating_mul(2);
    };
    let lines = buf[..complete]
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| String::from_utf8_lossy(l).to_string())
        .collect::<Vec<_>>();
    Some((lines, mtime_ms, start as i64 + complete as i64))
}

fn flag_observer(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if s.contains(OBSERVER_MARKER) {
        return true;
    }
    // The encoded project dir collapses `/` and `.` to `-`, so the marker becomes
    // `-Users-yolk--claude-mem-observer-sessions`.
    s.contains("-claude-mem-observer-sessions")
}

/// Collects `*.jsonl` from a project dir, plus each session's `subagents/` folder.
fn jsonl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if flag_observer(dir) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if flag_observer(&p) {
            continue;
        }
        if p.is_dir() {
            continue;
        }
        if p.extension().and_then(|x| x.to_str()) == Some("jsonl") {
            out.push(p);
        }
    }
    // one level down: <sessionId>/subagents/agent-*.jsonl
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let session = e.path();
        if !session.is_dir() || flag_observer(&session) {
            continue;
        }
        let sub = session.join("subagents");
        let Ok(sub_entries) = std::fs::read_dir(&sub) else {
            continue;
        };
        for se in sub_entries.flatten() {
            let sp = se.path();
            if flag_observer(&sp) {
                continue;
            }
            if sp.extension().and_then(|x| x.to_str()) == Some("jsonl") {
                out.push(sp);
            }
        }
    }
}

fn claude_row(
    id: String,
    session_id: &str,
    project: &str,
    model: &str,
    ts_ms: i64,
    tokens: TokenUsage,
) -> Option<MessageRow> {
    let model = model.trim();
    if model.is_empty() || model == "<synthetic>" {
        return None;
    }
    Some(MessageRow {
        id,
        agent: Agent::Claude,
        session_id: session_id.to_string(),
        project: project.to_string(),
        model: model.to_string(),
        ts_ms,
        tokens,
    })
}

fn claude_usage(u: &Value) -> TokenUsage {
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

fn record_ts(v: &Value, fallback_ms: i64) -> i64 {
    v.get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_iso_ms)
        .unwrap_or(fallback_ms)
}

/// The usage row for an `assistant` record, or `None` when the record carries no
/// indexable usage (no id/model, or a synthetic model).
fn claude_message_row(
    v: &Value,
    session_id: &str,
    project: &str,
    fallback_ms: i64,
) -> Option<MessageRow> {
    let msg = v.get("message")?;
    let mid = msg.get("id").and_then(Value::as_str)?;
    let model = msg.get("model").and_then(Value::as_str)?;
    let usage = claude_usage(msg.get("usage")?);
    claude_row(
        format!("claude:{mid}"),
        session_id,
        project,
        model,
        record_ts(v, fallback_ms),
        usage,
    )
}

pub fn index_claude(store: &mut Store, projects_dir: &Path, home: &str) -> IndexReport {
    let mut report = IndexReport::default();
    if !projects_dir.exists() {
        return report;
    }
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(projects_dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                jsonl_files(&p, &mut files);
            }
        }
    }
    files.sort();

    for path in files {
        let key = path.to_string_lossy().to_string();
        report.files_scanned += 1;
        let Some((mtime_ms, size)) = file_meta(&path) else {
            report.errors.push(format!("stat failed: {key}"));
            continue;
        };
        let progress = store.file_progress(&key);
        if let Some(p) = &progress {
            if p.mtime_ms == mtime_ms && p.size == size {
                report.files_skipped += 1;
                report.files_scanned -= 1;
                continue;
            }
        }
        let Some((lines, mtime_ms, offset)) = read_new_lines(&path, progress.as_ref()) else {
            report.errors.push(format!("read failed: {key}"));
            continue;
        };

        let session_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        if let Some(p) = &progress {
            if size < p.size {
                // File was rewritten shorter: drop its rows before re-reading from 0.
                if let Err(e) = store.delete_session(Agent::Claude, &session_id) {
                    report.errors.push(format!("delete failed: {e}"));
                }
            }
        }
        let mut cwd = String::new();
        let mut rows: Vec<MessageRow> = Vec::new();
        let mut spans: Vec<SpanRow> = Vec::new();
        // tool_use id -> (index into `spans`, span) so a later tool_result in the
        // same pass can close it without a second lookup.
        let mut open: std::collections::HashMap<String, usize> = Default::default();
        // The most recent `user` record's timestamp (prompt or tool_result):
        // the request anchor for the next assistant response.
        let mut last_request_ts: Option<i64> = None;
        // message.id -> (start_ms, end_ms, output_tokens, model) for an
        // estimated response-speed sample, possibly spanning several
        // `assistant` records that share the same message.id.
        let mut perf_open: std::collections::HashMap<String, (i64, i64, i64, String)> =
            Default::default();
        for v in lines
            .iter()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        {
            if let Some(c) = v.get("cwd").and_then(Value::as_str) {
                cwd = c.to_string();
            }
            let project = if cwd.is_empty() {
                "unknown".to_string()
            } else {
                project_name(&cwd, home)
            };
            match v.get("type").and_then(Value::as_str) {
                Some("assistant") => {
                    if let Some(r) = claude_message_row(&v, &session_id, &project, mtime_ms) {
                        rows.push(r);
                    }
                    let Some(msg) = v.get("message") else {
                        continue;
                    };
                    if let (Some(mid), Some(model)) = (
                        msg.get("id").and_then(Value::as_str),
                        msg.get("model").and_then(Value::as_str),
                    ) {
                        let ts = record_ts(&v, mtime_ms);
                        let out = msg
                            .get("usage")
                            .and_then(|u| u.get("output_tokens"))
                            .and_then(Value::as_u64)
                            .unwrap_or(0) as i64;
                        if let Some(entry) = perf_open.get_mut(mid) {
                            entry.1 = entry.1.max(ts);
                            entry.2 = out;
                        } else if let Some(start) = last_request_ts {
                            perf_open.insert(mid.to_string(), (start, ts, out, model.to_string()));
                        }
                    }
                    let Some(items) = msg.get("content").and_then(Value::as_array) else {
                        continue;
                    };
                    let tokens = msg.get("usage").map(claude_usage);
                    for item in items
                        .iter()
                        .filter(|i| i.get("type").and_then(Value::as_str) == Some("tool_use"))
                    {
                        let (Some(id), Some(name)) = (
                            item.get("id").and_then(Value::as_str),
                            item.get("name").and_then(Value::as_str),
                        ) else {
                            continue;
                        };
                        let ts = record_ts(&v, mtime_ms);
                        open.insert(id.to_string(), spans.len());
                        spans.push(SpanRow {
                            id: format!("claude:{id}"),
                            agent: Agent::Claude,
                            session_id: session_id.clone(),
                            project: project.clone(),
                            model: msg
                                .get("model")
                                .and_then(Value::as_str)
                                .filter(|m| !m.is_empty() && *m != "<synthetic>")
                                .map(str::to_string),
                            tool: name.to_string(),
                            detail: tool_detail(item.get("input")),
                            start_ms: ts,
                            end_ms: None,
                            status: "running".into(),
                            tokens: tokens.clone(),
                        });
                    }
                }
                Some("user") => {
                    // Every `user` record (a prompt or a tool_result) is the
                    // request anchor for whichever `assistant` response follows.
                    let ts = record_ts(&v, mtime_ms);
                    last_request_ts = Some(ts);
                    let Some(items) = v
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(Value::as_array)
                    else {
                        continue;
                    };
                    for item in items
                        .iter()
                        .filter(|i| i.get("type").and_then(Value::as_str) == Some("tool_result"))
                    {
                        let Some(id) = item.get("tool_use_id").and_then(Value::as_str) else {
                            continue;
                        };
                        let end_ms = Some(ts);
                        let status = if item.get("is_error").and_then(Value::as_bool) == Some(true)
                        {
                            "error"
                        } else {
                            "ok"
                        };
                        if let Some(i) = open.remove(id) {
                            spans[i].end_ms = end_ms;
                            spans[i].status = status.to_string();
                            continue;
                        }
                        // The tool_use was seen in an earlier pass: merge into the
                        // stored row. An orphan result (no opening row) is ignored.
                        let key = format!("claude:{id}");
                        if let Ok(Some(mut existing)) = store.span_by_id(&key) {
                            existing.end_ms = end_ms;
                            existing.status = status.to_string();
                            spans.push(existing);
                        }
                    }
                }
                _ => {}
            }
        }

        match store.upsert_messages(&rows) {
            Ok(n) => report.messages_upserted += n,
            Err(e) => report.errors.push(format!("upsert failed: {e}")),
        }
        match store.upsert_spans(&spans) {
            Ok(n) => report.spans_upserted += n,
            Err(e) => report.errors.push(format!("span upsert failed: {e}")),
        }
        let perf_rows: Vec<PerfRow> = perf_open
            .into_iter()
            .map(|(mid, (start, end, out, model))| PerfRow {
                id: format!("claude:{mid}"),
                agent: Agent::Claude,
                session_id: session_id.clone(),
                model,
                start_ms: start,
                end_ms: end,
                gen_ms: end - start,
                output_tokens: out,
                ttft_ms: None,
                precise: false,
            })
            .collect();
        if let Err(e) = store.upsert_perf(&perf_rows) {
            report.errors.push(format!("perf upsert failed: {e}"));
        }
        if let Err(e) = store.set_file_progress(&FileProgress {
            path: key,
            mtime_ms,
            size,
            offset,
        }) {
            report.errors.push(format!("progress failed: {e}"));
        }
    }
    report
}

fn codex_token_usage(v: &Value) -> TokenUsage {
    let get = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
    TokenUsage {
        input: get("input_tokens"),
        output: get("output_tokens"),
        cache_read: get("cached_input_tokens"),
        cache_write: get("cache_write_input_tokens"),
        reasoning: get("reasoning_output_tokens"),
    }
}

pub fn index_codex(store: &mut Store, sessions_dir: &Path, home: &str) -> IndexReport {
    let mut report = IndexReport::default();
    if !sessions_dir.exists() {
        return report;
    }
    let mut files = Vec::new();
    let mut stack = vec![sessions_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if flag_observer(&p) {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
                .unwrap_or(false)
            {
                files.push(p);
            }
        }
    }
    files.sort();

    for path in files {
        let key = path.to_string_lossy().to_string();
        report.files_scanned += 1;
        let Some((mtime_ms, size)) = file_meta(&path) else {
            report.errors.push(format!("stat failed: {key}"));
            continue;
        };
        let progress = store.file_progress(&key);
        if let Some(p) = &progress {
            if p.mtime_ms == mtime_ms && p.size == size {
                report.files_skipped += 1;
                report.files_scanned -= 1;
                continue;
            }
        }
        let Some((lines, mtime_ms, offset)) = read_new_lines(&path, progress.as_ref()) else {
            report.errors.push(format!("read failed: {key}"));
            continue;
        };

        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.trim_start_matches("rollout-").to_string())
            .unwrap_or_default();
        let mut model = String::new();
        let mut cwd = String::new();
        let mut ordinal: i64 = 0;
        let mut rows = Vec::new();
        for line in &lines {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let payload = v.get("payload").unwrap_or(&Value::Null);
            match v.get("type").and_then(Value::as_str) {
                Some("session_meta") => {
                    if let Some(c) = payload.get("cwd").and_then(Value::as_str) {
                        cwd = c.to_string();
                    }
                }
                Some("turn_context") => {
                    if let Some(c) = payload.get("cwd").and_then(Value::as_str) {
                        cwd = c.to_string();
                    }
                    if let Some(m) = payload.get("model").and_then(Value::as_str) {
                        model = m.to_string();
                    }
                }
                Some("event_msg") => {
                    if payload.get("type").and_then(Value::as_str) != Some("token_count") {
                        continue;
                    }
                    ordinal += 1;
                    let Some(usage) = payload.get("info").and_then(|i| i.get("last_token_usage"))
                    else {
                        continue;
                    };
                    if model.is_empty() {
                        continue;
                    }
                    let ts = v
                        .get("timestamp")
                        .and_then(Value::as_str)
                        .and_then(parse_iso_ms)
                        .unwrap_or(mtime_ms);
                    let project = if cwd.is_empty() {
                        "unknown".to_string()
                    } else {
                        project_name(&cwd, home)
                    };
                    rows.push(MessageRow {
                        id: format!("codex:{stem}:{ordinal}"),
                        agent: Agent::Codex,
                        session_id: stem.clone(),
                        project,
                        model: model.clone(),
                        ts_ms: ts,
                        tokens: codex_token_usage(usage),
                    });
                }
                _ => {}
            }
        }

        match store.upsert_messages(&rows) {
            Ok(n) => report.messages_upserted += n,
            Err(e) => report.errors.push(format!("upsert failed: {e}")),
        }
        if let Err(e) = store.set_file_progress(&FileProgress {
            path: key,
            mtime_ms,
            size,
            offset,
        }) {
            report.errors.push(format!("progress failed: {e}"));
        }
    }
    report
}

/// Tool-call parts newer than `since`, as spans. `detail` is deliberately never
/// filled: an opencode tool part carries its command text under `state.input`,
/// and that must not reach the index.
fn upsert_opencode_spans(
    store: &mut Store,
    conn: &rusqlite::Connection,
    since: i64,
    home: &str,
) -> anyhow::Result<usize> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.time_created, p.data, s.directory, s.id \
         FROM part p JOIN session s ON s.id = p.session_id \
         WHERE json_extract(p.data, '$.type') = 'tool' AND p.time_created > ?1 \
         ORDER BY p.time_created ASC",
    )?;
    let rows = stmt.query_map([since], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;

    let mut spans = Vec::new();
    for (id, created, data, directory, session_id) in rows.flatten() {
        let Ok(v) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        let state = v.get("state").unwrap_or(&Value::Null);
        let tool = v.get("tool").and_then(Value::as_str).unwrap_or("tool");
        let time = state.get("time").unwrap_or(&Value::Null);
        let start_ms = time.get("start").and_then(Value::as_i64).unwrap_or(created);
        let end_ms = time.get("end").and_then(Value::as_i64);
        let status = match state.get("status").and_then(Value::as_str) {
            Some("completed") => "ok",
            Some("error") => "error",
            _ => "running",
        };
        spans.push(SpanRow {
            id: format!("opencode:{id}"),
            agent: Agent::Opencode,
            session_id,
            project: project_name(&directory, home),
            model: None,
            tool: tool.to_string(),
            detail: None,
            start_ms,
            end_ms,
            status: status.to_string(),
            tokens: None,
        });
    }
    store.upsert_spans(&spans)
}

/// Model string for an opencode assistant message, following the same
/// `providerID`/`modelID` rule as the message pass. `None` when neither field
/// is usable.
fn opencode_message_model(msg_data: &Value) -> Option<String> {
    match (
        msg_data.get("providerID").and_then(Value::as_str),
        msg_data.get("modelID").and_then(Value::as_str),
    ) {
        (Some(p), Some(m)) => Some(format!("{p}/{m}")),
        (None, Some(m)) => Some(m.to_string()),
        _ => None,
    }
}

/// Precise per-step generation timing from opencode step-start/step-finish
/// parts. Uses a 1h lookback (`since - 3_600_000`, clamped at 0) rather than
/// `since` directly: the message pass advances `since` past a message's
/// `time_created`, so a message still streaming when first indexed would
/// otherwise never get its later steps. Re-reading is safe because rows
/// upsert idempotently by id.
fn upsert_opencode_perf(
    store: &mut Store,
    conn: &rusqlite::Connection,
    since: i64,
    report: &mut IndexReport,
) {
    let lookback = (since - 3_600_000).max(0);
    let Ok(mut stmt) = conn.prepare(
        "SELECT p.id, p.message_id, p.session_id, p.time_created, p.data, m.data \
         FROM part p JOIN message m ON m.id = p.message_id \
         WHERE m.time_created > ?1 ORDER BY p.message_id, p.time_created, p.id",
    ) else {
        report.errors.push("perf query failed".to_string());
        return;
    };
    let rows = stmt.query_map([lookback], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
        ))
    });
    let Ok(rows) = rows else {
        report.errors.push("perf query failed".to_string());
        return;
    };

    let mut out = Vec::new();
    let mut cur_msg: Option<String> = None;
    let mut model: Option<String> = None;
    let mut step_start: Option<i64> = None;
    let mut first: Option<i64> = None;
    let mut gen: i64 = 0;

    for (part_id, message_id, session_id, created, data, msg_data) in rows.flatten() {
        if cur_msg.as_deref() != Some(message_id.as_str()) {
            cur_msg = Some(message_id);
            step_start = None;
            first = None;
            gen = 0;
            model = serde_json::from_str::<Value>(&msg_data)
                .ok()
                .as_ref()
                .and_then(opencode_message_model);
        }
        let Ok(v) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        match v.get("type").and_then(Value::as_str) {
            Some("step-start") => {
                step_start = Some(created);
                first = None;
                gen = 0;
            }
            Some("text") | Some("reasoning") => {
                let Some(time) = v.get("time") else { continue };
                let start = time.get("start").and_then(Value::as_i64);
                let end = time.get("end").and_then(Value::as_i64);
                if let (Some(start), Some(end)) = (start, end) {
                    gen += (end - start).max(0);
                }
                if let Some(start) = start {
                    first = Some(first.map_or(start, |f| f.min(start)));
                }
            }
            Some("tool") => {
                let tool_start = v
                    .get("state")
                    .and_then(|s| s.get("time"))
                    .and_then(|t| t.get("start"))
                    .and_then(Value::as_i64);
                if let Some(tool_start) = tool_start {
                    gen += (tool_start - created).max(0);
                }
                first = Some(first.map_or(created, |f| f.min(created)));
            }
            Some("step-finish") => {
                if gen > 0 {
                    if let Some(model) = &model {
                        let tokens = v.get("tokens").cloned().unwrap_or(Value::Null);
                        let output = tokens.get("output").and_then(Value::as_i64).unwrap_or(0);
                        let reasoning =
                            tokens.get("reasoning").and_then(Value::as_i64).unwrap_or(0);
                        out.push(PerfRow {
                            id: format!("opencode:{part_id}"),
                            agent: Agent::Opencode,
                            session_id,
                            model: model.clone(),
                            start_ms: step_start.unwrap_or(created),
                            end_ms: created,
                            gen_ms: gen,
                            output_tokens: output + reasoning,
                            ttft_ms: step_start
                                .zip(first)
                                .map(|(start, f)| (f - start).max(0)),
                            precise: true,
                        });
                    }
                }
                step_start = None;
                first = None;
                gen = 0;
            }
            _ => {}
        }
    }

    match store.upsert_perf(&out) {
        Ok(_) => {}
        Err(e) => report.errors.push(format!("perf upsert failed: {e}")),
    }
}

/// Walks the opencode SQLite DB read-only and indexes assistant messages.
pub fn index_opencode(store: &mut Store, db: &Path, home: &str) -> IndexReport {
    let mut report = IndexReport::default();
    if !db.exists() {
        return report;
    }
    let key = db.to_string_lossy().to_string();
    let Ok(conn) =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        report.errors.push(format!("open failed: {key}"));
        return report;
    };
    let _ = conn.busy_timeout(std::time::Duration::from_millis(500));

    let progress = store.file_progress(&key);
    let since = progress.as_ref().map(|p| p.offset).unwrap_or(0);
    let (mtime_ms, size) = file_meta(db).unwrap_or((0, 0));

    let Ok(mut stmt) = conn.prepare(
        "SELECT m.id, m.time_created, m.data, s.directory \
         FROM message m JOIN session s ON s.id = m.session_id \
         WHERE m.time_created > ?1 ORDER BY m.time_created ASC",
    ) else {
        report.errors.push(format!("query failed: {key}"));
        return report;
    };
    let rows = stmt.query_map([since], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    });
    let Ok(rows) = rows else {
        report.errors.push(format!("query failed: {key}"));
        return report;
    };

    report.files_scanned = 1;
    let mut out = Vec::new();
    let mut max_ts = since;
    for (id, ts, data, directory) in rows.flatten() {
        max_ts = max_ts.max(ts);
        let Ok(v) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        if v.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let model = match (
            v.get("providerID").and_then(Value::as_str),
            v.get("modelID").and_then(Value::as_str),
        ) {
            (Some(p), Some(m)) => format!("{p}/{m}"),
            (None, Some(m)) => m.to_string(),
            _ => continue,
        };
        let tokens = v.get("tokens").cloned().unwrap_or(Value::Null);
        let get = |k: &str| tokens.get(k).and_then(Value::as_u64).unwrap_or(0);
        let cache = tokens.get("cache").cloned().unwrap_or(Value::Null);
        let cache_get = |k: &str| cache.get(k).and_then(Value::as_u64).unwrap_or(0);
        out.push(MessageRow {
            id: format!("opencode:{id}"),
            agent: Agent::Opencode,
            session_id: id.clone(),
            project: project_name(&directory, home),
            model,
            ts_ms: ts,
            tokens: TokenUsage {
                input: get("input"),
                output: get("output"),
                reasoning: get("reasoning"),
                cache_read: cache_get("read"),
                cache_write: cache_get("write"),
            },
        });
    }

    match store.upsert_messages(&out) {
        Ok(n) => report.messages_upserted += n,
        Err(e) => report.errors.push(format!("upsert failed: {e}")),
    }
    upsert_opencode_perf(store, &conn, since, &mut report);
    match upsert_opencode_spans(store, &conn, since, home) {
        Ok(n) => report.spans_upserted += n,
        Err(e) => report.errors.push(format!("span upsert failed: {e}")),
    }
    if let Err(e) = store.set_file_progress(&FileProgress {
        path: key,
        mtime_ms,
        size,
        offset: max_ts,
    }) {
        report.errors.push(format!("progress failed: {e}"));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use crate::transcript::encode_cwd;
    use std::io::Write;
    fn append(path: &Path, s: &str) {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        f.write_all(s.as_bytes()).unwrap();
    }

    fn home() -> &'static str {
        "/Users/yolk"
    }

    fn claude_assistant(mid: &str, cwd: &str, model: &str, output: u64) -> String {
        format!(
            r#"{{"type":"assistant","cwd":"{cwd}","timestamp":"2026-09-21T09:42:37.055Z","message":{{"id":"{mid}","model":"{model}","usage":{{"input_tokens":10,"output_tokens":{output},"cache_read_input_tokens":100,"cache_creation_input_tokens":1000,"output_tokens_details":{{"thinking_tokens":5}}}}}}}}"#
        )
    }

    fn claude_tool_use(mid: &str, tool_id: &str, name: &str, input: &str) -> String {
        let input = serde_json::to_string(input).unwrap();
        format!(
            r#"{{"type":"assistant","cwd":"/Users/yolk/Dev/kirimi","timestamp":"2026-09-21T09:42:37.055Z","message":{{"id":"{mid}","model":"claude-sonnet-5","usage":{{"input_tokens":10,"output_tokens":50,"cache_read_input_tokens":100,"cache_creation_input_tokens":1000}},"content":[{{"type":"tool_use","id":"{tool_id}","name":"{name}","input":{{"command":{input}}}}}]}}}}"#
        )
    }

    fn claude_tool_result(tool_id: &str, is_error: bool) -> String {
        format!(
            r#"{{"type":"user","timestamp":"2026-09-21T09:42:37.055Z","message":{{"content":[{{"type":"tool_result","tool_use_id":"{tool_id}","is_error":{is_error}}}]}}}}"#
        )
    }

    /// A plain user prompt record (not a tool_result) at an explicit timestamp.
    fn user_prompt_at(ts: &str) -> String {
        format!(r#"{{"type":"user","cwd":"/tmp/proj","timestamp":"{ts}","message":{{"content":"hi"}}}}"#)
    }

    /// One `assistant` chunk of a (possibly split) message, at an explicit
    /// timestamp and cumulative `usage.output_tokens`.
    fn assistant_chunk_at(mid: &str, ts: &str, output: u64) -> String {
        format!(
            r#"{{"type":"assistant","cwd":"/tmp/proj","timestamp":"{ts}","message":{{"id":"{mid}","model":"claude-opus-4-8","usage":{{"input_tokens":10,"output_tokens":{output},"cache_read_input_tokens":100,"cache_creation_input_tokens":1000,"output_tokens_details":{{"thinking_tokens":5}}}}}}}}"#
        )
    }

    /// Writes `records` as a single session file under one project dir and
    /// returns the tempdir (keep it alive) plus the `projects` root.
    fn claude_projects_with(records: &[String]) -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let dir = projects.join(encode_cwd("/tmp/proj"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("s1.jsonl"), format!("{}\n", records.join("\n"))).unwrap();
        (tmp, projects)
    }

    fn store_at(dir: &Path) -> Store {
        Store::open(&dir.join("agent-deck.db")).unwrap()
    }

    #[test]
    fn parse_iso_ms_handles_utc_and_offset() {
        // NB: the plan listed 1789033357055 here, but that constant is wrong for
        // 2026-09-21T09:42:37.055Z (verified against a reference epoch conversion).
        let expected = 1789983757055;
        assert_eq!(parse_iso_ms("2026-09-21T09:42:37.055Z"), Some(expected));
        assert_eq!(
            parse_iso_ms("2026-09-21T16:42:37.055+07:00"),
            Some(expected)
        );
        assert_eq!(parse_iso_ms("not a date"), None);
    }

    #[test]
    fn claude_indexes_assistant_usage_once_per_message_id() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let dir = projects.join(encode_cwd("/Users/yolk/Dev/kirimi"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("s1.jsonl"),
            format!(
                "{}\n{}\n",
                claude_assistant("m1", "/Users/yolk/Dev/kirimi", "claude-sonnet-5", 50),
                claude_assistant("m1", "/Users/yolk/Dev/kirimi", "claude-sonnet-5", 50)
            ),
        )
        .unwrap();

        let mut s = store_at(tmp.path());
        let r = index_claude(&mut s, &projects, home());
        assert_eq!(r.messages_upserted, 2);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(s.counts().unwrap().1, 1);
        let t = s.totals(0).unwrap();
        assert_eq!(t.messages, 1);
        assert_eq!(t.tokens.output, 50);
        let models = s.by_model(0).unwrap();
        assert_eq!(models[0].model, "claude-sonnet-5");
        assert_eq!(models[0].agent, Agent::Claude);
        let p = s.by_project(0).unwrap();
        assert_eq!(p[0].project, "kirimi");
    }

    #[test]
    fn claude_skips_observer_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let observer = projects.join(encode_cwd("/Users/yolk/.claude-mem/observer-sessions"));
        std::fs::create_dir_all(&observer).unwrap();
        std::fs::write(
            observer.join("o1.jsonl"),
            format!(
                "{}\n",
                claude_assistant(
                    "m1",
                    "/Users/yolk/.claude-mem/observer-sessions",
                    "claude-sonnet-5",
                    9
                )
            ),
        )
        .unwrap();

        let mut s = store_at(tmp.path());
        let r = index_claude(&mut s, &projects, home());
        assert_eq!(r.files_scanned, 0);
        assert_eq!(r.messages_upserted, 0);
        assert_eq!(s.counts().unwrap().1, 0);
    }

    #[test]
    fn claude_skips_unchanged_files_on_second_run() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let dir = projects.join(encode_cwd("/Users/yolk/Dev/kirimi"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("s1.jsonl"),
            format!(
                "{}\n",
                claude_assistant("m1", "/Users/yolk/Dev/kirimi", "claude-sonnet-5", 5)
            ),
        )
        .unwrap();

        let mut s = store_at(tmp.path());
        let first = index_claude(&mut s, &projects, home());
        assert_eq!(first.files_scanned, 1);
        assert_eq!(first.messages_upserted, 1);

        let second = index_claude(&mut s, &projects, home());
        assert_eq!(second.files_scanned, 0);
        assert!(second.files_skipped >= 1);
        assert_eq!(second.messages_upserted, 0);
        assert_eq!(s.counts().unwrap().1, 1);
    }

    #[test]
    fn claude_resumes_from_offset_when_file_grows() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let dir = projects.join(encode_cwd("/Users/yolk/Dev/kirimi"));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("s1.jsonl");
        std::fs::write(
            &file,
            format!(
                "{}\n",
                claude_assistant("m1", "/Users/yolk/Dev/kirimi", "claude-sonnet-5", 5)
            ),
        )
        .unwrap();

        let mut s = store_at(tmp.path());
        index_claude(&mut s, &projects, home());
        append(
            &file,
            &format!(
                "{}\n",
                claude_assistant("m2", "/Users/yolk/Dev/kirimi", "claude-sonnet-5", 7)
            ),
        );

        let second = index_claude(&mut s, &projects, home());
        assert_eq!(second.messages_upserted, 1);
        assert!(second.files_scanned >= 1);
        let t = s.totals(0).unwrap();
        assert_eq!(t.messages, 2);
        assert_eq!(t.tokens.output, 12);
    }

    #[test]
    fn claude_restarts_when_file_shrinks() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let dir = projects.join(encode_cwd("/Users/yolk/Dev/kirimi"));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("s1.jsonl");
        std::fs::write(
            &file,
            format!(
                "{}\n",
                claude_assistant("m1", "/Users/yolk/Dev/kirimi", "claude-sonnet-5", 500)
            ),
        )
        .unwrap();

        let mut s = store_at(tmp.path());
        index_claude(&mut s, &projects, home());
        assert_eq!(s.totals(0).unwrap().tokens.output, 500);

        std::fs::write(
            &file,
            format!(
                "{}\n",
                claude_assistant("m2", "/Users/yolk/Dev/kirimi", "claude-sonnet-5", 3)
            ),
        )
        .unwrap();
        index_claude(&mut s, &projects, home());

        let models = s.by_model(0).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].tokens.output, 3);
        assert_eq!(models[0].messages, 1);
    }

    #[test]
    fn claude_indexes_subagent_files() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let dir = projects.join(encode_cwd("/Users/yolk/Dev/submo"));
        let sub = dir.join("sess-1").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            dir.join("sess-1.jsonl"),
            format!(
                "{}\n",
                claude_assistant("m1", "/Users/yolk/Dev/submo", "claude-sonnet-5", 5)
            ),
        )
        .unwrap();
        std::fs::write(
            sub.join("agent-x.jsonl"),
            format!(
                "{}\n",
                claude_assistant("m2", "/Users/yolk/Dev/submo", "claude-haiku-4-5", 9)
            ),
        )
        .unwrap();

        let mut s = store_at(tmp.path());
        let r = index_claude(&mut s, &projects, home());
        assert_eq!(r.messages_upserted, 2);
        assert_eq!(s.counts().unwrap().1, 2);
        let models = s.by_model(0).unwrap();
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| m.model == "claude-haiku-4-5"));
    }

    #[test]
    fn claude_skips_records_without_model() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let dir = projects.join(encode_cwd("/Users/yolk/Dev/kirimi"));
        std::fs::create_dir_all(&dir).unwrap();
        let no_model = r#"{"type":"assistant","cwd":"/Users/yolk/Dev/kirimi","timestamp":"2026-09-21T09:42:37.055Z","message":{"id":"nm","usage":{"input_tokens":1,"output_tokens":1}}}"#;
        let synthetic = r#"{"type":"assistant","cwd":"/Users/yolk/Dev/kirimi","timestamp":"2026-09-21T09:42:37.055Z","message":{"id":"sy","model":"<synthetic>","usage":{"input_tokens":1,"output_tokens":1}}}"#;
        std::fs::write(
            dir.join("s1.jsonl"),
            format!(
                "{no_model}\n{synthetic}\n{}\n",
                claude_assistant("ok", "/Users/yolk/Dev/kirimi", "claude-sonnet-5", 4)
            ),
        )
        .unwrap();

        let mut s = store_at(tmp.path());
        let r = index_claude(&mut s, &projects, home());
        assert_eq!(r.messages_upserted, 1);
        assert_eq!(s.counts().unwrap().1, 1);
        assert_eq!(s.totals(0).unwrap().tokens.output, 4);
    }

    #[test]
    fn claude_span_opens_and_closes() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let dir = projects.join(encode_cwd("/Users/yolk/Dev/kirimi"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("s1.jsonl"),
            format!(
                "{}\n{}\n",
                claude_tool_use("m1", "t1", "Bash", "bun test"),
                claude_tool_result("t1", false)
            ),
        )
        .unwrap();

        let mut s = store_at(tmp.path());
        let r = index_claude(&mut s, &projects, home());
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(r.spans_upserted, 1);

        let spans = s.spans(0, i64::MAX).unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].id, "claude:t1");
        assert_eq!(spans[0].tool, "Bash");
        assert_eq!(spans[0].status, "ok");
        assert_eq!(spans[0].start_ms, 1789983757055);
        assert_eq!(spans[0].end_ms, Some(1789983757055));
    }

    #[test]
    fn claude_span_error_flag_sets_error_status() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let dir = projects.join(encode_cwd("/Users/yolk/Dev/kirimi"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("s1.jsonl"),
            format!(
                "{}\n{}\n",
                claude_tool_use("m1", "t1", "Bash", "false"),
                claude_tool_result("t1", true)
            ),
        )
        .unwrap();

        let mut s = store_at(tmp.path());
        index_claude(&mut s, &projects, home());
        let spans = s.spans(0, i64::MAX).unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].status, "error");
        assert!(spans[0].end_ms.is_some());
    }

    #[test]
    fn claude_unclosed_span_stays_running() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let dir = projects.join(encode_cwd("/Users/yolk/Dev/kirimi"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("s1.jsonl"),
            format!(
                "{}\n",
                claude_tool_use("m1", "t1", "Read", "/Users/yolk/Dev/kirimi/src/a.ts")
            ),
        )
        .unwrap();

        let mut s = store_at(tmp.path());
        index_claude(&mut s, &projects, home());
        let spans = s.spans(0, i64::MAX).unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].end_ms, None);
        assert_eq!(spans[0].status, "running");
    }

    #[test]
    fn claude_span_closed_in_a_later_pass() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let dir = projects.join(encode_cwd("/Users/yolk/Dev/kirimi"));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("s1.jsonl");
        std::fs::write(
            &file,
            format!("{}\n", claude_tool_use("m1", "t1", "Bash", "bun test")),
        )
        .unwrap();

        let mut s = store_at(tmp.path());
        index_claude(&mut s, &projects, home());
        assert_eq!(s.spans(0, i64::MAX).unwrap()[0].status, "running");

        append(&file, &format!("{}\n", claude_tool_result("t1", false)));
        index_claude(&mut s, &projects, home());

        let spans = s.spans(0, i64::MAX).unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].status, "ok");
        assert!(spans[0].end_ms.is_some());
    }

    #[test]
    fn claude_span_carries_detail_and_tokens() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects");
        let dir = projects.join(encode_cwd("/Users/yolk/Dev/kirimi"));
        std::fs::create_dir_all(&dir).unwrap();
        let long = "bun test --filter fare ".repeat(10);
        std::fs::write(
            dir.join("s1.jsonl"),
            format!("{}\n", claude_tool_use("m1", "t1", "Bash", &long)),
        )
        .unwrap();

        let mut s = store_at(tmp.path());
        index_claude(&mut s, &projects, home());
        let spans = s.spans(0, i64::MAX).unwrap();
        assert_eq!(spans.len(), 1);
        let detail = spans[0].detail.as_ref().unwrap();
        assert_eq!(detail.chars().count(), 80);
        assert_eq!(detail, &long[..80]);
        let tokens = spans[0]
            .tokens
            .clone()
            .expect("span should carry the opener's usage");
        assert_eq!(tokens.output, 50);
        assert_eq!(tokens.cache_read, 100);
        assert!(tokens.total() > 0);
    }

    #[test]
    fn claude_split_message_yields_one_estimated_sample() {
        let (tmp, projects) = claude_projects_with(&[
            user_prompt_at("2026-09-23T02:02:19.282Z"),
            assistant_chunk_at("m1", "2026-09-23T02:02:22.038Z", 130),
            assistant_chunk_at("m1", "2026-09-23T02:02:22.043Z", 130),
        ]);
        let mut s = store_at(tmp.path());
        index_claude(&mut s, &projects, home());
        let rows = s.perf_samples(0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "claude:m1");
        assert_eq!(rows[0].gen_ms, 2_761);
        assert_eq!(rows[0].output_tokens, 130);
        assert!(!rows[0].precise);
        assert_eq!(rows[0].ttft_ms, None);
    }

    #[test]
    fn claude_assistant_without_anchor_is_skipped() {
        let (tmp, projects) =
            claude_projects_with(&[assistant_chunk_at("m1", "2026-09-23T02:02:22.038Z", 130)]);
        let mut s = store_at(tmp.path());
        index_claude(&mut s, &projects, home());
        assert!(s.perf_samples(0).unwrap().is_empty());
    }

    #[test]
    fn claude_reindex_does_not_duplicate() {
        let (tmp, projects) = claude_projects_with(&[
            user_prompt_at("2026-09-23T02:02:19.282Z"),
            assistant_chunk_at("m1", "2026-09-23T02:02:22.038Z", 130),
        ]);
        let mut s = store_at(tmp.path());
        index_claude(&mut s, &projects, home());
        index_claude(&mut s, &projects, home());
        assert_eq!(s.perf_samples(0).unwrap().len(), 1);
    }

    #[test]
    fn codex_indexes_token_count_events() {
        let tmp = tempfile::tempdir().unwrap();
        let sessions = tmp.path().join("sessions/2026/04/04");
        std::fs::create_dir_all(&sessions).unwrap();
        let meta = r#"{"timestamp":"2026-04-03T22:54:57.166Z","type":"session_meta","payload":{"session_id":"abc","cwd":"/Users/yolk/Dev/kirimi"}}"#;
        let ctx = r#"{"timestamp":"2026-04-03T22:54:57.168Z","type":"turn_context","payload":{"cwd":"/Users/yolk/Dev/kirimi","model":"gpt-5.4"}}"#;
        let count = |input: u64, cached: u64, out: u64, reasoning: u64| {
            format!(
                r#"{{"timestamp":"2026-04-03T22:55:10.000Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":1,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":1,"reasoning_output_tokens":0}},"last_token_usage":{{"input_tokens":{input},"cached_input_tokens":{cached},"cache_write_input_tokens":0,"output_tokens":{out},"reasoning_output_tokens":{reasoning}}}}}}}}}"#
            )
        };
        std::fs::write(
            sessions.join("rollout-2026-04-04T05-54-28-abc.jsonl"),
            format!(
                "{meta}\n{ctx}\n{}\n{}\n",
                count(100, 40, 7, 3),
                count(200, 80, 9, 4)
            ),
        )
        .unwrap();

        let mut s = store_at(tmp.path());
        let r = index_codex(&mut s, &tmp.path().join("sessions"), home());
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(r.messages_upserted, 2);

        let models = s.by_model(0).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model, "gpt-5.4");
        assert_eq!(models[0].agent, Agent::Codex);
        assert_eq!(models[0].messages, 2);
        assert_eq!(models[0].tokens.input, 300);
        assert_eq!(models[0].tokens.cache_read, 120);
        assert_eq!(models[0].tokens.output, 16);
        assert_eq!(models[0].tokens.reasoning, 7);

        let projects = s.by_project(0).unwrap();
        assert_eq!(projects[0].project, "kirimi");
    }

    fn opencode_db(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("opencode.db");
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, directory TEXT, time_created INTEGER);
             CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);
             CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);",
        )
        .unwrap();
        c.execute(
            "INSERT INTO session VALUES ('ses_1','/Users/yolk/Dev/kirimi',1)",
            [],
        )
        .unwrap();
        path
    }

    /// A `part` row holding a tool call. `time` is spliced into `state.time`, so a
    /// test can pass `None` to omit it entirely and exercise the fallback.
    fn insert_tool_part(
        db: &Path,
        id: &str,
        created: i64,
        tool: &str,
        status: &str,
        time: Option<(i64, Option<i64>)>,
    ) {
        let c = rusqlite::Connection::open(db).unwrap();
        let time_json = match time {
            Some((start, Some(end))) => format!(r#","time":{{"start":{start},"end":{end}}}"#),
            Some((start, None)) => format!(r#","time":{{"start":{start}}}"#),
            None => String::new(),
        };
        let data = format!(
            r#"{{"type":"tool","tool":"{tool}","state":{{"status":"{status}","input":{{"command":"rm -rf /secret"}}{time_json}}}}}"#
        );
        c.execute(
            "INSERT INTO part VALUES (?1,'msg_1','ses_1',?2,?2,?3)",
            rusqlite::params![id, created, data],
        )
        .unwrap();
    }

    fn insert_message(db: &Path, id: &str, ts: i64, role: &str, input: u64, output: u64) {
        let c = rusqlite::Connection::open(db).unwrap();
        let data = format!(
            r#"{{"role":"{role}","providerID":"kn","modelID":"deepseek-v4-1-flash","tokens":{{"input":{input},"output":{output},"reasoning":3,"cache":{{"read":40,"write":5}}}}}}"#
        );
        c.execute(
            "INSERT INTO message VALUES (?1,'ses_1',?2,?3)",
            rusqlite::params![id, ts, data],
        )
        .unwrap();
    }

    fn insert_part(db: &Path, id: &str, msg: &str, created: i64, data: serde_json::Value) {
        let c = rusqlite::Connection::open(db).unwrap();
        c.execute(
            "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) VALUES (?1, ?2, 'ses_1', ?3, ?3, ?4)",
            rusqlite::params![id, msg, created, data.to_string()],
        )
        .unwrap();
    }

    #[test]
    fn opencode_step_yields_precise_sample() {
        let tmp = tempfile::tempdir().unwrap();
        let db = opencode_db(tmp.path());
        insert_message(&db, "msg1", 1_000, "assistant", 0, 0);
        insert_part(&db, "p1", "msg1", 1_000, serde_json::json!({"type":"step-start"}));
        insert_part(
            &db,
            "p2",
            "msg1",
            1_400,
            serde_json::json!({"type":"reasoning","time":{"start":1_400,"end":1_900}}),
        );
        insert_part(
            &db,
            "p3",
            "msg1",
            1_900,
            serde_json::json!({"type":"text","time":{"start":1_900,"end":2_900}}),
        );
        insert_part(
            &db,
            "p4",
            "msg1",
            3_000,
            serde_json::json!({"type":"step-finish","tokens":{"output":120,"reasoning":30}}),
        );
        let mut s = store_at(tmp.path());
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
        let tmp = tempfile::tempdir().unwrap();
        let db = opencode_db(tmp.path());
        insert_message(&db, "msg1", 1_000, "assistant", 0, 0);
        insert_part(&db, "p1", "msg1", 1_000, serde_json::json!({"type":"step-start"}));
        insert_part(
            &db,
            "p2",
            "msg1",
            1_100,
            serde_json::json!({"type":"tool","state":{"time":{"start":1_300,"end":61_300}}}),
        );
        insert_part(
            &db,
            "p3",
            "msg1",
            61_400,
            serde_json::json!({"type":"step-finish","tokens":{"output":40,"reasoning":0}}),
        );
        let mut s = store_at(tmp.path());
        index_opencode(&mut s, &db, home());
        let r = &s.perf_samples(0).unwrap()[0];
        assert_eq!(r.gen_ms, 200);
        assert_eq!(r.ttft_ms, Some(100));
        drop(tmp);
    }

    #[test]
    fn opencode_step_without_timed_parts_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let db = opencode_db(tmp.path());
        insert_message(&db, "msg1", 1_000, "assistant", 0, 0);
        insert_part(&db, "p1", "msg1", 1_000, serde_json::json!({"type":"step-start"}));
        insert_part(
            &db,
            "p2",
            "msg1",
            2_000,
            serde_json::json!({"type":"step-finish","tokens":{"output":40,"reasoning":0}}),
        );
        let mut s = store_at(tmp.path());
        index_opencode(&mut s, &db, home());
        assert!(s.perf_samples(0).unwrap().is_empty());
        drop(tmp);
    }

    #[test]
    fn opencode_perf_reindex_does_not_duplicate() {
        let tmp = tempfile::tempdir().unwrap();
        let db = opencode_db(tmp.path());
        insert_message(&db, "msg1", 1_000, "assistant", 0, 0);
        insert_part(&db, "p1", "msg1", 1_000, serde_json::json!({"type":"step-start"}));
        insert_part(
            &db,
            "p2",
            "msg1",
            1_100,
            serde_json::json!({"type":"text","time":{"start":1_100,"end":2_100}}),
        );
        insert_part(
            &db,
            "p3",
            "msg1",
            2_200,
            serde_json::json!({"type":"step-finish","tokens":{"output":40,"reasoning":0}}),
        );
        let mut s = store_at(tmp.path());
        index_opencode(&mut s, &db, home());
        // second pass: no new messages, but the 1h lookback re-reads this message's parts
        index_opencode(&mut s, &db, home());
        assert_eq!(s.perf_samples(0).unwrap().len(), 1);
        drop(tmp);
    }

    #[test]
    fn opencode_step_completed_after_first_index_is_caught_by_lookback() {
        let tmp = tempfile::tempdir().unwrap();
        let db = opencode_db(tmp.path());
        insert_message(&db, "msg1", 1_000, "assistant", 0, 0);
        insert_part(&db, "p1", "msg1", 1_000, serde_json::json!({"type":"step-start"}));
        insert_part(
            &db,
            "p2",
            "msg1",
            1_100,
            serde_json::json!({"type":"text","time":{"start":1_100,"end":1_600}}),
        );
        let mut s = store_at(tmp.path());
        index_opencode(&mut s, &db, home());
        assert!(
            s.perf_samples(0).unwrap().is_empty(),
            "no step-finish yet: no row should be produced"
        );

        // The message is still streaming: the step only finishes now. The
        // message pass's `since` already moved past this message's
        // time_created (1_000), so without the 1h lookback the part query
        // would never see it again and this step's timing would be lost.
        insert_part(
            &db,
            "p3",
            "msg1",
            2_000,
            serde_json::json!({"type":"step-finish","tokens":{"output":40,"reasoning":0}}),
        );
        index_opencode(&mut s, &db, home());
        let rows = s.perf_samples(0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].gen_ms, 500);
        drop(tmp);
    }

    #[test]
    fn opencode_spans_map_status() {
        let tmp = tempfile::tempdir().unwrap();
        let db = opencode_db(tmp.path());
        insert_tool_part(
            &db,
            "prt_ok",
            100,
            "bash",
            "completed",
            Some((110, Some(120))),
        );
        insert_tool_part(&db, "prt_err", 200, "bash", "error", Some((210, Some(220))));
        insert_tool_part(&db, "prt_run", 300, "bash", "running", Some((310, None)));

        let mut s = store_at(tmp.path());
        let r = index_opencode(&mut s, &db, home());
        assert!(r.errors.is_empty(), "{:?}", r.errors);

        let spans = s.spans(0, i64::MAX).unwrap();
        assert_eq!(spans.len(), 3);
        let get = |id: &str| spans.iter().find(|x| x.id == id).unwrap();
        assert_eq!(get("opencode:prt_ok").status, "ok");
        assert_eq!(get("opencode:prt_err").status, "error");
        assert_eq!(get("opencode:prt_run").status, "running");
        assert_eq!(get("opencode:prt_ok").tool, "bash");
        assert_eq!(get("opencode:prt_ok").agent, Agent::Opencode);
        assert_eq!(get("opencode:prt_ok").project, "kirimi");
    }

    #[test]
    fn opencode_span_uses_state_times_then_falls_back() {
        let tmp = tempfile::tempdir().unwrap();
        let db = opencode_db(tmp.path());
        insert_tool_part(
            &db,
            "prt_times",
            100,
            "bash",
            "completed",
            Some((110, Some(120))),
        );
        insert_tool_part(&db, "prt_fallback", 400, "bash", "running", None);

        let mut s = store_at(tmp.path());
        index_opencode(&mut s, &db, home());
        let spans = s.spans(0, i64::MAX).unwrap();
        let get = |id: &str| spans.iter().find(|x| x.id == id).unwrap();

        let timed = get("opencode:prt_times");
        assert_eq!(timed.start_ms, 110);
        assert_eq!(timed.end_ms, Some(120));

        let fallback = get("opencode:prt_fallback");
        assert_eq!(
            fallback.start_ms, 400,
            "must fall back to part.time_created"
        );
        assert_eq!(fallback.end_ms, None);
    }

    #[test]
    fn opencode_spans_store_no_command_text() {
        let tmp = tempfile::tempdir().unwrap();
        let db = opencode_db(tmp.path());
        insert_tool_part(
            &db,
            "prt_secret",
            100,
            "bash",
            "completed",
            Some((110, Some(120))),
        );

        let mut s = store_at(tmp.path());
        index_opencode(&mut s, &db, home());
        let spans = s.spans(0, i64::MAX).unwrap();
        let span = spans
            .iter()
            .find(|x| x.id == "opencode:prt_secret")
            .unwrap();
        assert_eq!(
            span.detail, None,
            "opencode must never persist command text"
        );
        assert_eq!(span.tokens, None);
    }

    #[test]
    fn opencode_indexes_assistant_messages_only() {
        let tmp = tempfile::tempdir().unwrap();
        let db = opencode_db(tmp.path());
        insert_message(&db, "msg_u", 100, "user", 1, 1);
        insert_message(&db, "msg_a", 200, "assistant", 10, 20);

        let mut s = store_at(tmp.path());
        let r = index_opencode(&mut s, &db, home());
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(r.messages_upserted, 1);
        assert_eq!(s.counts().unwrap().1, 1);

        let models = s.by_model(0).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model, "kn/deepseek-v4-1-flash");
        assert_eq!(models[0].agent, Agent::Opencode);
        assert_eq!(models[0].tokens.input, 10);
        assert_eq!(models[0].tokens.output, 20);
        assert_eq!(models[0].tokens.reasoning, 3);
        assert_eq!(models[0].tokens.cache_read, 40);
        assert_eq!(models[0].tokens.cache_write, 5);
        let projects = s.by_project(0).unwrap();
        assert_eq!(projects[0].project, "kirimi");
    }

    #[test]
    fn opencode_second_run_only_reads_newer_messages() {
        let tmp = tempfile::tempdir().unwrap();
        let db = opencode_db(tmp.path());
        insert_message(&db, "msg_a", 200, "assistant", 10, 20);

        let mut s = store_at(tmp.path());
        let first = index_opencode(&mut s, &db, home());
        assert_eq!(first.messages_upserted, 1);

        insert_message(&db, "msg_b", 300, "assistant", 1, 2);
        let second = index_opencode(&mut s, &db, home());
        assert_eq!(second.messages_upserted, 1);
        assert_eq!(s.counts().unwrap().1, 2);
    }

    #[test]
    fn opencode_missing_db_reports_no_error() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = store_at(tmp.path());
        let r = index_opencode(&mut s, &tmp.path().join("nope.db"), home());
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(r.messages_upserted, 0);
        assert_eq!(r.files_scanned, 0);
        assert_eq!(s.counts().unwrap(), (0, 0));
    }
}
