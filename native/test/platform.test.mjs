import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  ANDROID_NOTE,
  CONTROL_COMMANDS,
  STARTUP_COMMANDS,
  androidPlaceholder,
  commandsForPlatform,
  isAndroidUserAgent,
  launcherMessage,
} from "../src/platform.js";

const root = new URL("../", import.meta.url);

test("detects Android WebView user agents only", () => {
  assert.equal(
    isAndroidUserAgent("Mozilla/5.0 (Linux; Android 16; sdk_gphone64_arm64) AppleWebKit/537.36"),
    true,
  );
  assert.equal(
    isAndroidUserAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15"),
    false,
  );
  assert.equal(isAndroidUserAgent(""), false);
  assert.equal(isAndroidUserAgent(undefined), false);
  assert.equal(isAndroidUserAgent("NotAndroid"), false);
});

test("Android startup invokes no Tauri commands", () => {
  assert.deepEqual(commandsForPlatform(true), []);
  assert.deepEqual(commandsForPlatform(false), STARTUP_COMMANDS);
  for (const command of ["core_status", "list_apps", "set_governor", ...CONTROL_COMMANDS]) {
    assert.equal(commandsForPlatform(true).includes(command), false);
  }
});

test("permission denials become an availability message on Android", () => {
  assert.equal(launcherMessage("core_status not allowed", true), ANDROID_NOTE);
  assert.equal(launcherMessage("list_apps not allowed", true), ANDROID_NOTE);
  assert.equal(launcherMessage("set_governor not allowed", true), ANDROID_NOTE);
  assert.equal(launcherMessage("core_status not allowed", false), "core_status not allowed");
});

test("Android placeholders do not claim features are active", () => {
  for (const section of ["apps", "preds", "res", "fabric"]) {
    assert.match(androidPlaceholder(section), /unavailable|No wrapped apps/i);
  }
});

test("Android capability still grants no custom commands", () => {
  const android = JSON.parse(readFileSync(new URL("src-tauri/capabilities/android.json", root), "utf8"));
  const desktop = JSON.parse(readFileSync(new URL("src-tauri/capabilities/main.json", root), "utf8"));
  assert.deepEqual(android.permissions, []);
  assert.deepEqual(android.windows, ["main"]);
  assert.deepEqual(android.platforms, ["android"]);
  assert.equal(desktop.platforms.includes("android"), false);
  assert.equal(desktop.permissions.includes("encapsulation-core"), true);
});
