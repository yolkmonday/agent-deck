mod config;
mod probe;
mod projects;
mod providers;
mod secrets;
mod terminal;

use collector::billing::{
    cycle_for, days_left, format_date, local_date, parse_date_iso, start_of_day_ms, BillingAccount,
    BillingMode, BillingTable,
};
use collector::indexer::{index_claude, index_codex, index_opencode, IndexReport};
use collector::health::Thresholds;
use collector::live::{LiveCollector, Paths};
use collector::model::{Agent, LiveSnapshot, TokenUsage};
use collector::pricing::{PriceEntry, PriceTable};
use collector::process::SystemProcessTable;
use collector::savings::{read_lean_ctx, read_rtk};
use collector::store::{AccountAgg, ModelAgg, ProjectAgg, SpanRow, Store, TotalsAgg};
use collector::tail::{read_tail, TailEntry};
use collector::transcript;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, State};
use terminal::{TermDataEvent, TermExitEvent, TermProfile, TermSession, TerminalRegistry};

struct AppState {
    collector: Mutex<LiveCollector>,
    store: Mutex<Store>,
    pricing: Mutex<PriceTable>,
    billing: Mutex<BillingTable>,
    indexing: AtomicBool,
    last_run_ms: Mutex<Option<i64>>,
    savings_cache: Mutex<Option<SavingsCache>>,
    terminals: Mutex<TerminalRegistry>,
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

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SavingsSource {
    available: bool,
    saved_tokens: i64,
    total_tokens: i64,
    savings_pct: f64,
    entries: i64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SavingsDay {
    date: String,
    rtk_saved: i64,
    lean_ctx_saved: i64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SavingsCommand {
    command: String,
    saved_tokens: i64,
    savings_pct: f64,
    runs: i64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SavingsSummary {
    rtk: SavingsSource,
    lean_ctx: SavingsSource,
    daily: Vec<SavingsDay>,
    top_commands: Vec<SavingsCommand>,
    warnings: Vec<String>,
}

struct SavingsCache {
    key: i64,
    at_ms: i64,
    summary: SavingsSummary,
}

const SAVINGS_CACHE_MS: i64 = 30_000;
const TOP_COMMAND_LIMIT: usize = 10;

fn savings_pct(saved: i64, total: i64) -> f64 {
    if total <= 0 {
        0.0
    } else {
        saved as f64 / total as f64 * 100.0
    }
}

fn missing_source() -> SavingsSource {
    SavingsSource {
        available: false,
        saved_tokens: 0,
        total_tokens: 0,
        savings_pct: 0.0,
        entries: 0,
    }
}

/// Merges both readers into the wire view. A source that cannot be read is a
/// warning, never an error: the screen still shows whatever the other one has.
fn savings_summary_for(days: i64, home: &str) -> SavingsSummary {
    let since = since_ms(days);
    let mut warnings = Vec::new();

    let rtk_path = PathBuf::from(home)
        .join("Library/Application Support/rtk/history.db");
    let (rtk, rtk_daily, top_commands): (SavingsSource, Vec<(String, i64)>, Vec<SavingsCommand>) =
        match read_rtk(&rtk_path, since) {
        Ok(s) => {
            let source = SavingsSource {
                available: true,
                saved_tokens: s.saved_tokens,
                total_tokens: s.total_tokens,
                savings_pct: savings_pct(s.saved_tokens, s.total_tokens),
                entries: s.entries,
            };
            let top = s
                .top_commands
                .into_iter()
                .take(TOP_COMMAND_LIMIT)
                .map(|(command, saved_tokens, savings_pct, runs)| SavingsCommand {
                    command,
                    saved_tokens,
                    savings_pct,
                    runs,
                })
                .collect();
            (source, s.daily, top)
        }
        Err(e) => {
            warnings.push(format!("rtk: {e}"));
            (missing_source(), Vec::new(), Vec::new())
        }
    };

    let lean_path = PathBuf::from(home).join(".lean-ctx/stats.json");
    let (lean_ctx, lean_daily): (SavingsSource, Vec<(String, i64)>) =
        match read_lean_ctx(&lean_path, since) {
            Ok(s) => (
                SavingsSource {
                    available: true,
                    saved_tokens: s.saved_tokens,
                    total_tokens: s.total_tokens,
                    savings_pct: savings_pct(s.saved_tokens, s.total_tokens),
                    entries: s.entries,
                },
                s.daily,
            ),
            Err(e) => {
                warnings.push(format!("lean-ctx: {e}"));
                (missing_source(), Vec::new())
            }
        };

    let mut by_day: std::collections::BTreeMap<String, SavingsDay> = Default::default();
    for (date, saved) in rtk_daily {
        by_day.entry(date.clone()).or_insert_with(|| SavingsDay {
            date,
            rtk_saved: 0,
            lean_ctx_saved: 0,
        }).rtk_saved += saved;
    }
    for (date, saved) in lean_daily {
        by_day.entry(date.clone()).or_insert_with(|| SavingsDay {
            date,
            rtk_saved: 0,
            lean_ctx_saved: 0,
        }).lean_ctx_saved += saved;
    }

    SavingsSummary {
        rtk,
        lean_ctx,
        daily: by_day.into_values().collect(),
        top_commands,
        warnings,
    }
}

#[tauri::command]
fn savings_summary(days: i64, state: State<'_, Arc<AppState>>) -> Result<SavingsSummary, String> {
    let now = now_ms();
    {
        let cache = state.savings_cache.lock().map_err(|e| e.to_string())?;
        if let Some(c) = cache.as_ref() {
            if c.key == days && now - c.at_ms < SAVINGS_CACHE_MS {
                return Ok(c.summary.clone());
            }
        }
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let summary = savings_summary_for(days, &home);
    if let Ok(mut cache) = state.savings_cache.lock() {
        *cache = Some(SavingsCache {
            key: days,
            at_ms: now,
            summary: summary.clone(),
        });
    }
    Ok(summary)
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
    let prices = state.pricing.lock().unwrap().clone();
    let billing = state.billing.lock().unwrap().clone();
    let thresholds = thresholds(&state);
    state
        .collector
        .lock()
        .unwrap()
        .snapshot(now_ms(), &prices, &billing, &thresholds)
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

fn agent_rank(a: Agent) -> u8 {    match a {
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

const RENEWAL_DAY_MAX: u32 = 28;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountPeriod {
    account_id: String,
    label: String,
    mode: BillingMode,
    from_date: String,
    to_date: String,
    tokens: TokenUsage,
    spend_usd: f64,
    notional_usd: f64,
    committed_usd: Option<f64>,
    credit_left_usd: Option<f64>,
    days_left: Option<i64>,
    expired: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BillingSummary {
    periods: Vec<AccountPeriod>,
    total_spend_usd: f64,
    total_notional_usd: f64,
    warnings: Vec<String>,
}

/// One account as the user typed it, checked before it is written. Every failure
/// is the exact Indonesian string the dialog displays.
pub fn validate_account(a: &BillingAccount) -> Result<(), String> {
    if a.label.trim().is_empty() {
        return Err("nama akun tidak boleh kosong".into());
    }
    if a.matches.iter().all(|m| m.trim().is_empty()) {
        return Err("isi setidaknya satu awalan model".into());
    }
    match a.mode {
        BillingMode::Subscription => {
            let monthly = a
                .monthly_usd
                .ok_or_else(|| "biaya bulanan wajib diisi".to_string())?;
            if monthly < 0.0 {
                return Err("biaya bulanan tidak boleh negatif".into());
            }
            let day = a
                .renewal_day
                .ok_or_else(|| "tanggal perpanjangan wajib diisi".to_string())?;
            if !(1..=RENEWAL_DAY_MAX).contains(&day) {
                return Err(format!("tanggal perpanjangan harus 1 sampai {RENEWAL_DAY_MAX}"));
            }
        }
        BillingMode::Prepaid => {
            let credit = a
                .credit_usd
                .ok_or_else(|| "saldo awal wajib diisi".to_string())?;
            if credit < 0.0 {
                return Err("saldo awal tidak boleh negatif".into());
            }
            if a.started_on.as_deref().is_some_and(|d| parse_date_iso(d).is_none()) {
                return Err("tanggal mulai tidak valid".into());
            }
        }
        BillingMode::Payg => {}
    }
    for (field, value) in [("tanggal berakhir", &a.expires_on)] {
        if value.as_deref().is_some_and(|d| parse_date_iso(d).is_none()) {
            return Err(format!("{field} tidak valid"));
        }
    }
    Ok(())
}

fn validate_accounts(accounts: &[BillingAccount]) -> Result<(), String> {
    for a in accounts {
        validate_account(a)?;
    }
    Ok(())
}

fn period_for(agg: &AccountAgg, account: Option<&BillingAccount>, as_of: time::Date) -> AccountPeriod {
    let (from, to) = match account {
        Some(a) => cycle_for(a, as_of),
        // The fallback group is pay-as-you-go, so it reports the calendar month.
        None => cycle_for(
            &BillingAccount {
                id: String::new(),
                label: agg.label.clone(),
                mode: BillingMode::Payg,
                matches: Vec::new(),
                monthly_usd: None,
                renewal_day: None,
                credit_usd: None,
                started_on: None,
                expires_on: None,
            },
            as_of,
        ),
    };
    let left = account.and_then(|a| days_left(a, as_of));
    // Prepaid credit drains at API rates, so what is left is what was topped up
    // minus what this cycle's tokens cost. It never reads below zero.
    let credit_left = account
        .filter(|a| a.mode == BillingMode::Prepaid)
        .map(|a| (a.credit_usd.unwrap_or(0.0) - agg.spend_usd).max(0.0));
    AccountPeriod {
        account_id: agg.account_id.clone(),
        label: agg.label.clone(),
        mode: agg.mode,
        from_date: format_date(from),
        to_date: format_date(to),
        tokens: agg.tokens.clone(),
        spend_usd: agg.spend_usd,
        notional_usd: agg.notional_usd,
        committed_usd: account.and_then(|a| a.monthly_usd),
        credit_left_usd: credit_left,
        days_left: left,
        expired: left.is_some_and(|d| d < 0),
    }
}

#[tauri::command]
fn billing_accounts(state: State<'_, Arc<AppState>>) -> Vec<BillingAccount> {
    state.billing.lock().unwrap().accounts().to_vec()
}

#[tauri::command]
fn billing_save(
    accounts: Vec<BillingAccount>,
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<BillingAccount>, String> {
    validate_accounts(&accounts)?;
    let mut table = state.billing.lock().unwrap();
    table.set(accounts);
    table
        .save(&data_dir(&app).join("billing.json"))
        .map_err(|e| e.to_string())?;
    Ok(table.accounts().to_vec())
}

#[tauri::command]
fn billing_summary(
    as_of: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<BillingSummary, String> {
    let today = match as_of.as_deref() {
        Some(s) => parse_date_iso(s).ok_or_else(|| "tanggal tidak valid".to_string())?,
        None => local_date(now_ms()),
    };
    let table = state.billing.lock().unwrap().clone();
    let prices = state.pricing.lock().unwrap().clone();

    let mut periods = Vec::new();
    let mut warnings = Vec::new();
    for agg in table_accounts(&state, &table, &prices, today)? {
        let account = table.accounts().iter().find(|a| a.id == agg.account_id);
        let period = period_for(&agg, account, today);
        if period.expired {
            warnings.push(format!("{} sudah lewat tanggal", period.label));
        } else if let (Some(left), Some(a)) = (period.days_left, account) {
            if a.mode == BillingMode::Prepaid {
                if let (Some(credit), Some(started)) = (
                    a.credit_usd,
                    a.started_on.as_deref().and_then(parse_date_iso),
                ) {
                    let elapsed = (today - started).whole_days().max(1);
                    let burn = agg.spend_usd / elapsed as f64;
                    if burn > 0.0 {
                        let runs_out = (credit / burn).floor() as i64;
                        if runs_out < left {
                            warnings.push(format!("{} habis dalam {} hari", period.label, runs_out));
                        }
                    }
                }
            }
        }
        periods.push(period);
    }

    Ok(BillingSummary {
        total_spend_usd: periods.iter().map(|p| p.spend_usd).sum(),
        total_notional_usd: periods.iter().map(|p| p.notional_usd).sum(),
        periods,
        warnings,
    })
}

/// A window that covers every account's own cycle: each one starts somewhere in
/// the recent past, so a generous span in milliseconds is enough to feed them all
/// and the per-account cycle decides what is actually counted.
fn table_accounts(
    state: &Arc<AppState>,
    table: &BillingTable,
    prices: &PriceTable,
    as_of: time::Date,
) -> Result<Vec<AccountAgg>, String> {
    let earliest = table
        .accounts()
        .iter()
        .map(|a| cycle_for(a, as_of).0)
        .min()
        .unwrap_or(as_of);
    let latest = table
        .accounts()
        .iter()
        .map(|a| cycle_for(a, as_of).1)
        .max()
        .unwrap_or(as_of);
    let from_ms = start_of_day_ms(earliest);
    // The window is inclusive of the last day's messages.
    let to_ms = start_of_day_ms(latest) + 86_400_000;
    state
        .store
        .lock()
        .unwrap()
        .by_account(from_ms, to_ms, table, prices)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn term_profiles() -> Vec<TermProfile> {
    TerminalRegistry::profiles()
}
#[tauri::command]
fn term_start(
    profile_id: String,
    cwd: String,
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<TermSession, String> {
    let mut reg = state.terminals.lock().map_err(|e| e.to_string())?;
    let data_app = app.clone();
    reg.start(
        &profile_id,
        &cwd,
        move |evt: TermDataEvent| {
            let _ = data_app.emit("term://data", evt);
        },
        move |evt: TermExitEvent| {
            let _ = app.emit("term://exit", evt);
        },
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn term_list(state: State<'_, Arc<AppState>>) -> Result<Vec<TermSession>, String> {
    Ok(state.terminals.lock().map_err(|e| e.to_string())?.list())
}

#[tauri::command]
fn term_write(id: String, data: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state
        .terminals
        .lock()
        .map_err(|e| e.to_string())?
        .write(&id, &data)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn term_resize(
    id: String,
    cols: u16,
    rows: u16,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state
        .terminals
        .lock()
        .map_err(|e| e.to_string())?
        .resize(&id, cols, rows)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn term_kill(id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state
        .terminals
        .lock()
        .map_err(|e| e.to_string())?
        .kill(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn term_scrollback(id: String, state: State<'_, Arc<AppState>>) -> Result<String, String> {
    Ok(state
        .terminals
        .lock()
        .map_err(|e| e.to_string())?
        .scrollback(&id))
}

fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_default()
}

#[tauri::command]
fn models_overview(state: State<'_, Arc<AppState>>) -> Result<providers::ModelsOverview, String> {
    let home = home_dir();
    let (claude_recent, codex_models) = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        (
            store
                .distinct_models(Agent::Claude)
                .map_err(|e| e.to_string())?,
            store
                .distinct_models(Agent::Codex)
                .map_err(|e| e.to_string())?,
        )
    };
    providers::read_overview(&home, claude_recent, codex_models).map_err(|e| e.to_string())
}

#[tauri::command]
fn provider_save(
    provider: providers::OcProviderInput,
    state: State<'_, Arc<AppState>>,
) -> Result<providers::OcProvider, String> {
    let _ = &state;
    providers::save_provider(&home_dir(), provider).map_err(|e| e.to_string())
}

#[tauri::command]
fn provider_delete(id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let _ = &state;
    providers::delete_provider(&home_dir(), &id).map_err(|e| e.to_string())
}

/// Stores a key in its own 0600 file and returns the masked form. The plaintext the user
/// typed goes straight to disk and is never echoed back.
#[tauri::command]
fn secret_set(provider_id: String, key: String, state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let _ = &state;
    let home = home_dir();
    secrets::write_key(&home, &provider_id, &key).map_err(|e| e.to_string())?;
    Ok(secrets::mask(&key))
}

#[tauri::command]
fn secret_clear(provider_id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let _ = &state;
    secrets::clear_key(&home_dir(), &provider_id).map_err(|e| e.to_string())
}

/// The only command that returns a plaintext key. It exists for the eye button in the key
/// field and must only be invoked from an explicit user action; its result is never
/// logged, cached or written to disk by the dashboard.
#[tauri::command]
fn secret_reveal(provider_id: String, state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let _ = &state;
    secrets::read_key(&home_dir(), &provider_id).map_err(|e| e.to_string())
}

/// Moves a key that is still plaintext in the config into a 0600 file and replaces the
/// config value with the `{file:...}` reference. Returns the masked form.
#[tauri::command]
fn secret_migrate_inline(
    provider_id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let _ = &state;
    let home = home_dir();
    let key = providers::inline_key(&home, &provider_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "provider tidak punya key plaintext di config".to_string())?;

    secrets::write_key(&home, &provider_id, &key).map_err(|e| e.to_string())?;

    let mut file = config::ConfigFile::load(
        &providers::config_path(&home)
            .ok_or_else(|| "config opencode tidak ditemukan".to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let reference = secrets::file_ref(&home, &provider_id);
    let base = ["provider", provider_id.as_str(), "options"];
    let value = file.value().map_err(|e| e.to_string())?;
    let has_custom_header = value
        .get("provider")
        .and_then(|p| p.get(&provider_id))
        .and_then(|p| p.get("options"))
        .and_then(|o| o.get("apiKey"))
        .is_none();

    if has_custom_header {
        let name = value
            .get("provider")
            .and_then(|p| p.get(&provider_id))
            .and_then(|p| p.get("options"))
            .and_then(|o| o.get("headers"))
            .and_then(|h| h.as_object())
            .and_then(|h| h.keys().next().cloned())
            .ok_or_else(|| "provider tidak punya header atau apiKey".to_string())?;
        let mut path = base.to_vec();
        path.push("headers");
        path.push(name.as_str());
        file.set_path(&path, serde_json::json!(reference))
            .map_err(|e| e.to_string())?;
    } else {
        let mut path = base.to_vec();
        path.push("apiKey");
        file.set_path(&path, serde_json::json!(reference))
            .map_err(|e| e.to_string())?;
    }
    file.save_atomic().map_err(|e| e.to_string())?;

    Ok(secrets::mask(&key))
}

#[tauri::command]
async fn models_fetch(provider_id: String, state: State<'_, Arc<AppState>>) -> Result<Vec<String>, String> {
    let _ = &state;
    let home = home_dir();
    let (base_url, key, style, _) =
        providers::probe_target(&home, &provider_id).map_err(|e| e.to_string())?;
    probe::fetch_models(&base_url, &key, style)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn model_test(
    provider_id: String,
    model: String,
    state: State<'_, Arc<AppState>>,
) -> Result<probe::ModelTestResult, String> {
    let _ = &state;
    let home = home_dir();
    let (base_url, key, style, _) =
        providers::probe_target(&home, &provider_id).map_err(|e| e.to_string())?;
    Ok(probe::test_model(&base_url, &key, style, &model).await)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigBackup {
    path: String,
    at_ms: i64,
}

#[tauri::command]
fn config_backups(state: State<'_, Arc<AppState>>) -> Result<Vec<ConfigBackup>, String> {
    let _ = &state;
    let home = home_dir();
    let dir = config::backup_dir(&home);
    Ok(config::backups(&dir)
        .into_iter()
        .map(|(path, at_ms)| ConfigBackup {
            path: path.to_string_lossy().to_string(),
            at_ms,
        })
        .collect())
}

#[tauri::command]
fn config_restore(path: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let _ = &state;
    let home = home_dir();
    let target = providers::config_path(&home)
        .ok_or_else(|| "config opencode tidak ditemukan".to_string())?;
    // Only a backup inside the config's own directory may be restored.
    let backup = std::path::PathBuf::from(&path);
    let expected_dir = config::backup_dir(&home);
    if backup.parent() != Some(expected_dir.as_path()) {
        return Err("path backup tidak dikenal".to_string());
    }
    config::restore(&backup, &target).map_err(|e| e.to_string())
}

pub const ATTENTION_MODES: [&str; 3] = ["off", "notify", "auto"];
pub const ATTENTION_MODE_KEY: &str = "attention_mode";
pub const NOTIFY_SOUND_KEY: &str = "notify_sound";
pub const STALL_MINUTES_KEY: &str = "stall_minutes";
pub const SLOW_TOOL_MINUTES_KEY: &str = "slow_tool_minutes";
const SETTING_TRUE: &str = "1";

pub const DEFAULT_STALL_MINUTES: i64 = 5;
pub const DEFAULT_SLOW_TOOL_MINUTES: i64 = 10;
const MINUTES_ERROR: &str = "menit harus minimal 1";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Settings {
    attention_mode: String,
    notify_sound: bool,
    stall_minutes: i64,
    slow_tool_minutes: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            attention_mode: "notify".into(),
            notify_sound: false,
            stall_minutes: DEFAULT_STALL_MINUTES,
            slow_tool_minutes: DEFAULT_SLOW_TOOL_MINUTES,
        }
    }
}

/// An unknown value already in the table is a downgrade, not an error: the user
/// gets the conservative default instead of a dashboard that refuses to start.
/// The same goes for a stored minute count that is not a usable positive number.
fn settings_read(store: &Store) -> Result<Settings, String> {
    let mode = match store
        .setting(ATTENTION_MODE_KEY)
        .map_err(|e| e.to_string())?
        .as_deref()
    {
        Some(v) if ATTENTION_MODES.contains(&v) => v.to_string(),
        _ => Settings::default().attention_mode,
    };
    let notify_sound = store
        .setting(NOTIFY_SOUND_KEY)
        .map_err(|e| e.to_string())?
        .map(|v| v == SETTING_TRUE)
        .unwrap_or(false);
    let minutes = |key: &str, fallback: i64| -> Result<i64, String> {
        Ok(store
            .setting(key)
            .map_err(|e| e.to_string())?
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|v| *v >= 1)
            .unwrap_or(fallback))
    };
    Ok(Settings {
        attention_mode: mode,
        notify_sound,
        stall_minutes: minutes(STALL_MINUTES_KEY, DEFAULT_STALL_MINUTES)?,
        slow_tool_minutes: minutes(SLOW_TOOL_MINUTES_KEY, DEFAULT_SLOW_TOOL_MINUTES)?,
    })
}

fn settings_write(store: &mut Store, settings: &Settings) -> Result<Settings, String> {
    if !ATTENTION_MODES.contains(&settings.attention_mode.as_str()) {
        return Err("mode tidak dikenal".into());
    }
    if settings.stall_minutes < 1 || settings.slow_tool_minutes < 1 {
        return Err(MINUTES_ERROR.into());
    }
    store
        .set_setting(ATTENTION_MODE_KEY, &settings.attention_mode)
        .map_err(|e| e.to_string())?;
    store
        .set_setting(
            NOTIFY_SOUND_KEY,
            if settings.notify_sound { SETTING_TRUE } else { "0" },
        )
        .map_err(|e| e.to_string())?;
    store
        .set_setting(STALL_MINUTES_KEY, &settings.stall_minutes.to_string())
        .map_err(|e| e.to_string())?;
    store
        .set_setting(SLOW_TOOL_MINUTES_KEY, &settings.slow_tool_minutes.to_string())
        .map_err(|e| e.to_string())?;
    Ok(settings.clone())
}

/// Read settings only long enough to build the thresholds, then release the lock.
/// The collector lock order must stay: settings, then collector, never reversed.
fn thresholds(state: &Arc<AppState>) -> Thresholds {
    let minutes = settings_read(&state.store.lock().unwrap()).unwrap_or_default();
    Thresholds {
        stall_ms: minutes.stall_minutes * 60_000,
        slow_tool_ms: minutes.slow_tool_minutes * 60_000,
    }
}

#[tauri::command]
fn settings_get(state: State<'_, Arc<AppState>>) -> Result<Settings, String> {
    settings_read(&state.store.lock().unwrap())
}

#[tauri::command]
fn settings_set(settings: Settings, state: State<'_, Arc<AppState>>) -> Result<Settings, String> {
    settings_write(&mut state.store.lock().unwrap(), &settings)
}

/// The frontend asks this before notifying, so a notification never lands on a
/// window the user is already looking at.
#[tauri::command]
fn window_focused(window: tauri::Window) -> bool {
    window.is_focused().unwrap_or(false)
}

/// The six project commands, all of them `Result<T, String>` so a validation
/// failure travels to the UI as the exact Indonesian string it must display.

#[tauri::command]
fn projects_list(state: State<'_, Arc<AppState>>) -> Result<Vec<projects::Project>, String> {
    let rows = state.store.lock().unwrap().projects().map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(projects::to_project).collect())
}

fn next_sort_order(rows: &[collector::store::ProjectRow], requested: i64) -> i64 {
    if requested != 0 {
        return requested;
    }
    rows.iter().map(|r| r.sort_order).max().unwrap_or(0) + 1
}

/// Ids are a timestamp plus a counter, not a dependency: a project is created only
/// from a user gesture, so collisions within one millisecond are theoretical but
/// still cheap to rule out.
fn new_project_id(now: i64, taken: &[collector::store::ProjectRow]) -> String {
    let mut n = 0u32;
    loop {
        let id = format!("p{now:x}-{n:x}");
        if !taken.iter().any(|r| r.id == id) {
            return id;
        }
        n += 1;
    }
}

fn validate_project(
    input: &projects::ProjectInput,
    existing: &[collector::store::ProjectRow],
    editing_id: Option<&str>,
) -> Result<String, String> {
    projects::validate(input, &home_dir(), existing, editing_id)
}

#[tauri::command]
fn project_create(
    input: projects::ProjectInput,
    state: State<'_, Arc<AppState>>,
) -> Result<projects::Project, String> {
    let mut store = state.store.lock().unwrap();
    let existing = store.projects().map_err(|e| e.to_string())?;
    let path = validate_project(&input, &existing, None)?;
    let row = collector::store::ProjectRow {
        id: new_project_id(now_ms(), &existing),
        name: input.name.trim().to_string(),
        path,
        default_profile: input.default_profile.clone(),
        color: input.color.clone(),
        sort_order: next_sort_order(&existing, input.sort_order),
        last_used_ms: None,
    };
    store.upsert_project(&row).map_err(|e| e.to_string())?;
    Ok(projects::to_project(row))
}

#[tauri::command]
fn project_update(
    id: String,
    input: projects::ProjectInput,
    state: State<'_, Arc<AppState>>,
) -> Result<projects::Project, String> {
    let mut store = state.store.lock().unwrap();
    let existing = store.projects().map_err(|e| e.to_string())?;
    let previous = existing
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| "project tidak ditemukan".to_string())?
        .clone();
    let path = validate_project(&input, &existing, Some(&id))?;
    let row = collector::store::ProjectRow {
        id: previous.id,
        name: input.name.trim().to_string(),
        path,
        default_profile: input.default_profile.clone(),
        color: input.color.clone(),
        sort_order: input.sort_order,
        // Editing a project is not using it, so the last-used stamp survives.
        last_used_ms: previous.last_used_ms,
    };
    store.upsert_project(&row).map_err(|e| e.to_string())?;
    Ok(projects::to_project(row))
}

#[tauri::command]
fn project_delete(id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state
        .store
        .lock()
        .unwrap()
        .delete_project(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn project_touch(id: String, state: State<'_, Arc<AppState>>) -> Result<projects::Project, String> {
    let mut store = state.store.lock().unwrap();
    store
        .touch_project(&id, now_ms())
        .map_err(|e| e.to_string())?;
    let row = store
        .projects()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| "project tidak ditemukan".to_string())?;
    Ok(projects::to_project(row))
}

#[tauri::command]
fn project_suggestions(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<projects::ProjectSuggestion>, String> {
    let home = home_dir();
    let prices = state.pricing.lock().unwrap().clone();
    let billing = state.billing.lock().unwrap().clone();
    let live = state
        .collector
        .lock()
        .unwrap()
        .snapshot(now_ms(), &prices, &billing, &thresholds(&state));
    let (known, saved_paths) = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let known = store.known_project_dirs().map_err(|e| e.to_string())?;
        let saved_paths = store
            .projects()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| r.path)
            .collect();
        (known, saved_paths)
    };
    Ok(projects::suggestions(&live, &known, &saved_paths, &home))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptTail {
    entries: Vec<TailEntry>,
    file_size: u64,
    found: bool,
}

const TAIL_ENTRIES: usize = 60;

fn tail_path(projects: &std::path::Path, session_id: &str, cwd: &str, agent_id: Option<&str>) -> Option<PathBuf> {
    let parent = transcript::find_transcript(projects, cwd, session_id)?;
    match agent_id.filter(|id| !id.is_empty()) {
        Some(id) => Some(
            parent
                .with_extension("")
                .join("subagents")
                .join(format!("agent-{id}.jsonl")),
        ),
        None => Some(parent),
    }
}

/// The last records of a session transcript, for the live viewer. Conversation
/// text travels from disk to the caller and is never stored or logged: the index
/// keeps tool names and usage only.
#[tauri::command]
fn session_tail(
    session_id: String,
    cwd: String,
    agent_id: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<TranscriptTail, String> {
    let _ = &state;
    let projects = Paths::for_home(&home_dir()).claude_projects;
    let path = tail_path(&projects, &session_id, &cwd, agent_id.as_deref());
    let missing = || TranscriptTail {
        entries: Vec::new(),
        file_size: 0,
        found: false,
    };
    // A session with no transcript yet is an empty viewer, not a failure.
    let Some(path) = path.filter(|p| p.exists()) else {
        return Ok(missing());
    };
    let (entries, file_size) = read_tail(&path, TAIL_ENTRIES).map_err(|e| e.to_string())?;
    Ok(TranscriptTail {
        entries,
        file_size,
        found: true,
    })
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

    #[test]
    fn savings_wire_shape_matches_the_frontend_contract() {
        let summary = SavingsSummary {
            rtk: SavingsSource {
                available: true,
                saved_tokens: 448714,
                total_tokens: 1939572,
                savings_pct: 23.13,
                entries: 2201,
            },
            lean_ctx: missing_source(),
            daily: vec![SavingsDay {
                date: "2026-03-10".into(),
                rtk_saved: 120,
                lean_ctx_saved: 0,
            }],
            top_commands: vec![SavingsCommand {
                command: "git status".into(),
                saved_tokens: 120,
                savings_pct: 80.0,
                runs: 2,
            }],
            warnings: vec!["lean-ctx: missing".into()],
        };
        let v = serde_json::to_value(&summary).unwrap();
        assert_eq!(v["rtk"]["savedTokens"], 448714);
        assert_eq!(v["rtk"]["totalTokens"], 1939572);
        assert_eq!(v["rtk"]["savingsPct"], 23.13);
        assert_eq!(v["rtk"]["entries"], 2201);
        assert_eq!(v["leanCtx"]["available"], false);
        assert_eq!(v["leanCtx"]["savedTokens"], 0);
        assert_eq!(v["daily"][0]["rtkSaved"], 120);
        assert_eq!(v["daily"][0]["leanCtxSaved"], 0);
        assert_eq!(v["topCommands"][0]["command"], "git status");
        assert_eq!(v["topCommands"][0]["savedTokens"], 120);
        assert_eq!(v["topCommands"][0]["savingsPct"], 80.0);
        assert_eq!(v["topCommands"][0]["runs"], 2);
        assert_eq!(v["warnings"][0], "lean-ctx: missing");
    }

    #[test]
    fn savings_pct_is_zero_when_total_is_zero() {
        assert_eq!(savings_pct(0, 0), 0.0);
        assert_eq!(savings_pct(5, 0), 0.0);
        assert_eq!(savings_pct(50, 200), 25.0);
    }

    #[test]
    fn savings_summary_for_missing_sources_warns_and_returns_zeros() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_str().unwrap();
        let s = savings_summary_for(0, home);
        assert!(!s.rtk.available);
        assert!(!s.lean_ctx.available);
        assert_eq!(s.rtk.saved_tokens, 0);
        assert_eq!(s.daily.len(), 0);
        assert_eq!(s.top_commands.len(), 0);
        assert_eq!(s.warnings.len(), 2);
        assert!(s.warnings.iter().any(|w| w.starts_with("rtk: ")));
        assert!(s.warnings.iter().any(|w| w.starts_with("lean-ctx: ")));
    }

    #[test]
    fn savings_summary_merges_both_sources_by_date() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_path_buf();

        let rtk_dir = home.join("Library/Application Support/rtk");
        std::fs::create_dir_all(&rtk_dir).unwrap();
        let conn = rusqlite::Connection::open(rtk_dir.join("history.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE commands (id INTEGER PRIMARY KEY, timestamp TEXT NOT NULL, \
             original_cmd TEXT NOT NULL, rtk_cmd TEXT NOT NULL, input_tokens INTEGER NOT NULL, \
             output_tokens INTEGER NOT NULL, saved_tokens INTEGER NOT NULL, savings_pct REAL NOT NULL);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO commands (timestamp, original_cmd, rtk_cmd, input_tokens, output_tokens, saved_tokens, savings_pct) \
             VALUES ('2026-03-10T10:00:00.000000+00:00', 'git status -s', 'git status -s', 100, 20, 80, 80.0)",
            [],
        )
        .unwrap();
        drop(conn);

        std::fs::create_dir_all(home.join(".lean-ctx")).unwrap();
        std::fs::write(
            home.join(".lean-ctx/stats.json"),
            r#"{"total_commands": 3, "total_input_tokens": 500, "total_output_tokens": 200,
                "daily": [{"date": "2026-03-10", "commands": 3, "input_tokens": 300, "output_tokens": 100},
                          {"date": "2026-03-12", "commands": 1, "input_tokens": 200, "output_tokens": 100}]}"#,
        )
        .unwrap();

        let s = savings_summary_for(0, home.to_str().unwrap());
        assert!(s.rtk.available);
        assert!(s.lean_ctx.available);
        assert!(s.warnings.is_empty());
        assert_eq!(s.rtk.saved_tokens, 80);
        assert_eq!(s.rtk.total_tokens, 100);
        assert_eq!(s.rtk.savings_pct, 80.0);
        assert_eq!(s.lean_ctx.saved_tokens, 300);
        assert_eq!(s.lean_ctx.total_tokens, 500);
        assert_eq!(s.lean_ctx.savings_pct, 60.0);
        assert_eq!(s.lean_ctx.entries, 3);

        assert_eq!(s.daily.len(), 2);
        assert_eq!(s.daily[0].date, "2026-03-10");
        assert_eq!(s.daily[0].rtk_saved, 80);
        assert_eq!(s.daily[0].lean_ctx_saved, 200);
        assert_eq!(s.daily[1].date, "2026-03-12");
        assert_eq!(s.daily[1].rtk_saved, 0);
        assert_eq!(s.daily[1].lean_ctx_saved, 100);

        assert_eq!(s.top_commands.len(), 1);
        assert_eq!(s.top_commands[0].command, "git status");
        assert_eq!(s.top_commands[0].runs, 1);
    }

    #[test]
    fn savings_command_text_never_leaves_the_raw_line() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_path_buf();
        let rtk_dir = home.join("Library/Application Support/rtk");
        std::fs::create_dir_all(&rtk_dir).unwrap();
        let conn = rusqlite::Connection::open(rtk_dir.join("history.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE commands (id INTEGER PRIMARY KEY, timestamp TEXT NOT NULL, \
             original_cmd TEXT NOT NULL, rtk_cmd TEXT NOT NULL, input_tokens INTEGER NOT NULL, \
             output_tokens INTEGER NOT NULL, saved_tokens INTEGER NOT NULL, savings_pct REAL NOT NULL);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO commands (timestamp, original_cmd, rtk_cmd, input_tokens, output_tokens, saved_tokens, savings_pct) \
             VALUES ('2026-03-10T10:00:00.000000+00:00', \
                     'git status --short --untracked-files=all /Users/yolk/secret-path', \
                     'rtk git status', 100, 20, 80, 80.0)",
            [],
        )
        .unwrap();
        drop(conn);

        let s = savings_summary_for(0, home.to_str().unwrap());
        let wire = serde_json::to_string(&s).unwrap();
        assert!(!wire.contains("secret-path"));
        assert!(!wire.contains("--untracked-files"));
        assert_eq!(s.top_commands[0].command, "git status");
    }

    fn settings_store() -> (tempfile::TempDir, Store) {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        (tmp, store)
    }

    #[test]
    fn settings_default_when_empty() {
        let (_tmp, store) = settings_store();
        let s = settings_read(&store).unwrap();
        assert_eq!(s.attention_mode, "notify");
        assert!(!s.notify_sound);
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["attentionMode"], "notify");
        assert_eq!(v["notifySound"], false);
    }

    #[test]
    fn settings_round_trip_through_commands() {
        let (_tmp, mut store) = settings_store();
        let input = Settings {
            attention_mode: "auto".into(),
            notify_sound: true,
            ..Settings::default()
        };
        let saved = settings_write(&mut store, &input).unwrap();
        assert_eq!(saved, input);
        assert_eq!(settings_read(&store).unwrap(), input);
    }

    #[test]
    fn settings_reject_unknown_mode() {
        let (_tmp, mut store) = settings_store();
        let bad = Settings {
            attention_mode: "turbo".into(),
            notify_sound: true,
            ..Settings::default()
        };
        assert_eq!(
            settings_write(&mut store, &bad),
            Err("mode tidak dikenal".to_string())
        );
        assert_eq!(settings_read(&store).unwrap().attention_mode, "notify");
        assert_eq!(store.setting("attention_mode").unwrap(), None);
    }

    #[test]
    fn settings_unknown_stored_value_falls_back() {
        let (_tmp, mut store) = settings_store();
        store.set_setting("attention_mode", "garbage").unwrap();
        let s = settings_read(&store).unwrap();
        assert_eq!(s.attention_mode, "notify");
        assert!(!s.notify_sound);
    }

    #[test]
    fn settings_default_heartbeat_thresholds() {
        let (_tmp, store) = settings_store();
        let s = settings_read(&store).unwrap();
        assert_eq!(s.stall_minutes, 5);
        assert_eq!(s.slow_tool_minutes, 10);
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["stallMinutes"], 5);
        assert_eq!(v["slowToolMinutes"], 10);
    }

    #[test]
    fn settings_heartbeat_thresholds_round_trip() {
        let (_tmp, mut store) = settings_store();
        let input = Settings {
            attention_mode: "auto".into(),
            notify_sound: true,
            stall_minutes: 3,
            slow_tool_minutes: 20,
        };
        settings_write(&mut store, &input).unwrap();
        let back = settings_read(&store).unwrap();
        assert_eq!(back.stall_minutes, 3);
        assert_eq!(back.slow_tool_minutes, 20);
    }

    #[test]
    fn settings_reject_zero_minutes() {
        let (_tmp, mut store) = settings_store();
        let bad = Settings { stall_minutes: 0, ..Settings::default() };
        assert_eq!(
            settings_write(&mut store, &bad),
            Err("menit harus minimal 1".to_string())
        );
        let bad = Settings { slow_tool_minutes: 0, ..Settings::default() };
        assert_eq!(
            settings_write(&mut store, &bad),
            Err("menit harus minimal 1".to_string())
        );
    }

    fn billing_account(mode: BillingMode) -> BillingAccount {
        BillingAccount {
            id: "a1".into(),
            label: "Claude Max".into(),
            mode,
            matches: vec!["claude-".into()],
            monthly_usd: Some(200.0),
            renewal_day: Some(14),
            credit_usd: Some(20.0),
            started_on: Some("2026-03-01".into()),
            expires_on: None,
        }
    }

    #[test]
    fn billing_defaults_are_an_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = BillingTable::load(&tmp.path().join("billing.json"));
        assert!(loaded.accounts().is_empty(), "no plan is invented for the user");

        let wired = serde_json::to_value(BillingTable::defaults().accounts()).unwrap();
        assert_eq!(wired, serde_json::json!([]));
    }

    #[test]
    fn billing_shape_matches_the_frontend_contract() {
        let v = serde_json::to_value(billing_account(BillingMode::Subscription)).unwrap();
        assert_eq!(v["mode"], "subscription");
        assert_eq!(v["monthlyUsd"], 200.0);
        assert_eq!(v["renewalDay"], 14);
        assert_eq!(v["startedOn"], "2026-03-01");
        assert_eq!(v["expiresOn"], serde_json::Value::Null);
    }

    #[test]
    fn billing_rejects_an_empty_label() {
        let mut a = billing_account(BillingMode::Subscription);
        a.label = "   ".into();
        assert_eq!(validate_account(&a), Err("nama akun tidak boleh kosong".into()));
    }

    #[test]
    fn billing_rejects_an_empty_match_list() {
        let mut a = billing_account(BillingMode::Subscription);
        a.matches = vec!["".into(), " ".into()];
        assert_eq!(
            validate_account(&a),
            Err("isi setidaknya satu awalan model".into())
        );
    }

    #[test]
    fn billing_rejects_an_out_of_range_renewal_day() {
        for day in [0u32, 29, 31] {
            let mut a = billing_account(BillingMode::Subscription);
            a.renewal_day = Some(day);
            assert_eq!(
                validate_account(&a),
                Err("tanggal perpanjangan harus 1 sampai 28".into()),
                "day {day}"
            );
        }
    }

    #[test]
    fn billing_rejects_a_negative_monthly_price() {
        let mut a = billing_account(BillingMode::Subscription);
        a.monthly_usd = Some(-1.0);
        assert_eq!(validate_account(&a), Err("biaya bulanan tidak boleh negatif".into()));
    }

    #[test]
    fn billing_rejects_a_malformed_date() {
        let mut a = billing_account(BillingMode::Subscription);
        a.expires_on = Some("14-03-2026".into());
        assert_eq!(validate_account(&a), Err("tanggal berakhir tidak valid".into()));

        let mut p = billing_account(BillingMode::Prepaid);
        p.started_on = Some("kemarin".into());
        assert_eq!(validate_account(&p), Err("tanggal mulai tidak valid".into()));
    }

    #[test]
    fn billing_requires_the_fields_its_mode_needs() {
        let mut sub = billing_account(BillingMode::Subscription);
        sub.monthly_usd = None;
        assert_eq!(validate_account(&sub), Err("biaya bulanan wajib diisi".into()));
        sub.monthly_usd = Some(200.0);
        sub.renewal_day = None;
        assert_eq!(validate_account(&sub), Err("tanggal perpanjangan wajib diisi".into()));

        let mut pre = billing_account(BillingMode::Prepaid);
        pre.credit_usd = None;
        assert_eq!(validate_account(&pre), Err("saldo awal wajib diisi".into()));
    }

    #[test]
    fn billing_accepts_a_payg_account_with_no_extra_fields() {
        let mut a = billing_account(BillingMode::Payg);
        a.monthly_usd = None;
        a.renewal_day = None;
        a.credit_usd = None;
        a.started_on = None;
        assert_eq!(validate_account(&a), Ok(()));
    }

    #[test]
    fn transcript_tail_wire_shape_matches_the_frontend_contract() {
        let tail = TranscriptTail {
            entries: vec![
                TailEntry::Assistant {
                    ms: 1758526717824,
                    text: "halo".into(),
                    model: Some("claude-sonnet-5".into()),
                },
                TailEntry::Tool {
                    ms: 1758526718000,
                    name: "Bash".into(),
                    input: "{\"command\":\"bun test\"}".into(),
                    status: collector::tail::ToolStatus::Ok,
                },
                TailEntry::Result {
                    ms: 1758526719000,
                    tool_name: "Bash".into(),
                    preview: "3 pass".into(),
                    is_error: false,
                },
            ],
            file_size: 4096,
            found: true,
        };
        let v = serde_json::to_value(&tail).unwrap();
        assert_eq!(v["fileSize"], 4096);
        assert_eq!(v["found"], true);
        assert_eq!(v["entries"][0]["kind"], "assistant");
        assert_eq!(v["entries"][0]["ms"], 1758526717824i64);
        assert_eq!(v["entries"][0]["model"], "claude-sonnet-5");
        assert_eq!(v["entries"][1]["kind"], "tool");
        assert_eq!(v["entries"][1]["status"], "ok");
        assert_eq!(v["entries"][2]["kind"], "result");
        assert_eq!(v["entries"][2]["toolName"], "Bash");
        assert_eq!(v["entries"][2]["isError"], false);
    }

    #[test]
    fn tail_path_selects_the_subagent_transcript() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path();
        let dir = projects.join("-Users-yolk-Dev-kirimi");
        std::fs::create_dir_all(dir.join("s1/subagents")).unwrap();
        std::fs::write(dir.join("s1.jsonl"), "").unwrap();
        std::fs::write(dir.join("s1/subagents/agent-a1.jsonl"), "").unwrap();

        let cwd = "/Users/yolk/Dev/kirimi";
        assert_eq!(
            tail_path(projects, "s1", cwd, None),
            Some(dir.join("s1.jsonl"))
        );
        assert_eq!(
            tail_path(projects, "s1", cwd, Some("a1")),
            Some(dir.join("s1/subagents/agent-a1.jsonl"))
        );
        assert_eq!(tail_path(projects, "s1", cwd, Some("nope")), Some(dir.join("s1/subagents/agent-nope.jsonl")));
        assert_eq!(tail_path(projects, "missing", cwd, None), None);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let home = std::env::var("HOME").unwrap_or_default();
            let dir = data_dir(&app.handle().clone());
            let store = Store::open(&dir.join("agent-deck.db"))
                .expect("failed to open the history index");
            let pricing = PriceTable::load(&dir.join("pricing.json"));
            let billing = BillingTable::load(&dir.join("billing.json"));
            let collector = LiveCollector::new(Paths::for_home(&home), Arc::new(SystemProcessTable));
            let state = Arc::new(AppState {
                collector: Mutex::new(collector),
                store: Mutex::new(store),
                pricing: Mutex::new(pricing),
                billing: Mutex::new(billing),
                indexing: AtomicBool::new(false),
                last_run_ms: Mutex::new(None),
                savings_cache: Mutex::new(None),
                terminals: Mutex::new(TerminalRegistry::new()),
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
                    // The price and settings locks are held only for the clone,
                    // never across the emit or the sleep below.
                    let prices = state.pricing.lock().unwrap().clone();
                    let billing = state.billing.lock().unwrap().clone();
                    let thresholds = thresholds(&state);
                    let snap = state
                        .collector
                        .lock()
                        .unwrap()
                        .snapshot(now_ms(), &prices, &billing, &thresholds);
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
            savings_summary,
            pricing_get,
            pricing_set,
            billing_accounts,
            billing_save,
            billing_summary,
            term_profiles,
            term_start,
            term_list,
            term_write,
            term_resize,
            term_kill,
            term_scrollback,
            models_overview,
            provider_save,
            provider_delete,
            secret_set,
            secret_clear,
            secret_reveal,
            secret_migrate_inline,
            models_fetch,
            model_test,
            config_backups,
            config_restore,
            projects_list,
            project_create,
            project_update,
            project_delete,
            project_touch,
            project_suggestions,
            settings_get,
            settings_set,
            session_tail,
            window_focused
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // No agent process we spawned may outlive the window.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(state) = app.try_state::<Arc<AppState>>() {
                    if let Ok(mut reg) = state.terminals.lock() {
                        reg.kill_all();
                    }
                }
            }
        });
}
