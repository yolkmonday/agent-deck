use collector::indexer::{index_claude, index_codex, index_opencode, IndexReport};
use collector::live::{LiveCollector, Paths};
use collector::model::{Agent, LiveSnapshot, TokenUsage};
use collector::pricing::{PriceEntry, PriceTable};
use collector::process::SystemProcessTable;
use collector::store::{ModelAgg, ProjectAgg, SpanRow, Store, TotalsAgg};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, State};

struct AppState {
    collector: Mutex<LiveCollector>,
    store: Mutex<Store>,
    pricing: Mutex<PriceTable>,
    indexing: AtomicBool,
    last_run_ms: Mutex<Option<i64>>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn data_dir(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .ok()
        .or_else(dirs::data_dir)
        .unwrap_or_else(std::env::temp_dir)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IndexStatus {
    indexed_files: i64,
    messages: i64,
    running: bool,
    last_run_ms: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DailyRow {
    date: String,
    agent: Agent,
    tokens: TokenUsage,
    cost_usd: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelRow {
    model: String,
    agent: Agent,
    tokens: TokenUsage,
    cost_usd: f64,
    messages: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRow {
    project: String,
    tokens: TokenUsage,
    cost_usd: f64,
    messages: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Totals {
    tokens: TokenUsage,
    cost_usd: f64,
    messages: i64,
    cache_hit_pct: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TimelineSpan {
    id: String,
    tool: String,
    detail: Option<String>,
    start_ms: i64,
    end_ms: Option<i64>,
    status: String,
    tokens: Option<TokenUsage>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TimelineLane {
    session_id: String,
    agent: Agent,
    project: String,
    model: Option<String>,
    spans: Vec<TimelineSpan>,
}

const KNOWN_SPAN_STATUSES: [&str; 3] = ["ok", "error", "running"];

fn timeline_span(s: SpanRow) -> TimelineSpan {
    // The store keeps whatever string it was handed; the frontend contract only
    // colours ok/error/running, so anything unexpected degrades to running.
    let status = if KNOWN_SPAN_STATUSES.contains(&s.status.as_str()) {
        s.status
    } else {
        "running".to_string()
    };
    TimelineSpan {
        id: s.id,
        tool: s.tool,
        detail: s.detail,
        start_ms: s.start_ms,
        end_ms: s.end_ms,
        status,
        tokens: s.tokens,
    }
}

/// Spans overlapping the window, grouped into one lane per session and ordered by
/// each lane's earliest span. Sessions with no span inside the window are absent.
#[tauri::command]
fn timeline_spans(
    from_ms: i64,
    to_ms: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<TimelineLane>, String> {
    let rows = state
        .store
        .lock()
        .unwrap()
        .spans(from_ms, to_ms)
        .map_err(|e| e.to_string())?;

    // `Store::spans` already orders by session_id then start_ms.
    let mut lanes: Vec<TimelineLane> = Vec::new();
    for row in rows {
        let span = timeline_span(row.clone());
        match lanes.last_mut().filter(|l| l.session_id == row.session_id) {
            Some(lane) => {
                if row.model.is_some() {
                    lane.model = row.model;
                }
                lane.spans.push(span);
            }
            None => lanes.push(TimelineLane {
                session_id: row.session_id.clone(),
                agent: row.agent,
                project: row.project.clone(),
                model: row.model.clone(),
                spans: vec![span],
            }),
        }
    }

    lanes.sort_by_key(|l| l.spans.first().map(|s| s.start_ms).unwrap_or(0));
    Ok(lanes)
}

fn since_ms(days: i64) -> i64 {
    if days <= 0 {
        0
    } else {
        now_ms() - days * 86_400_000
    }
}

fn status(state: &AppState) -> Result<IndexStatus, String> {
    let (indexed_files, messages) = state.store.lock().unwrap().counts().map_err(|e| e.to_string())?;
    Ok(IndexStatus {
        indexed_files,
        messages,
        running: state.indexing.load(Ordering::SeqCst),
        last_run_ms: *state.last_run_ms.lock().unwrap(),
    })
}

#[tauri::command]
fn live_snapshot(state: State<'_, Arc<AppState>>) -> LiveSnapshot {
    state.collector.lock().unwrap().snapshot(now_ms())
}

#[tauri::command]
fn index_status(state: State<'_, Arc<AppState>>) -> Result<IndexStatus, String> {
    status(&state)
}

/// One full incremental pass. Reads every file WITHOUT holding the store lock,
/// then locks only to write.
fn run_index_pass(app: &tauri::AppHandle, state: &Arc<AppState>) -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let paths = Paths::for_home(&home);
    let claude_projects = paths.claude_projects.clone();
    let codex_sessions = PathBuf::from(format!("{home}/.codex/sessions"));

    let mut reports: Vec<IndexReport> = Vec::new();
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    reports.push(index_claude(&mut store, &claude_projects, &home));
    reports.push(index_opencode(&mut store, &paths.opencode_db, &home));
    reports.push(index_codex(&mut store, &codex_sessions, &home));
    drop(store);

    for r in &reports {
        for e in &r.errors {
            eprintln!("index warning: {e}");
        }
    }
    *state.last_run_ms.lock().unwrap() = Some(now_ms());
    let _ = app;
    Ok(())
}

#[tauri::command]
fn reindex(app: tauri::AppHandle, state: State<'_, Arc<AppState>>) -> Result<IndexStatus, String> {
    let state = state.inner().clone();
    if state
        .indexing
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return status(&state);
    }
    let result = run_index_pass(&app, &state);
    state.indexing.store(false, Ordering::SeqCst);
    result?;
    status(&state)
}

#[tauri::command]
fn history_daily(days: i64, state: State<'_, Arc<AppState>>) -> Result<Vec<DailyRow>, String> {
    let since = since_ms(days);
    let store = state.store.lock().unwrap();
    let pricing = state.pricing.lock().unwrap();
    let buckets = store.daily_by_model(since).map_err(|e| e.to_string())?;

    let mut by_key: std::collections::BTreeMap<(String, u8), DailyRow> = Default::default();
    for (date, agent, model, tokens) in buckets {
        let entry = by_key
            .entry((date.clone(), agent_rank(agent)))
            .or_insert_with(|| DailyRow {
                date: date.clone(),
                agent,
                tokens: TokenUsage::default(),
                cost_usd: 0.0,
            });
        entry.cost_usd += pricing.cost_usd(&model, &tokens);
        entry.tokens.add(&tokens);
    }
    Ok(by_key.into_values().collect())
}

fn agent_rank(a: Agent) -> u8 {
    match a {
        Agent::Claude => 0,
        Agent::Opencode => 1,
        Agent::Codex => 2,
    }
}

#[tauri::command]
fn history_by_model(days: i64, state: State<'_, Arc<AppState>>) -> Result<Vec<ModelRow>, String> {
    let since = since_ms(days);
    let store = state.store.lock().unwrap();
    let pricing = state.pricing.lock().unwrap();
    let rows: Vec<ModelAgg> = store.by_model(since).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|r| ModelRow {
            cost_usd: pricing.cost_usd(&r.model, &r.tokens),
            model: r.model,
            agent: r.agent,
            tokens: r.tokens,
            messages: r.messages,
        })
        .collect())
}

#[tauri::command]
fn history_by_project(days: i64, state: State<'_, Arc<AppState>>) -> Result<Vec<ProjectRow>, String> {
    let since = since_ms(days);
    let store = state.store.lock().unwrap();
    let pricing = state.pricing.lock().unwrap();
    let aggregates: Vec<ProjectAgg> = store.by_project(since).map_err(|e| e.to_string())?;

    let mut costs: std::collections::HashMap<String, f64> = Default::default();
    for (project, model, tokens) in store.by_project_model(since).map_err(|e| e.to_string())? {
        *costs.entry(project).or_default() += pricing.cost_usd(&model, &tokens);
    }

    Ok(aggregates
        .into_iter()
        .map(|r| ProjectRow {
            cost_usd: costs.get(&r.project).copied().unwrap_or(0.0),
            project: r.project,
            tokens: r.tokens,
            messages: r.messages,
        })
        .collect())
}

#[tauri::command]
fn history_totals(days: i64, state: State<'_, Arc<AppState>>) -> Result<Totals, String> {
    let since = since_ms(days);
    let store = state.store.lock().unwrap();
    let pricing = state.pricing.lock().unwrap();
    let t: TotalsAgg = store.totals(since).map_err(|e| e.to_string())?;
    let cost_usd: f64 = store
        .by_model(since)
        .map_err(|e| e.to_string())?
        .iter()
        .map(|m| pricing.cost_usd(&m.model, &m.tokens))
        .sum();
    let denom = t.tokens.cache_read + t.tokens.input;
    let cache_hit_pct = if denom == 0 {
        0.0
    } else {
        t.tokens.cache_read as f64 / denom as f64 * 100.0
    };
    Ok(Totals {
        tokens: t.tokens,
        cost_usd,
        messages: t.messages,
        cache_hit_pct,
    })
}

#[tauri::command]
fn pricing_get(state: State<'_, Arc<AppState>>) -> Vec<PriceEntry> {
    state.pricing.lock().unwrap().entries().to_vec()
}

#[tauri::command]
fn pricing_set(
    entries: Vec<PriceEntry>,
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<PriceEntry>, String> {
    let mut table = state.pricing.lock().unwrap();
    table.set(entries);
    let path = data_dir(&app).join("pricing.json");
    table.save(&path).map_err(|e| e.to_string())?;
    Ok(table.entries().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span_row(id: &str, session_id: &str, model: Option<&str>, start_ms: i64) -> SpanRow {
        SpanRow {
            id: id.into(),
            agent: Agent::Claude,
            session_id: session_id.into(),
            project: "kirimi".into(),
            model: model.map(str::to_string),
            tool: "Bash".into(),
            detail: None,
            start_ms,
            end_ms: None,
            status: "running".into(),
            tokens: None,
        }
    }

    #[test]
    fn timeline_wire_shape_matches_the_frontend_contract() {
        let lane = TimelineLane {
            session_id: "s1".into(),
            agent: Agent::Claude,
            project: "kirimi".into(),
            model: Some("claude-sonnet-5".into()),
            spans: vec![timeline_span(SpanRow {
                end_ms: None,
                status: "error".into(),
                detail: Some("bun test".into()),
                tokens: Some(TokenUsage { input: 1, output: 2, cache_read: 3, cache_write: 4, reasoning: 5 }),
                ..span_row("claude:t1", "s1", Some("claude-sonnet-5"), 1000)
            })],
        };
        let v = serde_json::to_value(&lane).unwrap();
        assert_eq!(v["sessionId"], "s1");
        assert_eq!(v["agent"], "claude");
        assert_eq!(v["spans"][0]["startMs"], 1000);
        assert_eq!(v["spans"][0]["endMs"], serde_json::Value::Null);
        assert_eq!(v["spans"][0]["status"], "error");
        assert_eq!(v["spans"][0]["tokens"]["cacheRead"], 3);
    }

    #[test]
    fn unknown_span_status_degrades_to_running() {
        let s = timeline_span(SpanRow {
            status: "weird".into(),
            ..span_row("x", "s1", None, 0)
        });
        assert_eq!(s.status, "running");
    }

    #[test]
    fn timeline_span_keeps_known_statuses() {
        for st in KNOWN_SPAN_STATUSES {
            let s = timeline_span(SpanRow {
                status: st.to_string(),
                ..span_row("x", "s1", None, 0)
            });
            assert_eq!(s.status, st);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let home = std::env::var("HOME").unwrap_or_default();
            let dir = data_dir(&app.handle().clone());
            let store = Store::open(&dir.join("agent-deck.db"))
                .expect("failed to open the history index");
            let pricing = PriceTable::load(&dir.join("pricing.json"));
            let collector = LiveCollector::new(Paths::for_home(&home), Arc::new(SystemProcessTable));
            let state = Arc::new(AppState {
                collector: Mutex::new(collector),
                store: Mutex::new(store),
                pricing: Mutex::new(pricing),
                indexing: AtomicBool::new(false),
                last_run_ms: Mutex::new(None),
            });
            app.manage(state.clone());

            let index_handle = app.handle().clone();
            let index_state = state.clone();
            std::thread::spawn(move || {
                if index_state
                    .indexing
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    let _ = run_index_pass(&index_handle, &index_state);
                    index_state.indexing.store(false, Ordering::SeqCst);
                }
            });

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last: Option<LiveSnapshot> = None;
                loop {
                    let snap = state.collector.lock().unwrap().snapshot(now_ms());
                    if last.as_ref().is_none_or(|l| !l.same_content(&snap)) {
                        let _ = handle.emit("live://snapshot", &snap);
                        last = Some(snap);
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            live_snapshot,
            index_status,
            reindex,
            history_daily,
            history_by_model,
            history_by_project,
            history_totals,
            timeline_spans,
            pricing_get,
            pricing_set
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
