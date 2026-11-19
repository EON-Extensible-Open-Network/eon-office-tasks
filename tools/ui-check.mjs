// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Drives the real EON Tasks window through the WebView2 DevTools protocol.
// Tasks is started on its own in a private EON Office home -- nothing else is
// installed or running, which is the point -- and the window is checked.
// No dependencies (Node 22+), Windows, debug build of Tasks.
//
//   node tools/ui-check.mjs --tasks <eon-tasks.exe> [--shots <dir>]

import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const arg = (name) => { const at = process.argv.indexOf(name); return at > 0 ? process.argv[at + 1] : undefined; };
const appExe = arg("--tasks") && resolve(arg("--tasks"));
const shots = arg("--shots");
if (!appExe) { console.error("usage: node tools/ui-check.mjs --tasks <exe> [--shots <dir>]"); process.exit(2); }
if (shots) mkdirSync(shots, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const home = mkdtempSync(join(tmpdir(), "eon-tasks-ui-"));
const port = 9300 + Math.floor(Math.random() * 500);
let failures = 0;
const check = (name, ok, detail = "") => {
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${name}${ok || !detail ? "" : `: ${detail}`}`);
  if (!ok) failures++;
};

const env = { ...process.env, EON_OFFICE_HOME: home, EON_WEBVIEW_ARGS: `--remote-debugging-port=${port}` };
// A shared preference changed elsewhere -- as the launcher would.
function setPref(key, value) {
  const r = spawnSync(appExe, ["--set-pref", key, JSON.stringify(value)], { env, encoding: "utf8" });
  if (r.status !== 0) throw new Error(`--set-pref ${key}: ${r.stderr}`);
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

let app;
try {
  console.log("EON Tasks interface check");
  setPref("locale", "en");
  app = spawn(appExe, [], { env, stdio: "ignore" });
  cdp = await devtools();
  await cdp.send("Emulation.setFocusEmulationEnabled", { enabled: true });
  const add = async (title, list = "") => js(`(() => { document.getElementById('add-title').value = ${JSON.stringify(title)}; document.getElementById('add-list').value = ${JSON.stringify(list)}; document.getElementById('add-form').requestSubmit(); })()`);
  const rowOf = (title) => `[...document.querySelectorAll('#task-list li')].find(li => li.querySelector('.title').textContent === ${JSON.stringify(title)})`;

  check("the window opens on its own, on Today", await until(`document.body.dataset.ready === '1' && document.getElementById('view-title').textContent === 'Today'`, 20000));
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

  // Another application opens a task with a link: a second start hands it to
  // this window and exits.
  const plants = await js(`(async () => (await window.__TAURI__.core.invoke('view', { view: 'all', today: '2026-11-19' })).tasks.find(t => t.title === 'Water the plants').id)()`);
  const second = spawn(appExe, [`eon-office://org.eon.office.tasks/${plants}`], { env, stdio: "ignore" });
  const exited = await new Promise((done) => { const timer = setTimeout(() => done(false), 15000); second.on("exit", () => { clearTimeout(timer); done(true); }); });
  check("a link opens the task in the running window", await until(`!document.getElementById('detail').hidden && document.getElementById('d-title').value === 'Water the plants'`, 15000));
  check("the second start exits instead of opening a second window", exited);

  // A reminder that is already due is shown by the window's own timer, which
  // tells the window it fired -- and firing it is not an edit: the task keeps
  // its version, so a detail panel open on it still saves.
  await js(`(window.__fired = false, window.__TAURI__.event.listen('changed', () => { window.__fired = true; }))`);
  const due = await js(`(async () => window.__TAURI__.core.invoke('create', { fields: { title: 'Ring the bank', remind_at: Date.now() - 1000 } }))()`);
  check("a due reminder is shown by the window's own timer", await until(`window.__fired === true`, 30000));
  check("showing it leaves the task's version alone", await js(`(async () => (await window.__TAURI__.core.invoke('get', { id: ${JSON.stringify(due.id)} })).rev === ${due.rev})()`));

  // iCalendar out and back in, through the forms a person uses.
  const ics = join(home, "out.ics");
  await js(`(() => { document.querySelector('details').open = true; document.getElementById('export-path').value = ${JSON.stringify(ics)}; document.getElementById('export-form').requestSubmit(); })()`);
  check("Export iCalendar writes the file", await until(`document.getElementById('announcer').textContent.startsWith('Saved ')`));
  await js(`(() => { document.getElementById('import-path').value = ${JSON.stringify(ics)}; document.getElementById('import-form').requestSubmit(); })()`);
  check("importing it back says how many", await until(`/^Imported [1-9]/.test(document.getElementById('announcer').textContent)`));

  await js(`document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`);
  await js(`document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', ctrlKey: true, bubbles: true, cancelable: true }))`);
  check("Ctrl+N goes to the new-task field", await until(`document.activeElement.id === 'add-title'`));

  setPref("locale", "tr");
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
  cdp?.close();
  try { app?.kill(); } catch { /* gone */ }
  await sleep(1500);
  try { rmSync(home, { recursive: true, force: true }); } catch { /* in use */ }
}
console.log(failures ? `\n${failures} check(s) failed.` : "\nall checks passed");
process.exit(failures ? 1 : 0);
