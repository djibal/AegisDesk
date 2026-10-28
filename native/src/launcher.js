import {
  ANDROID_NOTE,
  ANDROID_STATUS,
  androidPlaceholder,
  isAndroidUserAgent,
  launcherMessage,
} from "./platform.js";

function policyFrom(mode, extra) {
  if (mode === "allowList") return { mode, hosts: extra.split(",").map((h) => h.trim()).filter(Boolean) };
  if (mode === "proxy") return { mode, url: extra.trim() };
  return { mode };
}

const describe = (n) => n.mode === "allowList" ? `own domain${n.hosts.length ? " + " + n.hosts.join(", ") : ""}` : n.mode === "proxy" ? `proxy ${n.url}` : n.mode;

/**
 * Starts the launcher. Android shows explanations and hides controls that
 * would call commands denied by the Android capability. Desktop keeps the
 * existing command wiring.
 */
export function initLauncher({
  userAgent,
  invoke,
  listen,
  get,
  schedule = setInterval,
  createElement = document.createElement.bind(document),
}) {
  const android = isAndroidUserAgent(userAgent);
  const $ = get;
  const err = (e) => ($("err").textContent = launcherMessage(e, android));

  function showAndroidLauncher() {
    $("status").textContent = ANDROID_STATUS;
    const note = $("mobile-note");
    note.hidden = false;
    note.textContent = ANDROID_NOTE;
    $("add").hidden = true;
    $("score").hidden = true;
    $("gov").closest("label").hidden = true;
    $("eco").closest("label").hidden = true;
    const item = (section) => {
      const li = createElement("li");
      li.textContent = androidPlaceholder(section);
      return li;
    };
    $("apps").replaceChildren(item("apps"));
    $("preds").replaceChildren(item("preds"));
    $("res").replaceChildren(item("res"));
    $("fstats").textContent = androidPlaceholder("fabric");
  }

  async function render() {
    const apps = await invoke("list_apps");
    const ul = $("apps");
    ul.replaceChildren();
    for (const a of apps) {
      const li = createElement("li");
      const info = createElement("span");
      info.textContent = `${a.name} — ${a.host} · ${describe(a.network)}${a.ephemeral ? " · ephemeral" : ""}`;
      li.append(info);
      for (const [label, cmd] of [["Open", "launch_app"], ["Close", "close_app"], ["Wipe data", "wipe_session"], ["Remove", "remove_app"]]) {
        const b = createElement("button");
        b.textContent = label;
        b.onclick = () => invoke(cmd, { id: a.id }).then(() => (err(), render())).catch(err);
        li.append(b);
      }
      ul.append(li);
    }
  }

  const addThreat = (ev) => {
    const li = createElement("li");
    li.textContent = `${ev.blocked ? "BLOCKED" : "flagged"} · ${ev.score.host} · ${ev.score.score}/100 · ${ev.score.reasons.join("; ")}`;
    $("threats").prepend(li);
  };

  let names = {};
  async function refreshRuntime() {
    const apps = await invoke("list_apps");
    names = Object.fromEntries(apps.map((a) => [a.id, a.name]));
    const preds = await invoke("workspace_predictions");
    $("preds").replaceChildren(...(preds.length ? preds : [{ appId: "", confidence: 0 }]).map((p) => {
      const li = createElement("li");
      li.textContent = p.appId ? `${names[p.appId] ?? p.appId} · ${Math.round(p.confidence * 100)}% likely next` : "Learning your habits…";
      return li;
    }));
    const g = await invoke("governor_snapshot");
    $("res").replaceChildren(...Object.entries(g.apps).map(([id, u]) => {
      const li = createElement("li");
      li.textContent = `${names[id] ?? id} · ${u.state} · RAM ${u.ramMb.toFixed(0)}/${u.limits.ramMb} MB · CPU ${u.cpuPct.toFixed(1)}/${u.limits.cpuPct}%${u.limits.manual ? " (manual)" : " (auto)"}${g.preloaded.includes(id) ? " · pre-loaded" : ""}`;
      return li;
    }));
  }

  if (android) {
    showAndroidLauncher();
    return;
  }

  invoke("core_status")
    .then((s) => ($("status").textContent = `Secure core v${s.version} · policy: ${s.policy}`))
    .catch((e) => ($("status").textContent = `Core unavailable: ${e}`));

  $("add").addEventListener("submit", async (e) => {
    e.preventDefault();
    try {
      await invoke("add_app", { input: {
        name: $("name").value, url: $("url").value, ephemeral: $("ephemeral").checked,
        network: policyFrom($("mode").value, $("extra").value),
      } });
      e.target.reset(); err(); render();
    } catch (x) { err(x); }
  });

  render().catch(err);

  listen("fabric://threat", (e) => addThreat(e.payload));
  invoke("fabric_stats").then((s) => {
    $("fstats").textContent = `Local model v${s.modelVersion} · ${s.scored} scored · runs offline`;
    s.recent.reverse().forEach(addThreat);
  });
  $("score").addEventListener("submit", async (e) => {
    e.preventDefault();
    const s = await invoke("score_host", { host: $("host").value.trim(), appHost: "" }).catch(err);
    if (s) $("sout").textContent = `${s.score}/100 (${s.level})\n${s.reasons.join("\n") || "no risk signals"}`;
  });

  const pushGov = () => invoke("set_governor", { enabled: $("gov").checked, batterySaver: $("eco").checked }).catch(err);
  $("gov").onchange = pushGov;
  $("eco").onchange = pushGov;
  listen("governor://suspended", refreshRuntime);
  listen("workspace://preloaded", refreshRuntime);
  refreshRuntime().catch(err);
  schedule(() => refreshRuntime().catch(() => {}), 5000);
}
