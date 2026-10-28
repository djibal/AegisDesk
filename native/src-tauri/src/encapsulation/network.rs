use serde::{Deserialize, Serialize};
use url::Url;

/// Optional per-app networking controls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum NetworkPolicy {
    /// No restrictions beyond HTTPS-only top-level navigation.
    Open,
    /// Only the app's own host (and subdomains) plus listed extra hosts.
    AllowList { hosts: Vec<String> },
    /// All traffic goes through a user-supplied proxy (http:// or socks5://).
    Proxy { url: String },
    /// No network at all: routed to a dead local proxy at the engine level.
    Offline,
}

impl Default for NetworkPolicy {
    fn default() -> Self { NetworkPolicy::AllowList { hosts: vec![] } }
}

/// Unroutable local endpoint; nothing listens on the discard port.
pub const OFFLINE_PROXY: &str = "http://127.0.0.1:9";

fn host_matches(host: &str, rule: &str) -> bool {
    let rule = rule.trim().trim_start_matches("*.").to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    !rule.is_empty() && (host == rule || host.ends_with(&format!(".{rule}")))
}

impl NetworkPolicy {
    pub fn validate(&self) -> bool {
        match self {
            NetworkPolicy::AllowList { hosts } => hosts.iter().all(|h| {
                !h.is_empty() && h.len() < 254 && h.chars().all(|c| c.is_ascii_alphanumeric() || ".-*".contains(c))
            }),
            NetworkPolicy::Proxy { url } => Url::parse(url)
                .map(|u| matches!(u.scheme(), "http" | "socks5") && u.host_str().is_some())
                .unwrap_or(false),
            _ => true,
        }
    }

    /// Native top-level navigation decision (enforced in Rust, not JS).
    pub fn allows_navigation(&self, app_host: &str, target: &Url) -> bool {
        if target.scheme() == "about" || target.scheme() == "blob" || target.scheme() == "data" {
            return true;
        }
        if target.scheme() != "https" {
            return false;
        }
        let Some(host) = target.host_str() else { return false };
        match self {
            NetworkPolicy::Offline => false,
            NetworkPolicy::Open | NetworkPolicy::Proxy { .. } => true,
            NetworkPolicy::AllowList { hosts } => {
                host_matches(host, app_host) || hosts.iter().any(|r| host_matches(host, r))
            }
        }
    }

    pub fn proxy(&self) -> Option<Url> {
        match self {
            NetworkPolicy::Offline => Url::parse(OFFLINE_PROXY).ok(),
            NetworkPolicy::Proxy { url } => Url::parse(url).ok(),
            _ => None,
        }
    }

    /// Hosts the in-page guard allows for sub-resources (fetch/XHR/WS/etc).
    pub fn guard_hosts(&self, app_host: &str) -> Option<Vec<String>> {
        match self {
            NetworkPolicy::AllowList { hosts } => {
                let mut all = vec![app_host.to_string()];
                all.extend(hosts.iter().cloned());
                Some(all)
            }
            NetworkPolicy::Offline => Some(vec![]),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn u(s: &str) -> Url { Url::parse(s).unwrap() }

    #[test]
    fn allowlist_permits_own_host_and_subdomains() {
        let p = NetworkPolicy::AllowList { hosts: vec![] };
        assert!(p.allows_navigation("notion.so", &u("https://notion.so/x")));
        assert!(p.allows_navigation("notion.so", &u("https://www.notion.so/")));
        assert!(!p.allows_navigation("notion.so", &u("https://evil-notion.so/")));
        assert!(!p.allows_navigation("notion.so", &u("https://tracker.io/")));
    }
    #[test]
    fn allowlist_extra_hosts() {
        let p = NetworkPolicy::AllowList { hosts: vec!["*.google.com".into()] };
        assert!(p.allows_navigation("notion.so", &u("https://accounts.google.com/")));
    }
    #[test]
    fn offline_blocks_everything() {
        assert!(!NetworkPolicy::Offline.allows_navigation("a.com", &u("https://a.com/")));
        assert_eq!(NetworkPolicy::Offline.proxy().unwrap().as_str(), "http://127.0.0.1:9/");
    }
    #[test]
    fn http_never_allowed() {
        assert!(!NetworkPolicy::Open.allows_navigation("a.com", &u("http://a.com/")));
    }
    #[test]
    fn proxy_validation() {
        assert!(NetworkPolicy::Proxy { url: "socks5://127.0.0.1:1080".into() }.validate());
        assert!(!NetworkPolicy::Proxy { url: "ftp://x".into() }.validate());
        assert!(!NetworkPolicy::AllowList { hosts: vec!["bad host".into()] }.validate());
    }
}
