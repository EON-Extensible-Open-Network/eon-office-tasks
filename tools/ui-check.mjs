// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Drives the real EON Tasks window through the WebView2 DevTools protocol.
// The core is started in a private home, Tasks is installed and launched by
// the core -- as a person's launcher would -- and the window is checked.
// No dependencies (Node 22+), Windows, debug build of Tasks.
//
//   node tools/ui-check.mjs --tasks <eon-tasks.exe> --core <eon-office-core.exe> [--shots <dir>]

import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, existsSync, rmSync } from "node:fs";
import { createConnection } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const arg = (name) => { const at = process.argv.indexOf(name); return at > 0 ? process.argv[at + 1] : undefined; };
const appExe = arg("--tasks") && resolve(arg("--tasks"));
const coreExe = arg("--core") && resolve(arg("--core"));
const shots = arg("--shots");
if (!appExe || !coreExe) { console.error("usage: node tools/ui-check.mjs --tasks <exe> --core <exe> [--shots <dir>]"); process.exit(2); }
if (shots) mkdirSync(shots, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const home = mkdtempSync(join(tmpdir(), "eon-tasks-ui-"));
const port = 9300 + Math.floor(Math.random() * 500);
let failures = 0;
const check = (name, ok, detail = "") => {
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${name}${ok || !detail ? "" : `: ${detail}`}`);
  if (!ok) failures++;
};

function owner() {
  const endpoint = JSON.parse(readFileSync(join(home, "runtime", "endpoint.json"), "utf8"));
  const [host, p] = endpoint.address.split(":");
  const socket = createConnection({ host, port: Number(p) });
  let buffer = ""; let next = 1; const waiting = new Map();
  socket.setEncoding("utf8");
  socket.on("data", (chunk) => {
    buffer += chunk; let at;
    while ((at = buffer.indexOf("\n")) >= 0) {
      const m = JSON.parse(buffer.slice(0, at)); buffer = buffer.slice(at + 1);
      if (!m.method && waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id); }
    }
  });
  const call = (method, params = {}) => new Promise((ok, no) => {
    const id = next++;
    waiting.set(id, (m) => (m.error ? no(m.error) : ok(m.result)));
    socket.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
  });
  const token = readFileSync(join(home, "runtime", "owner.token"), "utf8").trim();
  const ready = new Promise((r) => socket.on("connect", r)).then(() => call("session.hello", { token }));
  return { call, ready, close: () => socket.destroy() };
}

let cdp;
async function devtools() {
  for (let i = 0; i < 300; i++) {
    try {
      const pages = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = pages.find((p) => p.type === "page" && p.webSocketDebuggerUrl);
      if (page) {
        const ws = new WebSocket(page.webSocketDebuggerUrl);
        await new Promise((ok, no) => { ws.onopen = ok; ws.onerror = no; });
        let next = 1; const waiting = new Map();
        ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id); } };
        return { send: (method, params = {}) => new Promise((ok) => { const id = next++; waiting.set(id, ok); ws.send(JSON.stringify({ id, method, params })); }), close: () => ws.close() };
      }
    } catch { /* not yet */ }
    await sleep(200);
  }
  throw new Error("the Tasks window's DevTools endpoint never appeared");
}
async function js(expression) {
  const r = await cdp.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails));
  return r.result?.result?.value;
}
async function until(expression, ms = 8000) {
  const end = Date.now() + ms;
  while (Date.now() < end) { if (await js(expression).catch(() => false)) return true; await sleep(100); }
  return false;
}
async function key(keyName, code = keyName, modifiers = 0, text) {
  const vk = { ArrowDown: 40, ArrowUp: 38, Home: 36, End: 35, ArrowRight: 39, ArrowLeft: 37, Enter: 13 }[keyName] ?? keyName.toUpperCase().charCodeAt(0);
  // Shortcuts with modifiers are raw key presses, as CDP expects; plain keys may carry text.
  await cdp.send("Input.dispatchKeyEvent", { type: modifiers ? "rawKeyDown" : "keyDown", key: keyName, code, modifiers, windowsVirtualKeyCode: vk, text });
  await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", key: keyName, code, modifiers, windowsVirtualKeyCode: vk });
}
// Ctrl+ shortcuts are sent as page-level keyboard events. Synthetic OS-level
// presses through CDP reach a background-started webview only intermittently,
// which tests the delivery, not Notes; arrows still go through CDP below.
async function shortcut(letter) {
  await js(`document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: '${letter}', ctrlKey: true, bubbles: true, cancelable: true }))`);
}
async function type(text) { await cdp.send("Input.insertText", { text }); }
async function shot(name) {
  if (!shots) return;
  const r = await cdp.send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(shots, `tasks-${name}.png`), Buffer.from(r.result.data, "base64"));
}

const core = spawn(coreExe, ["serve", "--home", home], {
  env: { ...process.env, EON_WEBVIEW_ARGS: `--remote-debugging-port=${port}` },
  stdio: "ignore",
});
let o;
try {
  console.log("EON Tasks interface check");
  for (let i = 0; i < 100 && !existsSync(join(home, "runtime", "endpoint.json")); i++) await sleep(100);
  o = owner();
  await o.ready;
  await o.call("prefs.set", { key: "locale", value: "en" });
  const manifest = JSON.parse(readFileSync(new URL("../manifest.json", import.meta.url), "utf8"));
  manifest.entry = { command: appExe };
  await o.call("apps.install", { manifest, grant: manifest.capabilities });
  await o.call("apps.launch", { app: manifest.id });
  cdp = await devtools();
  await cdp.send("Emulation.setFocusEmulationEnabled", { enabled: true });
  const add = async (title, list = "") => js(`(() => { document.getElementById('add-title').value = ${JSON.stringify(title)}; document.getElementById('add-list').value = ${JSON.stringify(list)}; document.getElementById('add-form').requestSubmit(); })()`);
  const rowOf = (title) => `[...document.querySelectorAll('#task-list li')].find(li => li.querySelector('.title').textContent === ${JSON.stringify(title)})`;

  check("the window connects and opens on Today", await until(`document.body.dataset.ready === '1' && document.getElementById('view-title').textContent === 'Today'`, 20000));
  check("the cursor starts in the new-task field", await js(`document.activeElement.id === 'add-title'`));

  await add("Buy bread", "Home");
  check("adding in Today makes a task due today", await until(`${rowOf("Buy bread")}?.querySelector('.meta')?.textContent.startsWith('Today')`));
  check("its list appears in the sidebar", await until(`document.getElementById('lists').textContent.includes('Home')`));
  await shot("01-today");

  await js(`${rowOf("Buy bread")}.querySelector('input[type=checkbox]').click()`);
  check("ticking a task completes it and says so", (await until(`document.getElementById('announcer').textContent === 'Done'`)) && (await until(`!${rowOf("Buy bread")}`)));

  await add("Water the plants", "Home");
  await until(`!!${rowOf("Water the plants")}`);
  await js(`${rowOf("Water the plants")}.querySelector('.title').click()`);
  check("the detail panel opens on the task, title focused", await until(`!document.getElementById('detail').hidden && document.activeElement.id === 'd-title'`));
  await js(`(() => { document.getElementById('d-repeat').value = 'FREQ=DAILY'; document.getElementById('d-notes').value = 'Balcony first'; document.getElementById('detail-form').requestSubmit(); })()`);
  check("saving the detail says so", await until(`document.getElementById('announcer').textContent === 'Saved'`));
  await js(`(() => { document.getElementById('sub-title').value = 'Fill the can'; document.getElementById('sub-form').requestSubmit(); })()`);
  check("a subtask appears under it", await until(`document.getElementById('subtasks').textContent.includes('Fill the can')`));
  await shot("02-detail");
  await js(`${rowOf("Water the plants")}.querySelector('input[type=checkbox]').click()`);
  check("completing a daily task moves it to tomorrow", await until(`document.getElementById('announcer').textContent.startsWith('Done — next on')`));
  await js(`document.getElementById('d-close').click()`);
  check("Close hides the detail panel", await until(`document.getElementById('detail').hidden`));

  await js(`document.querySelector('[data-view="all"]').click()`);
  await js(`document.getElementById('mode-board').click()`);
  check("the board shows three columns", await until(`document.querySelectorAll('#board section').length === 3`));
  await js(`[...document.querySelectorAll('#board li')].find(li => li.textContent.includes('Water the plants')).querySelector('.moves button').click()`);
  check("a card moves to Doing with its button", await until(`document.querySelectorAll('#board section')[1].textContent.includes('Water the plants')`));
  await shot("03-board");
  await js(`document.getElementById('mode-board').focus()`);
  await key("ArrowLeft");
  check("arrows switch between List and Board", await until(`document.activeElement.id === 'mode-list' && !document.getElementById('task-list').hidden`));

  const created = await o.call("apps.call", { app: manifest.id, method: "task.create", params: { title: "From Notes", due: "2030-01-01" } });
  check("task.create from another app opens the new task", !!created.id && (await until(`document.getElementById('d-title').value === 'From Notes'`)));

  await js(`document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`);
  await js(`document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', ctrlKey: true, bubbles: true, cancelable: true }))`);
  check("Ctrl+N goes to the new-task field", await until(`document.activeElement.id === 'add-title'`));

  await o.call("prefs.set", { key: "locale", value: "tr" });
  check("a language change reaches Tasks, including built text", await until(`document.querySelector('[data-view="today"]').textContent === 'Bugün' && document.getElementById('status').textContent.endsWith('görev')`));
  await shot("04-turkish");

  const unnamed = await js(`[...document.querySelectorAll('button, input, select, textarea')].filter(e => {
    if (e.closest('[hidden]')) return false;
    const label = e.getAttribute('aria-label') || e.textContent.trim() || (e.id && document.querySelector('label[for="' + e.id + '"]')?.textContent.trim());
    return !label;
  }).map(e => e.outerHTML.slice(0, 80))`);
  check("every visible control has an accessible name", unnamed.length === 0, JSON.stringify(unnamed));
} catch (error) {
  failures++;
  console.error(`  FAIL  ${error.stack || error}`);
} finally {
  try { await o?.call("core.shutdown"); } catch { /* gone */ }
  o?.close();
  cdp?.close();
  await sleep(1500);
  try { core.kill(); } catch { /* gone */ }
  try { rmSync(home, { recursive: true, force: true }); } catch { /* in use */ }
}
console.log(failures ? `\n${failures} check(s) failed.` : "\nall checks passed");
process.exit(failures ? 1 : 0);
