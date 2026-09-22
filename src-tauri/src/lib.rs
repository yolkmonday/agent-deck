use collector::live::{LiveCollector, Paths};
use collector::model::LiveSnapshot;
use collector::process::SystemProcessTable;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, State};

struct AppState {
    collector: Mutex<LiveCollector>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[tauri::command]
fn live_snapshot(state: State<'_, Arc<AppState>>) -> LiveSnapshot {
    state.collector.lock().unwrap().snapshot(now_ms())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let home = std::env::var("HOME").unwrap_or_default();
            let collector = LiveCollector::new(Paths::for_home(&home), Arc::new(SystemProcessTable));
            let state = Arc::new(AppState { collector: Mutex::new(collector) });
            app.manage(state.clone());
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last: Option<LiveSnapshot> = None;
                loop {
                    let snap = state.collector.lock().unwrap().snapshot(now_ms());
                    if last.as_ref().map_or(true, |l| !l.same_content(&snap)) {
                        let _ = handle.emit("live://snapshot", &snap);
                        last = Some(snap);
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![live_snapshot])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
