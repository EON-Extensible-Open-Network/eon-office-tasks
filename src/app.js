// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// EON Tasks' interface. No network of its own; text from tasks (which may be
// imported from anywhere) enters the page only through textContent.

"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (id) => document.getElementById(id);

let strings = {};
let view = "today";
let mode = "list";
let tasks = [];
let detail = null; // the task open in the side panel

function t(key, vars = {}) {
  let text = strings[key] ?? key;
  for (const [n, v] of Object.entries(vars)) text = text.replaceAll(`{${n}}`, String(v));
  return text;
}
const reason = (e) => (e && (e.message || e.code)) || String(e);

function today() {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

async function loadStrings(locale) {
  const language = locale && locale !== "system"
    ? (locale.startsWith("tr") ? "tr" : "en")
    : ((navigator.language || "en").toLowerCase().startsWith("tr") ? "tr" : "en");
  strings = await invoke("strings", { language });
  document.documentElement.lang = language;
  for (const n of document.querySelectorAll("[data-i18n]")) n.textContent = t(n.dataset.i18n);
  for (const n of document.querySelectorAll("[data-i18n-label]")) n.setAttribute("aria-label", t(n.dataset.i18nLabel));
  for (const n of document.querySelectorAll("[data-i18n-placeholder]")) n.setAttribute("placeholder", t(n.dataset.i18nPlaceholder));
}

async function applyPrefs() {
  const p = await invoke("prefs");
  const root = document.documentElement;
  if (p.theme === "light" || p.theme === "dark") root.dataset.theme = p.theme; else delete root.dataset.theme;
  if (p.high_contrast) root.dataset.contrast = "high"; else delete root.dataset.contrast;
  if (p.reduce_motion) root.dataset.motion = "reduce"; else delete root.dataset.motion;
  root.style.setProperty("--scale", String(p.font_scale || 1));
  await loadStrings(p.locale);
}

function announce(text) {
  $("announcer").textContent = "";
  window.setTimeout(() => { $("announcer").textContent = text; }, 30);
}

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else if (k.startsWith("on")) node.addEventListener(k.slice(2), v);
    else node.setAttribute(k, v);
  }
  for (const c of children) if (c !== null && c !== undefined && c !== false) node.append(c);
  return node;
}

function dueText(task) {
  if (!task.due) return "";
  const now = today();
  if (task.status !== "done" && task.due < now) return `${t("due.overdue")} · ${task.due}`;
  if (task.due === now) return t("due.today");
  return new Date(`${task.due}T00:00`).toLocaleDateString(document.documentElement.lang, { weekday: "short", day: "numeric", month: "short" });
}

function repeatText(rule) {
  const option = [...$("d-repeat").options].find((o) => o.value === rule);
  return option ? option.textContent : rule;
}

// ---- views ----------------------------------------------------------------

function viewTitle() {
  if (view.startsWith("list:")) return view.slice(5);
  return t(`view.${view}`);
}

async function refresh() {
  try {
    const r = await invoke("view", { view, today: today() });
    tasks = r.tasks;
    $("view-title").textContent = viewTitle();
    for (const b of document.querySelectorAll("#views button")) b.setAttribute("aria-current", b.dataset.view === view ? "page" : "false");
    $("lists").replaceChildren(...Object.entries(r.lists).map(([name, open]) => el("li", {},
      el("button", { "data-view": `list:${name}`, "aria-current": view === `list:${name}` ? "page" : "false", onclick: () => go(`list:${name}`) },
        el("span", { text: name }), el("span", { class: "muted", text: String(open) })))));
    $("list-names").replaceChildren(...Object.keys(r.lists).map((n) => el("option", { value: n })));
    const top = tasks.filter((x) => !x.parent);
    $("status").textContent = top.length ? t("count", { count: top.length }) : t("empty");
    if (mode === "list") renderList(top); else renderBoard(top);
    if (detail) renderSubtasks();
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
  }
}

function go(next) {
  view = next;
  if (next.startsWith("list:")) $("add-list").value = next.slice(5);
  refresh();
}
for (const b of document.querySelectorAll("#views button")) b.addEventListener("click", () => go(b.dataset.view));

function checkbox(task) {
  const box = el("input", {
    type: "checkbox",
    "aria-label": task.status === "done" ? t("done.undo", { title: task.title }) : t("done.mark", { title: task.title }),
  });
  box.checked = task.status === "done";
  box.addEventListener("change", () => toggle(task, box.checked));
  return box;
}

function row(task) {
  const overdue = task.status !== "done" && task.due && task.due < today();
  const meta = [dueText(task), task.rrule ? repeatText(task.rrule) : "", view.startsWith("list:") ? "" : task.list].filter(Boolean).join(" · ");
  return el("li", { class: task.status === "done" ? "done" : "", "data-id": task.id },
    checkbox(task),
    el("button", { class: "title", type: "button", text: task.title, "aria-label": t("open.detail", { title: task.title }), onclick: () => openDetail(task.id) }),
    meta ? el("span", { class: `meta${overdue ? " overdue" : ""}`, text: meta }) : null);
}

function renderList(top) {
  $("board").hidden = true;
  $("task-list").hidden = false;
  $("task-list").replaceChildren(...top.map(row));
}

const COLUMNS = ["todo", "doing", "done"];
function renderBoard(top) {
  $("task-list").hidden = true;
  $("board").hidden = false;
  $("board").replaceChildren(...COLUMNS.map((column, i) => el("section", { "aria-labelledby": `col-${column}` },
    el("h2", { id: `col-${column}`, text: t(`col.${column}`) }),
    el("ul", {}, ...top.filter((x) => (x.column || "todo") === column).map((task) => el("li", { "data-id": task.id },
      el("button", { class: "title", type: "button", text: task.title, onclick: () => openDetail(task.id) }),
      el("div", { class: "moves" },
        i > 0 ? el("button", { class: "secondary", type: "button", text: "←", "aria-label": t("move.left", { title: task.title, column: t(`col.${COLUMNS[i - 1]}`) }), onclick: () => move(task, COLUMNS[i - 1]) }) : null,
        i < 2 ? el("button", { class: "secondary", type: "button", text: "→", "aria-label": t("move.right", { title: task.title, column: t(`col.${COLUMNS[i + 1]}`) }), onclick: () => move(task, COLUMNS[i + 1]) }) : null,
      )))))));
}

function setMode(next) {
  mode = next;
  $("mode-list").setAttribute("aria-selected", String(next === "list"));
  $("mode-board").setAttribute("aria-selected", String(next === "board"));
  $("mode-list").tabIndex = next === "list" ? 0 : -1;
  $("mode-board").tabIndex = next === "board" ? 0 : -1;
  refresh();
}
$("mode-list").addEventListener("click", () => setMode("list"));
$("mode-board").addEventListener("click", () => setMode("board"));
for (const tab of [$("mode-list"), $("mode-board")]) {
  tab.addEventListener("keydown", (e) => {
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      e.preventDefault();
      const next = mode === "list" ? "board" : "list";
      setMode(next);
      $(`mode-${next}`).focus();
    }
  });
}

// ---- actions ---------------------------------------------------------------

async function toggle(task, done) {
  try {
    if (done) {
      const r = await invoke("complete", { id: task.id, rev: task.rev, today: today() });
      announce(r.next ? t("repeated", { date: r.next }) : t("completed"));
    } else {
      await invoke("reopen", { id: task.id, rev: task.rev });
    }
  } catch (e) {
    announce(e && e.code === "conflict" ? t("conflict") : t("error", { reason: reason(e) }));
  }
  await refresh();
}

async function move(task, column) {
  try {
    await invoke("update", { id: task.id, changes: { column }, rev: task.rev });
  } catch (e) {
    announce(e && e.code === "conflict" ? t("conflict") : t("error", { reason: reason(e) }));
  }
  await refresh();
  document.querySelector(`#board li[data-id="${task.id}"] .title`)?.focus();
}

$("add-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const title = $("add-title").value.trim();
  if (!title) return;
  const fields = { title, list: $("add-list").value.trim() };
  if (view === "today") fields.due = today();
  try {
    await invoke("create", { fields });
    $("add-title").value = "";
    await refresh();
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
  }
});

// ---- the detail panel --------------------------------------------------------

async function openDetail(id) {
  try {
    detail = await invoke("get", { id });
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
    return;
  }
  $("layout").classList.add("with-detail");
  $("detail").hidden = false;
  $("d-confirm").hidden = true;
  $("d-title").value = detail.title;
  $("d-due").value = detail.due || "";
  $("d-repeat").value = detail.rrule || "";
  $("d-list").value = detail.list || "";
  $("d-notes").value = detail.notes || "";
  $("d-remind").value = detail.remind_at ? new Date(detail.remind_at).toTimeString().slice(0, 5) : "";
  renderSubtasks();
  $("d-title").focus();
}

function closeDetail() {
  const id = detail?.id;
  detail = null;
  $("detail").hidden = true;
  $("layout").classList.remove("with-detail");
  document.querySelector(`[data-id="${id}"] .title`)?.focus();
}
$("d-close").addEventListener("click", closeDetail);
document.addEventListener("keydown", (e) => { if (e.key === "Escape" && detail) closeDetail(); });

function renderSubtasks() {
  if (!detail) return;
  const subs = tasks.filter((x) => x.parent === detail.id);
  $("subtasks").replaceChildren(...subs.map((s) => el("li", { class: s.status === "done" ? "done" : "", "data-id": s.id },
    checkbox(s), el("span", { class: "title", text: s.title }))));
}

$("detail-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const due = $("d-due").value;
  const time = $("d-remind").value;
  const changes = {
    title: $("d-title").value.trim(), notes: $("d-notes").value, list: $("d-list").value.trim(),
    due, rrule: $("d-repeat").value,
    remind_at: due && time ? new Date(`${due}T${time}`).getTime() : null,
  };
  try {
    const r = await invoke("update", { id: detail.id, changes, rev: detail.rev });
    detail.rev = r.rev;
    announce(t("saved"));
    await refresh();
  } catch (e) {
    announce(e && e.code === "conflict" ? t("conflict") : t("error", { reason: reason(e) }));
  }
});

$("d-delete").addEventListener("click", () => {
  $("d-question").textContent = t("delete.confirm", { title: detail.title });
  $("d-confirm").hidden = false;
  $("d-no").focus();
});
$("d-no").addEventListener("click", () => { $("d-confirm").hidden = true; $("d-delete").focus(); });
$("d-yes").addEventListener("click", async () => {
  try {
    await invoke("delete", { id: detail.id });
    closeDetail();
    await refresh();
    $("add-title").focus();
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
  }
});

$("sub-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const title = $("sub-title").value.trim();
  if (!title || !detail) return;
  try {
    await invoke("create", { fields: { title, parent: detail.id, list: detail.list || "" } });
    $("sub-title").value = "";
    await refresh();
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
  }
});

// ---- iCalendar ----------------------------------------------------------------

$("export-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const r = await invoke("exportIcs", { path: $("export-path").value, list: view.startsWith("list:") ? view.slice(5) : null });
    announce(t("exported", { count: r.count, path: r.path }));
    $("status").textContent = t("exported", { count: r.count, path: r.path });
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
  }
});
$("import-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const r = await invoke("importIcs", { path: $("import-path").value });
    announce(t("import.done", { count: r.imported }));
    $("status").textContent = t("import.done", { count: r.imported });
    await refresh();
  } catch (e) {
    announce(t("error", { reason: reason(e) }));
  }
});

// ---- the core -------------------------------------------------------------------

listen("show", async ({ payload }) => {
  await refresh();
  if (payload.open) openDetail(payload.open);
});
listen("changed", refresh);
listen("prefs-changed", async () => { await applyPrefs(); await refresh(); });
document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") { e.preventDefault(); $("add-title").focus(); }
});

async function start() {
  await loadStrings("system");
  try {
    await invoke("status");
    await applyPrefs();
    $("banner").hidden = true;
    $("layout").hidden = false;
    await refresh();
    if (document.activeElement === document.body || document.activeElement === null) $("add-title").focus();
    document.body.dataset.ready = "1";
  } catch (e) {
    $("layout").hidden = true;
    $("banner").hidden = false;
    $("banner-text").textContent = e && e.code === "not_installed" ? t("not.installed") : e && e.code === "no_core" ? t("no.core") : t("error", { reason: reason(e) });
  }
}
$("retry").addEventListener("click", start);
start();
