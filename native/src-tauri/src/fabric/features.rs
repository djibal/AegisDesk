use unicode_normalization::UnicodeNormalization;

use super::model::ModelData;

/// Registrable-domain approximation (last two labels; enough for scoring).
pub fn base_domain(host: &str) -> String {
    let parts: Vec<&str> = host.trim_end_matches('.').split('.').collect();
    if parts.len() <= 2 { host.to_string() } else { parts[parts.len() - 2..].join(".") }
}

fn entropy(s: &str) -> f32 {
    if s.is_empty() { return 0.0; }
    let mut counts = [0u32; 256];
    for b in s.bytes() { counts[b as usize] += 1; }
    let n = s.len() as f32;
    counts.iter().filter(|&&c| c > 0).map(|&c| { let p = c as f32 / n; -p * p.log2() }).sum()
}

/// Skeleton for lookalike detection: fold common confusables to ASCII.
fn skeleton(s: &str) -> String {
    s.nfkd()
        .filter(|c| c.is_ascii())
        .collect::<String>()
        .to_ascii_lowercase()
        .replace('0', "o").replace('1', "l").replace("rn", "m").replace('3', "e").replace('5', "s")
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + (ca != *cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Extract (feature, value) pairs. `app_host` is the wrapped app's own host.
pub fn extract(host: &str, app_host: &str, m: &ModelData) -> Vec<(&'static str, f32)> {
    let host = host.to_ascii_lowercase();
    let base = base_domain(&host);
    let label = base.split('.').next().unwrap_or("");
    let tld = host.rsplit('.').next().unwrap_or("");
    let is_ip = host.parse::<std::net::IpAddr>().is_ok();
    let non_ascii = host.chars().any(|c| !c.is_ascii());
    let has_latin = host.chars().any(|c| c.is_ascii_alphabetic());

    let known = m.known_good.iter().any(|k| host == *k || host.ends_with(&format!(".{k}")));
    let sk = skeleton(label);
    let lookalike = !known
        && m.brands.iter().any(|b| sk != *b && (sk.contains(b.as_str()) || levenshtein(&sk, b) == 1));
    let digits = label.chars().filter(|c| c.is_ascii_digit()).count() as f32 / label.len().max(1) as f32;

    vec![
        ("punycode", host.split('.').any(|l| l.starts_with("xn--")) as u8 as f32),
        ("mixed_script", (non_ascii && has_latin) as u8 as f32),
        ("brand_lookalike", lookalike as u8 as f32),
        ("label_entropy", (entropy(label) - 3.0).max(0.0)),
        ("digit_ratio", digits),
        ("hyphen_count", label.matches('-').count() as f32),
        ("subdomain_depth", (host.matches('.').count() as f32 - 1.0).max(0.0)),
        ("host_length", host.len() as f32),
        ("risky_tld", m.risky_tlds.iter().any(|t| t == tld) as u8 as f32),
        ("tracker_keyword", m.tracker_keywords.iter().any(|k| host.contains(k.as_str())) as u8 as f32),
        ("ip_literal", is_ip as u8 as f32),
        ("cross_site", (base != base_domain(app_host)) as u8 as f32),
        ("known_good", known as u8 as f32),
    ]
}
