use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::Model;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    pub app_id: String,
    pub origin_host: String,
    pub app_host: String,
    pub kind: String,
    /// Did the user click/type in the last few seconds?
    pub user_gesture: bool,
    pub window_focused: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Decision { Allow, Ask, Block }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Advice { pub decision: Decision, pub risk: u8, pub reasons: Vec<String> }

/// Learns per-app, per-permission preferences from the user's own choices
/// (Beta-prior counts). Never leaves the device.
#[derive(Default, Serialize, Deserialize)]
pub struct PermissionAdvisor {
    history: HashMap<String, (u32, u32)>, // key -> (allowed, denied)
}

fn key(app: &str, kind: &str) -> String { format!("{app}:{kind}") }

impl PermissionAdvisor {
    pub fn record(&mut self, app_id: &str, kind: &str, allowed: bool) {
        let e = self.history.entry(key(app_id, kind)).or_default();
        if allowed { e.0 += 1 } else { e.1 += 1 }
    }

    pub fn advise(&self, req: &PermissionRequest, model: &Model) -> Advice {
        let mut reasons = Vec::new();
        let mut risk = model.permission_base_risk(&req.kind);
        let origin = model.score(&req.origin_host, &req.app_host);
        risk = risk * 0.6 + (origin.score as f32 / 100.0) * 0.4;
        if origin.score >= 35 { reasons.push(format!("requesting site {}", origin.reasons.join(", "))); }
        if !req.user_gesture { risk += 0.15; reasons.push("asked without you clicking anything".into()); }
        if !req.window_focused { risk += 0.2; reasons.push("asked while the app was in the background".into()); }

        // User history: Beta(1,1) prior -> expected allow rate.
        let (a, d) = self.history.get(&key(&req.app_id, &req.kind)).copied().unwrap_or((0, 0));
        let allow_rate = (a as f32 + 1.0) / ((a + d) as f32 + 2.0);
        let confident = a + d >= 3;
        if confident { risk -= (allow_rate - 0.5) * 0.6; }

        let risk = risk.clamp(0.0, 1.0);
        let decision = if origin.score >= 70 || risk >= 0.8 { Decision::Block }
            else if confident && allow_rate > 0.85 && risk < 0.5 { Decision::Allow }
            else if risk < 0.2 { Decision::Allow }
            else { Decision::Ask };
        if confident { reasons.push(format!("you allowed this {a} of {} times before", a + d)); }
        Advice { decision, risk: (risk * 100.0).round() as u8, reasons }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn req(kind: &str, gesture: bool) -> PermissionRequest {
        PermissionRequest { app_id: "a".into(), origin_host: "meet.google.com".into(), app_host: "meet.google.com".into(),
            kind: kind.into(), user_gesture: gesture, window_focused: true }
    }
    #[test]
    fn camera_asks_by_default() {
        let m = Model::builtin();
        assert_eq!(PermissionAdvisor::default().advise(&req("camera", true), &m).decision, Decision::Ask);
    }
    #[test]
    fn learns_from_user() {
        let m = Model::builtin();
        let mut p = PermissionAdvisor::default();
        for _ in 0..5 { p.record("a", "camera", true); }
        assert_eq!(p.advise(&req("camera", true), &m).decision, Decision::Allow);
    }
    #[test]
    fn spoofed_origin_blocked() {
        let m = Model::builtin();
        let mut r = req("camera", true);
        r.origin_host = "xn--gmal-5qa.com".into();
        assert_eq!(PermissionAdvisor::default().advise(&r, &m).decision, Decision::Block);
    }
    #[test]
    fn background_usb_blocked() {
        let m = Model::builtin();
        let mut r = req("usb", false);
        r.window_focused = false;
        assert_eq!(PermissionAdvisor::default().advise(&r, &m).decision, Decision::Block);
    }
}
