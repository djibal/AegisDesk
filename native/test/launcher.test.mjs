import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { initLauncher } from "../src/launcher.js";
import {
  ANDROID_NOTE,
  ANDROID_STATUS,
  CONTROL_COMMANDS,
  STARTUP_COMMANDS,
} from "../src/platform.js";

const ANDROID_UA = "Mozilla/5.0 (Linux; Android 16; sdk_gphone64_arm64) AppleWebKit/537.36";
const DESKTOP_UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15";

function element(tag) {
  return {
    tag,
    hidden: false,
    textContent: "",
    value: "",
    checked: false,
    children: [],
    listeners: {},
    label: null,
    closest(selector) {
      return selector === "label" ? this.label : null;
    },
    addEventListener(type, fn) {
      this.listeners[type] = fn;
    },
    replaceChildren(...nodes) {
      this.children = nodes;
    },
    append(node) {
      this.children.push(node);
    },
    prepend(node) {
      this.children.unshift(node);
    },
    reset() {},
  };
}

function page() {
  const nodes = {};
  const add = (id, tag, extra = {}) => {
    nodes[id] = Object.assign(element(tag), extra, { id });
    return nodes[id];
  };
  add("status", "p");
  add("mobile-note", "p", { hidden: true });
  add("add", "form");
  add("name", "input", { value: "Mail" });
  add("url", "input", { value: "https://app.example.com" });
  add("mode", "select", { value: "open" });
  add("extra", "input");
  add("ephemeral", "input");
  add("err", "p");
  add("apps", "ul");
  add("preds", "ul");
  add("res", "ul");
  const gov = add("gov", "input", { checked: true });
  gov.label = element("label");
  const eco = add("eco", "input");
  eco.label = element("label");
  add("fstats", "p");
  add("score", "form");
  add("host", "input", { value: "example.com" });
  add("sout", "pre");
  add("threats", "ul");
  return nodes;
}

function start(userAgent) {
  const calls = [];
  const events = [];
  let intervals = 0;
  const nodes = page();
  const invoke = async (cmd, args) => {
    calls.push({ cmd, args });
    if (cmd === "core_status") return { version: "0.4.0", policy: "strict" };
    if (cmd === "list_apps") return [];
    if (cmd === "fabric_stats") return { modelVersion: 1, scored: 0, recent: [] };
    if (cmd === "workspace_predictions") return [];
    if (cmd === "governor_snapshot") return { apps: {}, preloaded: [] };
    if (cmd === "add_app") return { id: "app-1" };
    if (cmd === "score_host") return { score: 10, level: "low", reasons: [] };
    return {};
  };
  initLauncher({
    userAgent,
    invoke,
    listen: (event, handler) => events.push({ event, handler }),
    get: (id) => nodes[id],
    schedule: () => { intervals += 1; },
    createElement: element,
  });
  return { calls, events, intervals: () => intervals, nodes };
}

async function flush() {
  await new Promise((resolve) => setImmediate(resolve));
}

test("the page entry uses the tested launcher initializer", () => {
  const main = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  assert.match(main, /initLauncher\(\{[\s\S]*navigator\.userAgent/);
  assert.equal(main.includes("invoke("), false);
  assert.match(html, /<title>AegisDesk<\/title>/);
  assert.match(html, /<h1>AegisDesk<\/h1>/);
});

test("Android initialization hides unsupported controls and issues no commands", async () => {
  const started = start(ANDROID_UA);
  await flush();
  assert.equal(started.calls.length, 0);
  assert.equal(started.events.length, 0);
  assert.equal(started.intervals(), 0);
  assert.equal(started.nodes.add.hidden, true);
  assert.equal(started.nodes.score.hidden, true);
  assert.equal(started.nodes.gov.label.hidden, true);
  assert.equal(started.nodes.eco.label.hidden, true);
  assert.equal(started.nodes.add.listeners.submit, undefined);
  assert.equal(started.nodes.score.listeners.submit, undefined);
  assert.equal(started.nodes.gov.onchange, undefined);
  assert.equal(started.nodes["mobile-note"].hidden, false);
  assert.equal(started.nodes["mobile-note"].textContent, ANDROID_NOTE);
  assert.equal(started.nodes.status.textContent, ANDROID_STATUS);
  assert.match(started.nodes.apps.children[0].textContent, /No wrapped apps/);
  assert.match(started.nodes.preds.children[0].textContent, /unavailable/);
  assert.match(started.nodes.res.children[0].textContent, /unavailable/);
  assert.match(started.nodes.fstats.textContent, /unavailable/);
});

test("desktop initialization keeps controls and the existing startup commands", async () => {
  const started = start(DESKTOP_UA);
  await flush();
  assert.equal(started.nodes.add.hidden, false);
  assert.equal(started.nodes.score.hidden, false);
  assert.equal(started.nodes.gov.label.hidden, false);
  assert.equal(started.nodes.eco.label.hidden, false);
  assert.equal(started.nodes["mobile-note"].hidden, true);
  const commands = started.calls.map((call) => call.cmd);
  for (const command of STARTUP_COMMANDS) assert.equal(commands.includes(command), true, command);
  for (const command of CONTROL_COMMANDS) assert.equal(commands.includes(command), false, command);
  assert.deepEqual(
    started.events.map((event) => event.event).sort(),
    ["fabric://threat", "governor://suspended", "workspace://preloaded"],
  );
  assert.equal(started.intervals(), 1);
  assert.match(started.nodes.status.textContent, /Secure core v0.4.0 · policy: strict/);
});

test("desktop controls still invoke add, score, and governor commands", async () => {
  const started = start(DESKTOP_UA);
  await flush();
  const before = started.calls.length;
  await started.nodes.add.listeners.submit({ preventDefault() {}, target: { reset() {} } });
  await started.nodes.score.listeners.submit({ preventDefault() {} });
  await started.nodes.gov.onchange();
  const commands = started.calls.slice(before).map((call) => call.cmd);
  assert.deepEqual(commands.filter((cmd) => CONTROL_COMMANDS.includes(cmd)), [
    "add_app",
    "score_host",
    "set_governor",
  ]);
  const governor = started.calls.find((call) => call.cmd === "set_governor");
  assert.deepEqual(governor.args, { enabled: true, batterySaver: false });
});

test("a user agent that only contains the Android letters stays on the desktop path", async () => {
  const started = start("NotAndroid");
  await flush();
  assert.equal(started.nodes.add.hidden, false);
  assert.equal(started.calls.some((call) => call.cmd === "core_status"), true);
});
