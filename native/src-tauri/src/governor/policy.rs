use serde::{Deserialize, Serialize};

pub const RAM_MIN_MB: f32 = 64.0;
pub const RAM_MAX_MB: f32 = 1024.0;
pub const CPU_MIN: f32 = 5.0;
pub const CPU_MAX: f32 = 100.0;
pub const IDLE_SUSPEND_SECS: u64 = 15 * 60;
const ALPHA: f32 = 0.2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Limits { pub ram_mb: f32, pub cpu_pct: f32, pub manual: bool }

impl Default for Limits { fn default() -> Self { Self { ram_mb: 512.0, cpu_pct: 50.0, manual: false } } }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUsage {
    pub ram_mb: f32,
    pub cpu_pct: f32,
    pub avg_ram_mb: f32,
    pub avg_cpu_pct: f32,
    pub peak_ram_mb: f32,
    pub peak_cpu_pct: f32,
    pub samples: u32,
    pub limits: Limits,
    pub idle_secs: u64,
    pub state: String,
}

#[derive(Debug, PartialEq)]
pub enum Action { None, LowerPriority, RestorePriority, TrimMemory, Suspend }

impl AppUsage {
    /// Feed one sample; returns actions to take this tick.
    pub fn observe(&mut self, ram_mb: f32, cpu_pct: f32, focused: bool, idle_secs: u64, battery_saver: bool) -> Vec<Action> {
        self.ram_mb = ram_mb;
        self.cpu_pct = cpu_pct;
        self.idle_secs = idle_secs;
        if self.samples == 0 { self.avg_ram_mb = ram_mb; self.avg_cpu_pct = cpu_pct; }
        self.avg_ram_mb += ALPHA * (ram_mb - self.avg_ram_mb);
        self.avg_cpu_pct += ALPHA * (cpu_pct - self.avg_cpu_pct);
        // Peaks decay slowly so one spike doesn't inflate limits forever.
        self.peak_ram_mb = (self.peak_ram_mb * 0.995).max(ram_mb);
        self.peak_cpu_pct = (self.peak_cpu_pct * 0.99).max(cpu_pct);
        self.samples = self.samples.saturating_add(1);

        if !self.limits.manual && self.samples >= 12 {
            let headroom = if battery_saver { 1.1 } else { 1.3 };
            self.limits.ram_mb = (self.peak_ram_mb * headroom).clamp(RAM_MIN_MB, RAM_MAX_MB).round();
            let cpu_cap = if battery_saver { 40.0 } else { CPU_MAX };
            self.limits.cpu_pct = (self.peak_cpu_pct * headroom).clamp(CPU_MIN, cpu_cap).round();
        }

        let mut actions = vec![];
        if !focused && idle_secs >= IDLE_SUSPEND_SECS && cpu_pct < 1.0 {
            self.state = "suspending".into();
            return vec![Action::Suspend];
        }
        if cpu_pct > self.limits.cpu_pct && !focused { actions.push(Action::LowerPriority); }
        else if focused { actions.push(Action::RestorePriority); }
        if ram_mb > self.limits.ram_mb { actions.push(Action::TrimMemory); }
        self.state = if focused { "active" } else if cpu_pct < 1.0 { "idle" } else { "background" }.into();
        if actions.is_empty() { actions.push(Action::None); }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limits_adapt_to_usage() {
        let mut u = AppUsage::default();
        for _ in 0..20 { u.observe(150.0, 10.0, true, 0, false); }
        assert_eq!(u.limits.ram_mb, 195.0);
        assert_eq!(u.limits.cpu_pct, 13.0);
    }
    #[test]
    fn limits_clamped() {
        let mut u = AppUsage::default();
        for _ in 0..20 { u.observe(20.0, 1.0, true, 0, false); }
        assert_eq!(u.limits.ram_mb, RAM_MIN_MB);
        assert_eq!(u.limits.cpu_pct, CPU_MIN);
    }
    #[test]
    fn manual_limits_respected() {
        let mut u = AppUsage { limits: Limits { ram_mb: 300.0, cpu_pct: 30.0, manual: true }, ..Default::default() };
        for _ in 0..20 { u.observe(100.0, 5.0, true, 0, false); }
        assert_eq!(u.limits.ram_mb, 300.0);
    }
    #[test]
    fn background_cpu_hog_deprioritised() {
        let mut u = AppUsage { limits: Limits { ram_mb: 500.0, cpu_pct: 20.0, manual: true }, ..Default::default() };
        assert!(u.observe(100.0, 60.0, false, 0, false).contains(&Action::LowerPriority));
    }
    #[test]
    fn over_ram_trims() {
        let mut u = AppUsage { limits: Limits { ram_mb: 100.0, cpu_pct: 50.0, manual: true }, ..Default::default() };
        assert!(u.observe(180.0, 2.0, true, 0, false).contains(&Action::TrimMemory));
    }
    #[test]
    fn long_idle_suspends() {
        let mut u = AppUsage::default();
        assert_eq!(u.observe(80.0, 0.0, false, IDLE_SUSPEND_SECS, false), vec![Action::Suspend]);
        assert_eq!(u.observe(80.0, 0.0, true, IDLE_SUSPEND_SECS, false).first(), Some(&Action::RestorePriority));
    }
    #[test]
    fn battery_saver_caps_cpu() {
        let mut u = AppUsage::default();
        for _ in 0..20 { u.observe(150.0, 80.0, true, 0, true); }
        assert_eq!(u.limits.cpu_pct, 40.0);
    }
}
