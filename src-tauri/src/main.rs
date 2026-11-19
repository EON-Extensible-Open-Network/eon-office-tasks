// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! EON Tasks -- the window.
//!
//! ```text
//! eon-tasks                    open the window
//! eon-tasks eon-office://...   open the window on an item (a running window takes it)
//! eon-tasks --selftest [FILE]  strings and interface checks
//! eon-tasks --version
//! ```
//!
//! EON Tasks stands alone: it opens its own store in the EON Office home
//! folder through the `eon-office-core` library. Nothing else has to be
//! installed or running.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eon_office_core::Home;
use eon_tasks::checks;
use eon_tasks::tasks::{Outcome, Tasks};
use serde_json::{json, Value};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tauri::{Emitter, Manager};

const INDEX: &str = include_str!("../../src/index.html");
const SCRIPT: &str = include_str!("../../src/app.js");

#[derive(Default)]
struct Shared {
    tasks: Mutex<Option<Arc<Tasks>>>,
    /// The link EON Tasks was started with, until the window has shown it.
    pending: Mutex<Option<String>>,
}

impl Shared {
    fn tasks(&self) -> Result<Arc<Tasks>, Value> {
        self.tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| json!({ "code": "not_open", "message": "not open yet" }))
    }
}

type State<'a> = tauri::State<'a, Shared>;

/// Open the store in the EON Office home folder and register EON Tasks there.
fn open(app: &tauri::AppHandle, shared: &Shared) -> Outcome {
    if shared.tasks().is_ok() {
        return Ok(json!({ "open": true }));
    }
    let root = Home::default_location()
        .ok_or_else(|| json!({ "code": "no_home", "message": "no per-user data folder" }))?;
    let home = eon_tasks::tasks::core(Home::open(&root))?;
    let folder = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf));
    let tasks = Arc::new(Tasks::open(&home, folder.as_deref())?);
    *shared.tasks.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&tasks));

    // Reminders: checked every 20 seconds while the window is open, and at
    // once on start, so one that fell due while EON Tasks was closed shows now.
    let reminders = Arc::clone(&tasks);
    let handle = app.clone();
    std::thread::spawn(move || loop {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0));
        let due = reminders.due_reminders(now);
        if !due.is_empty() {
            if reminders.may_notify() {
                use tauri_plugin_notification::NotificationExt;
                for item in &due {
                    let _ = handle
                        .notification()
                        .builder()
                        .title(item["title"].as_str().unwrap_or_default())
                        .body(item["body"].as_str().unwrap_or_default())
                        .show();
                }
            }
            let _ = handle.emit("changed", ());
        }
        std::thread::sleep(Duration::from_secs(20));
    });

    // Preferences are shared with every EON Office application; notice a
    // change made elsewhere (the launcher) within two seconds.
    let handle = app.clone();
    std::thread::spawn(move || {
        let mut seen = tasks.prefs_version();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let now = tasks.prefs_version();
            if now != seen {
                seen = now;
                let _ = handle.emit("prefs-changed", ());
            }
        }
    });
    Ok(json!({ "open": true }))
}

/// Show what a link asks for, bringing the window forward.
fn deliver(app: &tauri::AppHandle, link: &str) {
    let state = app.state::<Shared>();
    let Ok(tasks) = state.tasks() else {
        // Not open yet: the window asks for it once it is.
        *state.pending.lock().unwrap_or_else(PoisonError::into_inner) = Some(link.to_owned());
        return;
    };
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    if let Some(show) = tasks.link(link) {
        let _ = app.emit("show", show);
    }
}

fn link_argument(args: &[String]) -> Option<String> {
    args.iter()
        .skip(1)
        .find(|a| a.starts_with(eon_office_core::links::SCHEME))
        .cloned()
}

#[tauri::command(async)]
fn strings(language: String) -> Value {
    eon_tasks::strings(&language)
}

#[tauri::command(async)]
fn status(app: tauri::AppHandle, state: State<'_>) -> Outcome {
    open(&app, &state)
}

/// The link EON Tasks was started with, once: what the window shows first.
#[tauri::command(async)]
fn initial(state: State<'_>) -> Outcome {
    let tasks = state.tasks()?;
    let link = state
        .pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    Ok(link.and_then(|l| tasks.link(&l)).unwrap_or(Value::Null))
}

#[tauri::command(async)]
fn prefs(state: State<'_>) -> Outcome {
    state.tasks()?.prefs()
}

#[tauri::command(async)]
fn view(state: State<'_>, view: String, today: String) -> Outcome {
    state.tasks()?.view(&view, &today)
}

#[tauri::command(async)]
fn get(state: State<'_>, id: String) -> Outcome {
    state.tasks()?.get(&id)
}

#[tauri::command(async)]
fn create(state: State<'_>, fields: Value) -> Outcome {
    state.tasks()?.create(&fields)
}

#[tauri::command(async)]
fn update(state: State<'_>, id: String, changes: Value, rev: u64) -> Outcome {
    state.tasks()?.update(&id, &changes, rev)
}

#[tauri::command(async)]
fn complete(state: State<'_>, id: String, rev: u64, today: String) -> Outcome {
    state.tasks()?.complete(&id, rev, &today)
}

#[tauri::command(async)]
fn reopen(state: State<'_>, id: String, rev: u64) -> Outcome {
    state.tasks()?.reopen(&id, rev)
}

#[tauri::command(async)]
fn delete(state: State<'_>, id: String) -> Outcome {
    state.tasks()?.delete(&id)
}

#[tauri::command(async)]
fn export_ics(state: State<'_>, path: String, list: Option<String>) -> Outcome {
    let out = state.tasks()?.export_ics(list.as_deref())?;
    let path = path.trim().trim_matches('"');
    std::fs::write(path, out["ics"].as_str().unwrap_or_default())
        .map_err(|e| json!({ "code": "io", "message": e.to_string() }))?;
    Ok(json!({ "path": path, "count": out["count"] }))
}

#[tauri::command(async)]
fn import_ics(state: State<'_>, path: String) -> Outcome {
    let text = std::fs::read_to_string(path.trim().trim_matches('"'))
        .map_err(|e| json!({ "code": "unreadable", "message": e.to_string() }))?;
    state.tasks()?.import_ics(&text)
}

fn main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    #[allow(unused_mut)]
    let mut builder =
        tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
            .title("EON Tasks")
            .inner_size(1100.0, 760.0)
            .min_inner_size(640.0, 480.0);
    // Debug builds only: a DevTools port for tools/ui-check.mjs.
    #[cfg(debug_assertions)]
    if let Ok(extra) = std::env::var("EON_WEBVIEW_ARGS") {
        builder = builder.additional_browser_args(&format!(
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection {extra}"
        ));
    }
    builder.build().map(|_| ())
}

fn window(link: Option<String>) {
    let shared = Shared::default();
    *shared
        .pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = link;
    let app = tauri::Builder::default()
        // First, so a second start hands over its link and exits before
        // anything else happens.
        .plugin(tauri_plugin_single_instance::init(
            |app, args, _cwd| match link_argument(&args) {
                Some(link) => deliver(app, &link),
                None => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                }
            },
        ))
        .plugin(tauri_plugin_notification::init())
        .manage(shared)
        .setup(|app| {
            main_window(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            strings, status, initial, prefs, view, get, create, update, complete, reopen, delete,
            export_ics, import_ics,
        ])
        .build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|_, _| {}),
        Err(e) => {
            eprintln!("EON Tasks could not start: {e}");
            std::process::exit(1);
        }
    }
}

fn selftest() -> String {
    let mut report = format!("EON Tasks {} self-test\n", env!("CARGO_PKG_VERSION"));
    let mut failed = false;
    let mut section = |name: &str, problems: Vec<String>| {
        if problems.is_empty() {
            let _ = writeln!(report, "  ok    {name}");
        } else {
            failed = true;
            for problem in problems {
                let _ = writeln!(report, "  FAIL  {name}: {problem}");
            }
        }
    };
    section(
        "strings (en, tr)",
        checks::catalogue_problems(eon_tasks::EN, eon_tasks::TR),
    );
    section("interface", checks::ui_problems(INDEX, SCRIPT));
    let manifest: Value = serde_json::from_str(eon_tasks::MANIFEST).unwrap_or(Value::Null);
    let mut problems = Vec::new();
    if manifest["id"] != eon_tasks::APP_ID || manifest["version"] != env!("CARGO_PKG_VERSION") {
        problems.push("manifest.json's id or version does not match this build".to_owned());
    }
    section("manifest", problems);
    report.push_str(if failed { "FAILED\n" } else { "ok\n" });
    report
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        Some("--version" | "-V") => println!("EON Tasks {}", env!("CARGO_PKG_VERSION")),
        // Debug builds only: change a shared preference, as the launcher would,
        // so tools/ui-check.mjs can check that a running window follows it.
        #[cfg(debug_assertions)]
        Some("--set-pref") => {
            let (key, value) = (
                arguments.next().unwrap_or_default(),
                arguments.next().unwrap_or_default(),
            );
            let value: Value = serde_json::from_str(&value).unwrap_or(Value::String(value));
            let done = Home::default_location()
                .ok_or_else(|| "no home folder".to_owned())
                .and_then(|root| Home::open(&root).map_err(|e| e.to_string()))
                .and_then(|home| home.shared().map_err(|e| e.to_string()))
                .and_then(|shared| shared.set_pref(&key, &value).map_err(|e| e.to_string()));
            if let Err(e) = done {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        Some("--selftest") => {
            let report = selftest();
            match arguments.next() {
                Some(path) => {
                    if let Err(e) = std::fs::write(&path, &report) {
                        eprintln!("could not write {path}: {e}");
                        std::process::exit(2);
                    }
                }
                None => print!("{report}"),
            }
            if !report.ends_with("ok\n") {
                std::process::exit(1);
            }
        }
        _ => window(link_argument(&std::env::args().collect::<Vec<_>>())),
    }
}
