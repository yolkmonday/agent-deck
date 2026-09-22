use collector::indexer::{index_claude, index_codex, index_opencode, IndexReport};
use collector::live::{LiveCollector, Paths};
use collector::model::{Agent, LiveSnapshot, TokenUsage};
use collector::pricing::{PriceEntry, PriceTable};
use collector::process::SystemProcessTable;
use collector::store::{ModelAgg, ProjectAgg, Store, TotalsAgg};
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
            pricing_get,
            pricing_set
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
