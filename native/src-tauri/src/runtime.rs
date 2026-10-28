//! Background supervisor that ties the Governor and Workspace together.
//! Runs on a single low-priority thread, waking every 5 seconds.

use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    encapsulation::{session, Registry},
    governor::{enforce, policy::Action, AppUsage, Limits},
    workspace::{self, Workspace, PRELOAD_MIN_CONFIDENCE},
};

const TICK: Duration = Duration::from_secs(5);
const PRELOAD_TTL: Duration = Duration::from_secs(10 * 60);
/// Total RAM budget for all wrapped apps before pre-loading is skipped.
const PRELOAD_BUDGET_MB: f32 = 1200.0;

#[derive(Default)]
pub struct RuntimeState {
    pub usage: HashMap<String, AppUsage>,
    pub last_focus: HashMap<String, Instant>,
    pub focused: Option<String>,
    pub preloaded: HashMap<String, Instant>,
    pub battery_saver: bool,
    pub enabled: bool,
}

pub struct Runtime(pub Mutex<RuntimeState>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernorSnapshot { pub enabled: bool, pub battery_saver: bool, pub apps: HashMap<String, AppUsage>, pub preloaded: Vec<String> }

impl Runtime {
    pub fn new() -> Self { Runtime(Mutex::new(RuntimeState { enabled: true, ..Default::default() })) }

    pub fn on_focus(&self, handle: &AppHandle, app_id: &str) {
        let mut s = self.0.lock().unwrap();
        s.focused = Some(app_id.to_string());
        s.last_focus.insert(app_id.to_string(), Instant::now());
        s.preloaded.remove(app_id); // the prediction paid off
        drop(s);
        let ws = handle.state::<Workspace>();
        ws.record(app_id, workspace::bucket(workspace::now_secs(), local_offset()));
    }

    pub fn on_blur(&self, app_id: &str) {
        let mut s = self.0.lock().unwrap();
        if s.focused.as_deref() == Some(app_id) { s.focused = None; }
        s.last_focus.insert(app_id.to_string(), Instant::now());
    }

    pub fn snapshot(&self) -> GovernorSnapshot {
        let s = self.0.lock().unwrap();
        GovernorSnapshot { enabled: s.enabled, battery_saver: s.battery_saver, apps: s.usage.clone(), preloaded: s.preloaded.keys().cloned().collect() }
    }

    pub fn set_limits(&self, app_id: &str, limits: Option<Limits>) {
        let mut s = self.0.lock().unwrap();
        let u = s.usage.entry(app_id.to_string()).or_default();
        u.limits = limits.map(|l| Limits { manual: true, ..l }).unwrap_or_default();
    }
}

/// Local UTC offset is not in std; users can override via AEGIS_UTC_OFFSET (secs).
fn local_offset() -> i64 { std::env::var("AEGIS_UTC_OFFSET").ok().and_then(|v| v.parse().ok()).unwrap_or(0) }

fn data_dir_of(cmd: &[std::ffi::OsString]) -> Option<String> {
    cmd.iter().find_map(|a| a.to_str()?.strip_prefix("--user-data-dir=").map(|s| s.trim_matches('"').to_lowercase()))
}

pub fn start(handle: AppHandle) {
    thread::Builder::new().name("aegis-governor".into()).stack_size(512 * 1024).spawn(move || {
        let mut sys = System::new();
        let kind = ProcessRefreshKind::new().with_cpu().with_memory().with_cmd(UpdateKind::OnlyIfNotSet);
        let cores = std::thread::available_parallelism().map(|n| n.get() as f32).unwrap_or(1.0);
        loop {
            thread::sleep(TICK);
            let runtime = handle.state::<Runtime>();
            if !runtime.0.lock().unwrap().enabled { continue; }
            sys.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);

            let registry = handle.state::<Registry>();
            let apps = registry.apps.lock().unwrap().clone();
            let dir_to_app: HashMap<String, String> = apps.iter()
                .map(|a| (registry.session_dir(&a.id).to_string_lossy().to_lowercase(), a.id.clone()))
                .collect();

            // Sum all WebView2 processes (browser, renderer, GPU, utility) per app.
            let mut totals: HashMap<String, (f32, f32, Vec<u32>)> = HashMap::new();
            for (pid, p) in sys.processes() {
                let Some(dir) = data_dir_of(p.cmd()) else { continue };
                let Some(app_id) = dir_to_app.iter().find(|(d, _)| dir.starts_with(d.as_str())).map(|(_, id)| id.clone()) else { continue };
                let e = totals.entry(app_id).or_default();
                e.0 += p.memory() as f32 / 1_048_576.0;
                e.1 += p.cpu_usage() / cores;
                e.2.push(pid.as_u32());
            }

            let open: HashSet<String> = apps.iter().filter(|a| handle.get_webview_window(&session::window_label(&a.id)).is_some()).map(|a| a.id.clone()).collect();
            let mut suspend = vec![];
            {
                let mut s = runtime.0.lock().unwrap();
                let battery = s.battery_saver;
                let focused = s.focused.clone();
                for (app_id, (ram, cpu, pids)) in &totals {
                    let idle = s.last_focus.get(app_id).map(|t| t.elapsed().as_secs()).unwrap_or(0);
                    let is_focused = focused.as_deref() == Some(app_id.as_str());
                    let preloaded = s.preloaded.contains_key(app_id);
                    let u = s.usage.entry(app_id.clone()).or_default();
                    for action in u.observe(*ram, *cpu, is_focused, idle, battery) {
                        match action {
                            Action::LowerPriority => pids.iter().for_each(|p| enforce::set_low_priority(*p, true)),
                            Action::RestorePriority => pids.iter().for_each(|p| enforce::set_low_priority(*p, false)),
                            Action::TrimMemory => pids.iter().for_each(|p| enforce::trim(*p)),
                            Action::Suspend if !preloaded => suspend.push(app_id.clone()),
                            _ => {}
                        }
                    }
                }
                // Expire unused pre-loads.
                let expired: Vec<String> = s.preloaded.iter().filter(|(_, t)| t.elapsed() > PRELOAD_TTL).map(|(k, _)| k.clone()).collect();
                for id in expired { s.preloaded.remove(&id); suspend.push(id); }
            }
            for id in &suspend { session::close(&handle, id); }
            if !suspend.is_empty() { let _ = handle.emit_to("main", "governor://suspended", &suspend); }

            // Workspace pre-load: at most one hidden window, only within budget.
            let total_ram: f32 = totals.values().map(|t| t.0).sum();
            let has_preload = !runtime.0.lock().unwrap().preloaded.is_empty();
            if !has_preload && total_ram < PRELOAD_BUDGET_MB && !runtime.0.lock().unwrap().battery_saver {
                let ws = handle.state::<Workspace>();
                let next_hour = workspace::bucket(workspace::now_secs() + 600, local_offset());
                let exclude: Vec<String> = open.iter().cloned().collect();
                if let Some(p) = ws.predict(next_hour, &exclude).into_iter().find(|p| p.confidence >= PRELOAD_MIN_CONFIDENCE) {
                    if let Ok(app) = registry.get(&p.app_id) {
                        if !app.ephemeral && session::launch_hidden(&handle, &app).is_ok() {
                            runtime.0.lock().unwrap().preloaded.insert(app.id.clone(), Instant::now());
                            let _ = handle.emit_to("main", "workspace://preloaded", &app.id);
                        }
                    }
                }
            }
        }
    }).expect("spawn governor");
}
