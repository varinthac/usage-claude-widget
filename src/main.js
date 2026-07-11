// Usage Widget for Claude — frontend logic.
// The OAuth token never reaches this side: all credential handling lives in Rust.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const views = ["usage", "error", "onboarding", "settings"];

const MAX_BACKOFF_MS = 10 * 60 * 1000;

let intervalMs = clampInterval(parseInt(localStorage.getItem("intervalMs") || "60000", 10));
let backoffMs = null;
let pollTimer = null;
let snapshot = null;
let fetchedAt = null;
let settingsOpen = false;

function clampInterval(ms) {
  return Number.isFinite(ms) ? Math.min(Math.max(ms, 30000), 600000) : 60000;
}

function showView(name) {
  for (const v of views) $(`view-${v}`).hidden = v !== name;
}

function setMood(mood) {
  $("mascot").dataset.mood = mood;
}

// ---------- rendering ----------

function fmtCountdown(resetsAt) {
  if (!resetsAt) return "";
  const ms = new Date(resetsAt) - Date.now();
  if (ms <= 0) return "resetting…";
  const min = Math.ceil(ms / 60000);
  const d = Math.floor(min / 1440), h = Math.floor((min % 1440) / 60), m = min % 60;
  if (d > 0) return `resets in ${d} d ${h} hr`;
  if (h > 0) return `resets in ${h} hr ${m} min`;
  return `resets in ${m} min`;
}

function fmtAgo(ts) {
  const s = Math.max(0, Math.round((Date.now() - ts) / 1000));
  if (s < 5) return "just now";
  if (s < 60) return `${s} s ago`;
  return `${Math.round(s / 60)} min ago`;
}

function sevClass(limit) {
  const p = limit.percent ?? 0;
  if (p >= 95 || limit.severity === "critical" || limit.severity === "exceeded") return "danger";
  if (p >= 80 || (limit.severity && limit.severity !== "normal")) return "warn";
  return "ok";
}

function renderUsage() {
  if (!snapshot) return;
  const bars = $("bars");
  bars.replaceChildren();
  for (const lim of snapshot.limits) {
    const pct = Math.round(lim.percent ?? 0);
    const el = document.createElement("div");
    el.className = `limit ${sevClass(lim)}`;
    el.title = lim.resets_at ? `Resets at ${new Date(lim.resets_at).toLocaleString()}` : "";

    const meta = document.createElement("div");
    meta.className = "meta";
    const name = document.createElement("span");
    name.className = "name";
    name.textContent = lim.label;
    const pctEl = document.createElement("span");
    pctEl.className = "pct";
    pctEl.textContent = `${pct}%`;
    meta.append(name, pctEl);

    const bar = document.createElement("div");
    bar.className = "bar";
    const fill = document.createElement("div");
    fill.className = "fill";
    fill.style.width = `${Math.min(pct, 100)}%`;
    bar.append(fill);

    const reset = document.createElement("span");
    reset.className = "reset";
    reset.textContent = fmtCountdown(lim.resets_at);

    el.append(meta, bar, reset);
    bars.append(el);
  }

  if (snapshot.plan) {
    $("plan").textContent = snapshot.plan;
    $("plan").hidden = false;
  }
  $("updated").textContent = `Updated ${fmtAgo(fetchedAt)}`;

  const worst = Math.max(0, ...snapshot.limits.map((l) => l.percent ?? 0));
  setMood(worst >= 80 ? "panic" : worst >= 50 ? "worried" : "happy");

  const session = snapshot.limits.find((l) => l.kind === "session");
  const week = snapshot.limits.find((l) => l.kind === "weekly_all");
  const parts = [];
  if (session) parts.push(`Session ${Math.round(session.percent)}%`);
  if (week) parts.push(`Week ${Math.round(week.percent)}%`);
  invoke("update_tray", { tooltip: parts.join(" · ") || "Claude usage" }).catch(() => {});

  if (!settingsOpen) showView("usage");
}

function renderMeta() {
  if (!snapshot || $("view-usage").hidden) return;
  $("updated").textContent = `Updated ${fmtAgo(fetchedAt)}`;
  const resets = document.querySelectorAll("#bars .limit");
  snapshot.limits.forEach((lim, i) => {
    const el = resets[i]?.querySelector(".reset");
    if (el) el.textContent = fmtCountdown(lim.resets_at);
  });
}

// ---------- fetching ----------

function schedule(ms) {
  clearTimeout(pollTimer);
  pollTimer = setTimeout(fetchUsage, ms);
}

async function fetchUsage() {
  const btn = $("btn-refresh");
  btn.classList.add("spinning");
  try {
    snapshot = await invoke("get_usage");
    fetchedAt = Date.now();
    backoffMs = null;
    renderUsage();
    schedule(intervalMs);
  } catch (err) {
    handleError(err);
    backoffMs = Math.min((backoffMs || intervalMs) * 2, MAX_BACKOFF_MS);
    schedule(backoffMs);
  } finally {
    btn.classList.remove("spinning");
  }
}

function handleError(err) {
  const code = err?.code || "unknown";
  setMood("sleepy");
  if (code === "no-credentials") {
    if (!settingsOpen) showView("onboarding");
    return;
  }
  const messages = {
    "token-expired": "Your Claude Code token has expired.\nOpen Claude Code once to refresh it, then retry.",
    unauthorized: "The token was rejected.\nLog in to Claude Code again, or paste a fresh token.",
    network: "Can’t reach Anthropic right now.\nWill keep retrying in the background.",
    "rate-limited": (err?.message || "Anthropic is rate-limiting requests right now.") + "\nThis is temporary — the widget will keep retrying automatically.",
    api: err?.message || "Anthropic API returned an error.",
    unknown: err?.message || String(err),
  };
  $("error-msg").textContent = messages[code] || messages.unknown;
  if (!settingsOpen) showView("error");
}

// ---------- settings ----------

async function openSettings() {
  settingsOpen = true;
  $("sel-interval").value = String(intervalMs);
  try {
    const st = await invoke("get_settings_state");
    $("chk-aot").checked = st.always_on_top;
    $("chk-autostart").checked = st.autostart;
  } catch { /* non-fatal */ }
  await refreshTokenSource();
  showView("settings");
}

async function refreshTokenSource() {
  try {
    const cs = await invoke("credentials_status");
    const label = !cs.found
      ? "No credentials configured"
      : cs.source === "claude-code"
        ? "Token: auto from Claude Code ✓"
        : "Token: saved manually ✓";
    $("token-source").textContent = label;
    $("btn-clear-token").hidden = cs.source !== "manual";
  } catch {
    $("token-source").textContent = "";
  }
}

function closeSettings() {
  settingsOpen = false;
  if (snapshot) { renderUsage(); showView("usage"); } else { fetchUsage(); showView("usage"); }
}

// ---------- wiring ----------

$("btn-hide").addEventListener("click", () => invoke("hide_window"));
$("btn-refresh").addEventListener("click", fetchUsage);
$("btn-retry").addEventListener("click", fetchUsage);
$("btn-onboard-retry").addEventListener("click", fetchUsage);
$("btn-settings").addEventListener("click", openSettings);
$("btn-back").addEventListener("click", closeSettings);
$("btn-error-token").addEventListener("click", () => { settingsOpen = false; showView("onboarding"); });

$("btn-save-token").addEventListener("click", async () => {
  const input = $("token-input");
  const status = $("onboard-status");
  try {
    await invoke("save_manual_token", { token: input.value });
    input.value = "";
    status.textContent = "Saved to Windows Credential Manager ✓";
    await fetchUsage();
  } catch (err) {
    status.textContent = err?.message || String(err);
  }
});

$("btn-clear-token").addEventListener("click", async () => {
  await invoke("clear_manual_token").catch(() => {});
  await refreshTokenSource();
});

$("sel-interval").addEventListener("change", (e) => {
  intervalMs = clampInterval(parseInt(e.target.value, 10));
  localStorage.setItem("intervalMs", String(intervalMs));
  if (!backoffMs) schedule(intervalMs);
});

$("chk-aot").addEventListener("change", (e) => {
  invoke("set_always_on_top", { on: e.target.checked }).catch(() => {});
  localStorage.setItem("aot", e.target.checked ? "1" : "0");
});

$("chk-autostart").addEventListener("change", (e) => {
  invoke("set_autostart", { on: e.target.checked }).catch(() => {});
});

listen("tray-refresh", fetchUsage);
listen("aot-changed", (ev) => {
  localStorage.setItem("aot", ev.payload ? "1" : "0");
  $("chk-aot").checked = !!ev.payload;
});
listen("autostart-changed", (ev) => {
  $("chk-autostart").checked = !!ev.payload;
});

// ---------- startup ----------

const savedAot = localStorage.getItem("aot");
if (savedAot === "1" || savedAot === "0") {
  invoke("set_always_on_top", { on: savedAot === "1" }).catch(() => {});
}

setInterval(renderMeta, 10000);
fetchUsage();
