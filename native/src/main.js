import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { initLauncher } from "./launcher.js";

initLauncher({
  userAgent: navigator.userAgent,
  invoke,
  listen,
  get: (id) => document.getElementById(id),
});
