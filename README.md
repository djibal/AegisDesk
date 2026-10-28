# AegisDesk — v0.4 — Foundation, Encapsulation Core, Intelligence Fabric, Workspace AI, Resource Governor

Tauri 2 project with a Rust backend and a WebView2 frontend on Windows. It has no Electron dependencies.

## Requirements (Windows)
- Rust (stable, 1.77 or newer) with the MSVC toolchain
- Microsoft C++ Build Tools ("Desktop development with C++")
- WebView2 Runtime (built into Windows 11; the installer adds it on Windows 10)
- Node.js 18 or newer

See **ARCHITECTURE.md** for the full review, a map of requirements to code, and known limitations.

## Run
```
npm install
npx tauri icon path/to/logo.png   # generates the icon files in src-tauri/icons
npm run tauri dev
```
Build installers: `npm run tauri build`. Run tests: `cd src-tauri && cargo test`.

## Deny-by-default model
| Layer | What it does |
|---|---|
| `build.rs` app manifest | App commands can't be called from the frontend unless a permission grants them |
| `permissions/app.toml` | Each command has its own permission |
| `capabilities/main.json` | Gives only the `main` window access to 2 commands and the event listen/unlisten permissions. Filesystem, shell, HTTP, clipboard and remote URLs are not granted |
| `tauri.conf.json` CSP | Only the app's own files (`'self'`) plus IPC are allowed; no inline scripts, frames, objects or form posts |
| `freezePrototype`, `withGlobalTauri: false` | Stops pages from tampering with built-in JavaScript objects, and leaves no global Tauri object on `window` |
| `devtools: false` + runtime hardening | No inspector in release builds |
| `security::validate_https_url` | Accepts only HTTPS sites; blocks localhost, private network addresses and passwords embedded in the URL |

## Layout
```
index.html, src/          frontend (vanilla JS + Vite)
src-tauri/
  Cargo.toml, build.rs, tauri.conf.json
  capabilities/main.json  window grants
  permissions/app.toml    per-command permissions
  src/main.rs, lib.rs     entry + builder
  src/commands.rs         IPC commands
  src/security.rs         URL validation + window hardening (with tests)
  src/error.rs            safe IPC errors
```

## Encapsulation Core (Sequence 2)

**Isolation for each app**
- Every wrapped site opens in its own window, named `app-<id>`.
- **Persistent apps:** each app keeps its own data folder (`sessions/<id>`). Cookies, saved site data, the cache and service workers are never shared between apps.
- **Ephemeral apps:** these run in incognito mode and keep nothing on disk.
- **No backend access:** app windows aren't listed in any capability, so wrapped sites can't call any backend command.
- **New windows:** pop-ups are forced into the same window so the navigation filter checks them. Developer tools and drag-and-drop are turned off.
- **Wipe data:** deletes everything stored for one app. **Remove** deletes the app together with its data.

**Optional network controls for each app (`NetworkPolicy`)**
| Mode | Top-level navigation (enforced in Rust) | Other requests |
|---|---|---|
| `allowList` (default) | The app's own domain, its subdomains and any extra hosts you list | An in-page guard on fetch, XHR, WebSocket, sendBeacon and window.open |
| `open` | Any HTTPS site | No restriction |
| `proxy` | Any HTTPS site | All traffic goes through your proxy (http or socks5) |
| `offline` | Blocked | All traffic goes to a dead local proxy, so it is blocked by the browser engine itself |

Plain-HTTP navigation is always blocked. Changing an app's network policy closes its window; the new policy applies the next time you open it.

> **Limitation:** in allow-list mode, the in-page guard is a second line of defence, not a hard boundary. Sub-requests can be blocked by the engine itself either with WebView2 `WebResourceRequested` on Windows or a small local filtering proxy. That's planned for Threat Shield (Phase 2).

**Memory (target: 30–80 MB per app)**
- **One process per app on Windows:** WebView2 is limited to one page process per app, and background networking, sync, extensions, translation and component updates are turned off.
- **Script memory cap:** each page's JavaScript memory is capped at about 96 MB.
- **Closing frees memory:** a closed app's window is destroyed, which releases its memory straight away. The app list is a small JSON file kept in memory.
- **Tradeoff:** a separate data folder for each app gives the strongest isolation, but each app also runs its own copy of WebView2's shared browser process. If the per-app memory goes over budget, the next step is WebView2 profiles: they share one browser process while keeping each app's storage separate.

**Build options**
- **macOS proxy:** proxy and offline modes on macOS need macOS 14+ and building with `--features macos-proxy`.
- **Tests:** `cd src-tauri && cargo test` checks the network policy rules and the URL validation.

## Intelligence Fabric (Sequence 3)

**Fully local**
- **The model:** a small scoring model whose weights sit in a 2 KB JSON file bundled with the app (`resources/model.json`). It needs no internet connection and makes no cloud calls.
- **Offline updates:** to update the model, drop a newer `model.json` into the app data folder. It is picked up at the next launch.
- **What it learns:** it learns from your own allow/deny choices, stored in `permission-history.json` on your device only.

**Threat scoring (`fabric::model`)**
- **Signals:** it checks 13 signals for each domain:
  - encoded (punycode) domains, mixed alphabets and brand lookalikes (with digit-for-letter swaps such as `paypa1`)
  - random-looking names, digits, hyphens and deeply nested subdomains
  - high-abuse domain endings, tracker keywords, raw IP addresses and cross-site requests
  - known-good domains, which lower the score
- **Output:** a score from 0 to 100, a low/medium/high level, and up to three plain-language reasons.
- **Built into navigation:** every page an app tries to open is scored. A score of 70 or more is blocked, and the launcher shows it as a threat event.

**Permission advice (`fabric::permissions`)**
- **Inputs:** the base risk of the permission, the score of the site asking, whether you just clicked or typed, and whether the window was in the foreground.
- **Learning:** your past choices for each app and permission are folded in.
- **Output:** Allow, Ask or Block, with reasons.
- **Commands:** `advise_permission` and `record_permission`.

**Performance (`fabric::worker`)**
| Measure | Effect |
|---|---|
| Lightweight scoring | Pure arithmetic: 10,000 scores in well under 0.5 s (there's a test for this), with no neural network loaded in memory |
| Result cache (2,048 entries) | Repeat domains cost one lookup |
| One background thread with a small stack | UI notifications are sent in batches every 0.5 s |
| Bounded queue (256) | When full, events are dropped rather than slowing apps down (the drop counter is shown in `fabric_stats`) |
| Memory | Well under 1 MB for the model, cache and logs |

**Security:** the new commands sit behind their own `allow-fabric` permission, granted only to the launcher window. Wrapped sites still can't call any backend command.

> **Not yet connected:** WebView2 permission prompts (camera, microphone and so on) aren't wired to `advise_permission` yet. That requires the `webview2-com` crate's `PermissionRequested` handler, at a version matching Tauri's. Until then the advisor is reachable through its commands.

## Workspace AI and Resource Governor (v0.4)
- **Workspace:** learns which apps you use at each hour of the week. Each hour before, it quietly pre-loads the most likely next app in a hidden window. Opening that app is then instant. The pre-load is closed if you don't use it within 10 minutes.
- **Governor:** every 5 seconds it measures each app's RAM and CPU use (all of its WebView2 processes combined), learns its normal usage, and sets each app's own limits automatically.
  - **CPU limit:** a background app that goes over its CPU limit gets lower priority.
  - **RAM limit:** an app that goes over its RAM limit has unused memory released.
  - **Idle apps:** an app left in the background for 15 minutes with no activity is suspended. It reloads with your session intact when you open it again.
  - **Battery saver:** tightens all limits.
  - **Manual override:** `set_app_limits` lets you set your own limits for an app.
