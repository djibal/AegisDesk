# AegisDesk — Architecture Review & Rebuild (v0.4)

Principal architect review of Sequences 1–3 plus the Expanded Scope, with the decisions taken in this rebuild.

## 1. Requirements traceability
| Requirement | Status | Where |
|---|---|---|
| S1 Tauri 2, Rust backend, WebView2 on Windows | Done | `src-tauri/`, `tauri.conf.json` (WebView2 embed bootstrapper) |
| S1 Deny-by-default | Done | `build.rs` app manifest, `permissions/app.toml`, `capabilities/main.json`, strict CSP |
| S1 No Electron | Done | No Electron anywhere in the dependency tree |
| S2 Isolated sessions / sandboxing | Done | `encapsulation/session.rs` (one window and data folder per app, incognito option, zero IPC) |
| S2 Optional per-app networking | Done | `encapsulation/network.rs` (allow-list, open, proxy, offline) |
| S2 30–80 MB per app | Designed, **not yet measured** | Lean WebView2 flags, renderer limit, JS heap cap, governor trimming |
| S3 Local AI permissions and threat scoring | Done | `fabric/` (offline model, permission advisor) |
| S3 Low performance impact | Done | Microsecond scoring, cache, bounded queue, perf test |
| Scope: AI threat intelligence on traffic | Partial | Navigation scored natively; sub-resource interception planned (see §4) |
| Scope: workspace prediction and pre-load | **New in v0.4** | `workspace/`, `runtime.rs` |
| Scope: dynamic RAM/CPU limits | **New in v0.4** | `governor/`, `runtime.rs` |

## 2. Layer map
```text
 Launcher window ("main")  -- only window with capabilities
        | IPC (deny-by-default, per-command permissions)
 Rust core
  ├─ encapsulation/  registry, sessions, network policy
  ├─ fabric/         threat model, permission advisor, event worker
  ├─ workspace/      hour-of-week habit model, predictions
  ├─ governor/       usage learning, dynamic limits, OS enforcement
  └─ runtime.rs      5 s supervisor thread: governor + pre-loading
        |
 App windows (app-<id>)  -- WebView2, own data folder, NO IPC
```

## 3. Review findings and fixes in this rebuild
1. **Expanded-scope features existed only in the web demo.** Now implemented in Rust.
2. **App focus wasn't tracked.** Window focus events now feed both the habit model and the idle detection.
3. **Pre-loading could hurt performance.** It is limited to one hidden window, only when total app RAM is under 1.2 GB, never in battery saver, never for ephemeral apps, and auto-closed after 10 minutes if unused.
4. **Learned limits could ratchet upward.** Peaks decay over time, limits are clamped (64–1024 MB, 5–100% CPU), and manual overrides always win.
5. **Removing an app left its habit data behind.** Removing now also clears its workspace history.

## 4. Known limitations / next sequences
- **Process tracking on Windows:** WebView2 processes are matched to apps by their `--user-data-dir`. Ephemeral (incognito) apps can't be matched, so they are not governed.
- **CPU limits are soft:** they work by lowering process priority. A hard cap would need Windows Job Objects with `JOB_OBJECT_CPU_RATE_CONTROL`. That's straightforward, but WebView2 processes must be assigned to the job, and they are created after the webview starts.
- **macOS and Linux:** usage is monitored and limits are calculated, but they are not yet enforced.
- **Time zone:** the habit model uses UTC unless `AEGIS_UTC_OFFSET` is set. Add the `time` crate with local-offset support when needed.
- **Not yet connected:** native WebView2 `PermissionRequested` and `WebResourceRequested` handlers (for permission prompts and sub-resource threat scoring). Both need `webview2-com` pinned to the same version Tauri uses.
- **Memory check:** RAM per app must be checked on real Windows hardware before the 30–80 MB claim is published.

## 5. Verification
`cd src-tauri && cargo test` runs these tests:
- **URL safety:** HTTPS-only URL checks.
- **Network policy:** the allow-list, offline and proxy rules.
- **Threat scores:** known-good sites score low, spoofed sites score high, trackers get flagged, and a performance budget is enforced.
- **Permission advice:** the advisor's decisions and how it learns from your choices.
- **Workspace:** the time-bucket maths and predictions.
- **Governor:** adaptive limits, clamping, manual overrides, deprioritising, memory trimming, idle suspend and battery saver.
