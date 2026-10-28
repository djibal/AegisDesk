use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};

use super::{Registry, WrappedApp};
use crate::error::AegisError;

pub fn window_label(id: &str) -> String { format!("app-{id}") }

/// Chromium flags that keep each WebView2 instance near the 30–80 MB target.
#[cfg(windows)]
const LEAN_BROWSER_ARGS: &str = concat!(
    "--renderer-process-limit=1 ",
    "--disable-background-networking ",
    "--disable-component-update ",
    "--disable-default-apps ",
    "--disable-sync ",
    "--disable-extensions ",
    "--disable-features=msSmartScreenProtection,Translate,OptimizationHints,MediaRouter,SpareRendererForSitePerProcess ",
    "--js-flags=--max-old-space-size=96"
);

/// In-page guard for sub-resource requests. Defense-in-depth only: top-level
/// navigation is enforced natively, and Offline mode is enforced by proxy.
fn guard_script(hosts: &[String]) -> String {
    let list = serde_json::to_string(hosts).unwrap_or_else(|_| "[]".into());
    format!(r#"(() => {{
  const H = {list};
  const ok = (u) => {{ try {{ const h = new URL(u, location.href).hostname;
    return H.some(r => {{ r = r.replace(/^\*\./, ''); return h === r || h.endsWith('.' + r); }}); }} catch {{ return false; }} }};
  const deny = (u) => {{ console.warn('[AegisDesk] blocked', String(u)); }};
  const f = window.fetch; window.fetch = function(i, o) {{ const u = i instanceof Request ? i.url : i;
    if (!ok(u)) {{ deny(u); return Promise.reject(new TypeError('Blocked by AegisDesk')); }} return f.call(this, i, o); }};
  const xo = XMLHttpRequest.prototype.open; XMLHttpRequest.prototype.open = function(m, u, ...r) {{
    if (!ok(u)) {{ deny(u); throw new DOMException('Blocked by AegisDesk', 'SecurityError'); }} return xo.call(this, m, u, ...r); }};
  const WS = window.WebSocket; window.WebSocket = function(u, p) {{ if (!ok(u)) {{ deny(u); throw new DOMException('Blocked', 'SecurityError'); }} return new WS(u, p); }};
  window.WebSocket.prototype = WS.prototype;
  const sb = navigator.sendBeacon?.bind(navigator); if (sb) navigator.sendBeacon = (u, d) => ok(u) ? sb(u, d) : (deny(u), false);
  window.open = (u) => {{ if (u) location.href = u; return null; }};
}})();"#)
}

/// Opens (or focuses / reveals a pre-loaded) isolated window for one app.
pub fn launch(handle: &AppHandle, app: &WrappedApp) -> Result<(), AegisError> {
    if let Some(w) = handle.get_webview_window(&window_label(&app.id)) {
        let _ = w.show();
        // Minimize/restore is a desktop window-manager concept; Android has no such API.
        #[cfg(desktop)]
        let _ = w.unminimize();
        let _ = w.set_focus();
        return Ok(());
    }
    build(handle, app, true)
}

/// Pre-load: same isolation, created hidden (used by Workspace AI).
pub fn launch_hidden(handle: &AppHandle, app: &WrappedApp) -> Result<(), AegisError> {
    if handle.get_webview_window(&window_label(&app.id)).is_some() { return Ok(()); }
    build(handle, app, false)
}

fn build(handle: &AppHandle, app: &WrappedApp, visible: bool) -> Result<(), AegisError> {
    let label = window_label(&app.id);

    let url: Url = app.url.parse().map_err(|_| AegisError::InvalidUrl)?;
    let policy = app.network.clone();
    let host = app.host.clone();
    let registry = handle.state::<Registry>();
    let fabric = handle.state::<crate::fabric::Fabric>().inner().clone();
    let app_id = app.id.clone();

    let mut b = WebviewWindowBuilder::new(handle, &label, WebviewUrl::External(url))
        .title(&app.name)
        .visible(visible);
    // WebviewWindowBuilder::focused is compiled only for desktop targets.
    #[cfg(desktop)]
    {
        b = b.focused(visible);
    }
    b = b
        .inner_size(1200.0, 800.0)
        .devtools(false)
        .disable_drag_drop_handler()
        // Native, non-bypassable top-level navigation filter.
        // and Intelligence Fabric threat gate (cached, microseconds).
        .on_navigation(move |target| {
            if !policy.allows_navigation(&host, target) { return false; }
            let Some(h) = target.host_str() else { return true };
            let score = fabric.score(h, &host);
            let blocked = score.score >= 70;
            fabric.report(&app_id, "navigation", score, blocked);
            !blocked
        });

    b = if app.ephemeral {
        b.incognito(true)
    } else {
        b.data_directory(registry.session_dir(&app.id))
    };

    #[cfg(windows)]
    {
        b = b.additional_browser_args(LEAN_BROWSER_ARGS);
    }

    #[cfg(any(windows, target_os = "linux", feature = "macos-proxy"))]
    if let Some(proxy) = app.network.proxy() {
        b = b.proxy_url(proxy);
    }

    if let Some(hosts) = app.network.guard_hosts(&app.host) {
        b = b.initialization_script(&guard_script(&hosts));
    }

    // Remote windows are not listed in any capability, so they have
    // zero access to Tauri IPC / commands (deny-by-default).
    let window = b.build()?;
    let h = handle.clone();
    let id = app.id.clone();
    window.on_window_event(move |e| {
        // Match the boolean payload by value so this compiles whether the
        // callback sees `Focused(bool)` or `Focused(&bool)` (desktop vs mobile).
        let rt = h.state::<crate::runtime::Runtime>();
        match e {
            tauri::WindowEvent::Focused(true) => rt.on_focus(&h, &id),
            tauri::WindowEvent::Focused(false) => rt.on_blur(&id),
            _ => {}
        }
    });
    Ok(())
}

pub fn close(handle: &AppHandle, id: &str) {
    if let Some(w) = handle.get_webview_window(&window_label(id)) {
        let _ = w.destroy(); // frees the WebView process memory immediately
    }
}

/// Deletes all cookies, storage and cache for one app.
pub fn wipe(handle: &AppHandle, id: &str) -> Result<(), AegisError> {
    close(handle, id);
    let dir = handle.state::<Registry>().session_dir(id);
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}
