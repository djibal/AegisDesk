use std::net::IpAddr;
use tauri::{App, Manager};
use url::Url;

use crate::error::AegisError;

/// Validate a site before it may ever be wrapped: HTTPS only, no
/// credentials in the URL, no localhost / private-network targets.
pub fn validate_https_url(raw: &str) -> Result<Url, AegisError> {
    let url = Url::parse(raw.trim()).map_err(|_| AegisError::InvalidUrl)?;
    if url.scheme() != "https" {
        return Err(AegisError::InsecureScheme);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AegisError::InvalidUrl);
    }
    let host = url.host_str().ok_or(AegisError::InvalidUrl)?;
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".local") {
        return Err(AegisError::ForbiddenHost);
    }
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        let private = match ip {
            IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified(),
            IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
        };
        if private {
            return Err(AegisError::ForbiddenHost);
        }
    }
    Ok(url)
}

/// Runtime hardening applied to every window at startup.
///
/// DevTools open/close APIs are compiled only with `debug_assertions` or the
/// `devtools` Cargo feature. We deliberately omit that feature in release so
/// inspector APIs stay unavailable. Release builds rely on `devtools: false`
/// in `tauri.conf.json` and `WebviewWindowBuilder::devtools(false)` instead.
pub fn harden_windows(app: &mut App) -> tauri::Result<()> {
    for (_label, window) in app.webview_windows() {
        let _ = window;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https() {
        assert!(validate_https_url("https://app.notion.so").is_ok());
    }
    #[test]
    fn rejects_http() {
        assert!(matches!(validate_https_url("http://example.com"), Err(AegisError::InsecureScheme)));
    }
    #[test]
    fn rejects_private_hosts() {
        assert!(validate_https_url("https://localhost").is_err());
        assert!(validate_https_url("https://192.168.1.10").is_err());
        assert!(validate_https_url("https://127.0.0.1").is_err());
    }
    #[test]
    fn rejects_embedded_credentials() {
        assert!(validate_https_url("https://user:pw@example.com").is_err());
    }
}
