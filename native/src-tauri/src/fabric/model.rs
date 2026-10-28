use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::features;

#[derive(Debug, Clone, Deserialize)]
pub struct ModelData {
    pub version: String,
    pub bias: f32,
    pub weights: HashMap<String, f32>,
    pub risky_tlds: Vec<String>,
    pub tracker_keywords: Vec<String>,
    pub brands: Vec<String>,
    pub known_good: Vec<String>,
    pub permissions: HashMap<String, f32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThreatScore {
    pub host: String,
    /// 0–100.
    pub score: u8,
    pub level: &'static str,
    /// Top contributing signals, human-readable, for explainability.
    pub reasons: Vec<String>,
}

pub struct Model { pub data: ModelData }

pub const BUILTIN: &str = include_str!("../../resources/model.json");

fn reason(name: &str) -> &'static str {
    match name {
        "punycode" => "uses an encoded (punycode) domain",
        "mixed_script" => "mixes alphabets, a common spoofing trick",
        "brand_lookalike" => "looks like a well-known brand",
        "label_entropy" => "random-looking name",
        "digit_ratio" => "unusually many digits",
        "hyphen_count" => "many hyphens",
        "subdomain_depth" => "deeply nested subdomain",
        "risky_tld" => "high-abuse domain ending",
        "tracker_keyword" => "looks like a tracker",
        "ip_literal" => "raw IP address instead of a name",
        "cross_site" => "different site from the app",
        _ => "other signal",
    }
}

impl Model {
    pub fn builtin() -> Self {
        Self { data: serde_json::from_str(BUILTIN).expect("bundled model is valid") }
    }

    pub fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok().map(|data| Self { data })
    }

    pub fn score(&self, host: &str, app_host: &str) -> ThreatScore {
        let feats = features::extract(host, app_host, &self.data);
        let mut z = self.data.bias;
        let mut contrib: Vec<(&str, f32)> = Vec::new();
        for (name, v) in feats {
            let c = self.data.weights.get(name).copied().unwrap_or(0.0) * v;
            z += c;
            if c > 0.3 { contrib.push((name, c)); }
        }
        let p = 1.0 / (1.0 + (-z).exp());
        let score = (p * 100.0).round().clamp(0.0, 100.0) as u8;
        contrib.sort_by(|a, b| b.1.total_cmp(&a.1));
        ThreatScore {
            host: host.to_string(),
            score,
            level: if score >= 70 { "high" } else if score >= 35 { "medium" } else { "low" },
            reasons: contrib.into_iter().take(3).map(|(n, _)| reason(n).to_string()).collect(),
        }
    }

    pub fn permission_base_risk(&self, kind: &str) -> f32 {
        self.data.permissions.get(kind).copied().unwrap_or(0.6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_good_is_low() {
        let m = Model::builtin();
        assert_eq!(m.score("api.notion.so", "notion.so").level, "low");
        assert_eq!(m.score("accounts.google.com", "notion.so").level, "low");
    }
    #[test]
    fn spoofs_are_high() {
        let m = Model::builtin();
        assert_eq!(m.score("xn--gmal-5qa.com", "mail.google.com").level, "high");
        assert_eq!(m.score("paypa1-login.xyz", "notion.so").level, "high");
    }
    #[test]
    fn trackers_are_flagged() {
        let m = Model::builtin();
        assert!(m.score("trk.adnexus-pixel.io", "figma.com").score >= 35);
    }
    #[test]
    fn scoring_is_fast() {
        let m = Model::builtin();
        let t = std::time::Instant::now();
        for _ in 0..10_000 { m.score("telemetry.unknown-host.ru", "notion.so"); }
        assert!(t.elapsed().as_millis() < 500, "10k scores took {:?}", t.elapsed());
    }
}
