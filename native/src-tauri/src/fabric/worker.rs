use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex, RwLock},
    thread,
    time::Duration,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::{permissions::Advice, Model, PermissionAdvisor, PermissionRequest, ThreatScore};

const CACHE_MAX: usize = 2048;
const QUEUE_MAX: usize = 256;
const LOG_MAX: usize = 200;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreatEvent { pub app_id: String, pub kind: String, pub score: ThreatScore, pub blocked: bool }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricStats { pub model_version: String, pub cached: usize, pub scored: u64, pub dropped: u64, pub recent: Vec<ThreatEvent> }

struct Cache { map: HashMap<String, ThreatScore>, order: VecDeque<String> }

/// Shared, cheap-to-clone handle to the Intelligence Fabric.
#[derive(Clone)]
pub struct Fabric(Arc<Inner>);

struct Inner {
    model: RwLock<Model>,
    cache: Mutex<Cache>,
    advisor: Mutex<PermissionAdvisor>,
    log: Mutex<VecDeque<ThreatEvent>>,
    tx: mpsc::SyncSender<ThreatEvent>,
    counters: Mutex<(u64, u64)>,
    state_file: PathBuf,
}

impl Fabric {
    pub fn start(handle: &AppHandle) -> Self {
        let dir = handle.path().app_data_dir().unwrap_or_default();
        // Optional offline model update: drop a newer model.json in the data dir.
        let model = fs::read_to_string(dir.join("model.json")).ok()
            .and_then(|s| Model::from_json(&s)).unwrap_or_else(Model::builtin);
        let state_file = dir.join("permission-history.json");
        let advisor = fs::read(&state_file).ok()
            .and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();

        let (tx, rx) = mpsc::sync_channel::<ThreatEvent>(QUEUE_MAX);
        let fabric = Fabric(Arc::new(Inner {
            model: RwLock::new(model),
            cache: Mutex::new(Cache { map: HashMap::new(), order: VecDeque::new() }),
            advisor: Mutex::new(advisor),
            log: Mutex::new(VecDeque::new()),
            tx,
            counters: Mutex::new((0, 0)),
            state_file,
        }));

        // One background thread batches UI notifications so bursts of
        // requests never flood the IPC bridge or the launcher window.
        let h = handle.clone();
        let f = fabric.clone();
        thread::Builder::new().name("aegis-fabric".into()).stack_size(256 * 1024).spawn(move || {
            let mut batch = Vec::new();
            loop {
                match rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(ev) => batch.push(ev),
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => break,
                }
                if !batch.is_empty() && (batch.len() >= 32 || rx.try_recv().is_err()) {
                    let mut log = f.0.log.lock().unwrap();
                    for ev in batch.drain(..) {
                        if log.len() == LOG_MAX { log.pop_back(); }
                        log.push_front(ev.clone());
                        let _ = h.emit_to("main", "fabric://threat", ev);
                    }
                }
            }
        }).expect("spawn fabric worker");
        fabric
    }

    /// Hot path: cached, allocation-light, safe to call from the navigation filter.
    pub fn score(&self, host: &str, app_host: &str) -> ThreatScore {
        let key = format!("{app_host}|{host}");
        if let Some(s) = self.0.cache.lock().unwrap().map.get(&key) { return s.clone(); }
        let s = self.0.model.read().unwrap().score(host, app_host);
        let mut c = self.0.cache.lock().unwrap();
        if c.map.len() >= CACHE_MAX { if let Some(old) = c.order.pop_front() { c.map.remove(&old); } }
        c.order.push_back(key.clone());
        c.map.insert(key, s.clone());
        self.0.counters.lock().unwrap().0 += 1;
        s
    }

    /// Fire-and-forget report; drops if the queue is full (never blocks).
    pub fn report(&self, app_id: &str, kind: &str, score: ThreatScore, blocked: bool) {
        if score.score < 35 && !blocked { return; }
        let ev = ThreatEvent { app_id: app_id.into(), kind: kind.into(), score, blocked };
        if self.0.tx.try_send(ev).is_err() { self.0.counters.lock().unwrap().1 += 1; }
    }

    pub fn advise(&self, req: &PermissionRequest) -> Advice {
        let model = self.0.model.read().unwrap();
        self.0.advisor.lock().unwrap().advise(req, &model)
    }

    pub fn record_decision(&self, app_id: &str, kind: &str, allowed: bool) {
        let mut a = self.0.advisor.lock().unwrap();
        a.record(app_id, kind, allowed);
        if let Ok(bytes) = serde_json::to_vec(&*a) { let _ = fs::write(&self.0.state_file, bytes); }
    }

    pub fn stats(&self) -> FabricStats {
        let (scored, dropped) = *self.0.counters.lock().unwrap();
        FabricStats {
            model_version: self.0.model.read().unwrap().data.version.clone(),
            cached: self.0.cache.lock().unwrap().map.len(),
            scored, dropped,
            recent: self.0.log.lock().unwrap().iter().take(50).cloned().collect(),
        }
    }
}
