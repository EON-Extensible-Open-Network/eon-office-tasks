// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! EON Tasks -- the window.
//!
//! ```text
//! eon-tasks                    open the window
//! eon-tasks --selftest [FILE]  strings and interface checks
//! eon-tasks --version
//! ```

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eon_office_sdk::{Client, Incoming};
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
}

impl Shared {
    fn tasks(&self) -> Result<Arc<Tasks>, Value> {
        self.tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| json!({ "code": "not_connected", "message": "not connected" }))
    }
}

type State<'a> = tauri::State<'a, Shared>;

/// Connect to the core as EON Tasks and start answering other applications.
fn connect(app: &tauri::AppHandle, shared: &Shared) -> Outcome {
    if let Ok(tasks) = shared.tasks() {
        return Ok(json!({ "granted": tasks.client.granted() }));
    }
    let client = Client::connect_app(eon_tasks::APP_ID).map_err(|e| {
        let code = if e.code() == "unauthenticated" {
            "not_installed"
        } else if e.code() == "connection" {
            "no_core"
        } else {
            e.code()
        };
        json!({ "code": code, "message": e.to_string() })
    })?;
    let granted = client.granted();
    let tasks = Arc::new(Tasks::new(client));
    *shared.tasks.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&tasks));
    let handle = app.clone();
    std::thread::spawn(move || {
        while let Some(message) = tasks.client.next(None) {
            match message {
                Incoming::Invoke {
                    id, method, params, ..
                } => {
                    if let Some(show) = tasks.answer(&id, &method, &params) {
                        if let Some(window) = handle.get_webview_window("main") {
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                        let _ = handle.emit("show", show);
                    }
                }
                Incoming::JobDue { name, payload, .. } if name.starts_with("remind-") => {
                    tasks.remind(&payload);
                    let _ = handle.emit("changed", ());
                }
                Incoming::Event { topic, .. } if topic == "core/prefs.changed" => {
                    let _ = handle.emit("prefs-changed", ());
                }
                Incoming::Shutdown => {
                    // The core asks Tasks to leave: the interface saves first.
                    let _ = handle.emit("save-and-quit", ());
                    std::thread::sleep(Duration::from_millis(1500));
                    handle.exit(0);
                    break;
                }
                _ => {}
            }
        }
    });
    Ok(json!({ "granted": granted }))
}

#[tauri::command(async)]
fn strings(language: String) -> Value {
    eon_tasks::strings(&language)
}

#[tauri::command(async)]
fn status(app: tauri::AppHandle, state: State<'_>) -> Outcome {
    connect(&app, &state)
}

#[tauri::command(async)]
fn prefs(state: State<'_>) -> Outcome {
    state
        .tasks()?
        .client
        .prefs()
        .map_err(|e| json!({ "code": e.code(), "message": e.to_string() }))
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

fn window() {
    let app = tauri::Builder::default()
        .manage(Shared::default())
        .setup(|app| {
            main_window(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            strings, status, prefs, view, get, create, update, complete, reopen, delete,
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
        _ => window(),
    }
}
