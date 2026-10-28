//! Automated Workspace Management: learns when you use each app
//! (hour-of-week buckets with decay) and pre-loads the likely next app
//! as a hidden window, only when the resource budget allows it.

use std::{collections::HashMap, fs, path::PathBuf, sync::Mutex, time::{SystemTime, UNIX_EPOCH}};

use serde::{Deserialize, Serialize};

const BUCKETS: usize = 168; // 7 days x 24 hours
const DECAY: f32 = 0.98;    // per recorded focus, older habits fade slowly
pub const PRELOAD_MIN_CONFIDENCE: f32 = 0.35;

#[derive(Serialize, Deserialize, Default)]
struct Store { usage: HashMap<String, Vec<f32>> }

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Prediction { pub app_id: String, pub confidence: f32 }

pub struct Workspace { file: PathBuf, store: Mutex<Store> }

/// Hour-of-week from a unix timestamp (UTC; local offset applied by caller).
pub fn bucket(unix_secs: u64, utc_offset_secs: i64) -> usize {
    let t = (unix_secs as i64 + utc_offset_secs).max(0) as u64;
    let hour = (t / 3600) % 24;
    let day = ((t / 86400) + 3) % 7; // 1970-01-01 was a Thursday -> Mon=0
    (day * 24 + hour) as usize
}

pub fn now_secs() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

impl Workspace {
    pub fn load(file: PathBuf) -> Self {
        let store = fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { file, store: Mutex::new(store) }
    }

    pub fn in_memory() -> Self { Self { file: PathBuf::new(), store: Mutex::new(Store::default()) } }

    /// Called when an app window gains focus.
    pub fn record(&self, app_id: &str, at_bucket: usize) {
        let mut s = self.store.lock().unwrap();
        for v in s.usage.values_mut() { v.iter_mut().for_each(|x| *x *= DECAY); }
        let v = s.usage.entry(app_id.to_string()).or_insert_with(|| vec![0.0; BUCKETS]);
        v[at_bucket % BUCKETS] += 1.0;
        // Light smoothing: habits at 9:00 also hint at 8:00 and 10:00.
        v[(at_bucket + BUCKETS - 1) % BUCKETS] += 0.25;
        v[(at_bucket + 1) % BUCKETS] += 0.25;
        if !self.file.as_os_str().is_empty() {
            if let Ok(b) = serde_json::to_vec(&*s) { let _ = fs::write(&self.file, b); }
        }
    }

    pub fn forget(&self, app_id: &str) { self.store.lock().unwrap().usage.remove(app_id); }

    /// Ranked predictions for a given hour-of-week bucket.
    pub fn predict(&self, at_bucket: usize, exclude: &[String]) -> Vec<Prediction> {
        let s = self.store.lock().unwrap();
        let scores: Vec<(String, f32)> = s.usage.iter()
            .filter(|(id, _)| !exclude.contains(id))
            .map(|(id, v)| (id.clone(), v[at_bucket % BUCKETS] + 0.5 * v[(at_bucket + 1) % BUCKETS]))
            .collect();
        let total: f32 = scores.iter().map(|(_, x)| x).sum();
        if total <= f32::EPSILON { return vec![]; }
        let mut out: Vec<Prediction> = scores.into_iter()
            .map(|(app_id, x)| Prediction { app_id, confidence: (x / total * 100.0).round() / 100.0 })
            .filter(|p| p.confidence > 0.0).collect();
        out.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bucket_math() {
        assert_eq!(bucket(0, 0), 3 * 24); // Thu 00:00
        assert_eq!(bucket(4 * 86400 + 9 * 3600, 0), 9); // Mon 09:00
    }
    #[test]
    fn predicts_habit() {
        let w = Workspace::in_memory();
        for _ in 0..10 { w.record("slack", 9); }
        for _ in 0..2 { w.record("figma", 9); }
        w.record("figma", 15);
        let p = w.predict(9, &[]);
        assert_eq!(p[0].app_id, "slack");
        assert!(p[0].confidence > PRELOAD_MIN_CONFIDENCE);
    }
    #[test]
    fn excludes_open_apps() {
        let w = Workspace::in_memory();
        w.record("slack", 9);
        assert!(w.predict(9, &["slack".into()]).is_empty());
    }
}
