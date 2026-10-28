/**
 * Android System WebView user agents contain the token "Android".
 * This only chooses launcher copy. Tauri capabilities remain the command boundary.
 */
export function isAndroidUserAgent(userAgent) {
  return typeof userAgent === "string" && /\bAndroid\b/i.test(userAgent);
}

export const ANDROID_STATUS =
  "Android launcher · managed apps stay on desktop until isolation is verified";

export const ANDROID_NOTE =
  "App wrapping, threat scoring, workspace predictions, and the resource governor are unavailable on this Android build. Their commands stay denied so a wrapped page cannot call them.";

const PLACEHOLDERS = {
  apps: "No wrapped apps on Android.",
  preds: "Workspace predictions are unavailable on Android.",
  res: "Resource limits are unavailable on Android.",
  fabric: "Threat scoring is unavailable on Android.",
};

export function androidPlaceholder(section) {
  return PLACEHOLDERS[section] ?? ANDROID_NOTE;
}

/** Commands the desktop launcher issues before the user acts. */
export const STARTUP_COMMANDS = [
  "core_status",
  "list_apps",
  "fabric_stats",
  "workspace_predictions",
  "governor_snapshot",
];

/** Commands behind visible controls. */
export const CONTROL_COMMANDS = [
  "add_app",
  "launch_app",
  "close_app",
  "wipe_session",
  "remove_app",
  "score_host",
  "set_governor",
];

export function commandsForPlatform(android) {
  return android ? [] : STARTUP_COMMANDS.slice();
}

/** Android must not surface raw Tauri permission denials. */
export function launcherMessage(error, android) {
  if (!android) return error ? String(error) : "";
  return ANDROID_NOTE;
}
