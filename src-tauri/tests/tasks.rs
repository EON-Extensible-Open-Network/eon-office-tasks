// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! EON Tasks on the EON Office library, in a home folder of its own: the
//! roadmap's acceptance criteria that a machine can check. Nothing else has
//! to be installed or running.

#![allow(
    clippy::too_many_lines,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::format_push_string
)]

use eon_office_core::Home;
use eon_tasks::tasks::Tasks;
use serde_json::{json, Value};
use std::path::PathBuf;

struct TempHome(PathBuf);

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn start(name: &str) -> (TempHome, Home, Tasks) {
    let dir = std::env::temp_dir().join(format!("eon-tasks-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let home = Home::open(&dir).unwrap();
    let tasks = Tasks::open(&home, None).unwrap();
    (TempHome(dir), home, tasks)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[test]
fn tasks_views_repeats_board_and_conflicts() {
    let (_dir, _home, tasks) = start("basic");
    let today = "2026-11-19";
    let a = tasks.create(&json!({ "title": "Pay rent", "due": "2026-11-19", "list": "Home", "rrule": "FREQ=MONTHLY" })).unwrap();
    let b = tasks
        .create(&json!({ "title": "Dentist", "due": "2026-11-23", "list": "Health" }))
        .unwrap();
    let c = tasks
        .create(&json!({ "title": "Someday", "list": "Home" }))
        .unwrap();
    let sub = tasks
        .create(&json!({ "title": "Find the bill", "parent": a["id"], "list": "Home" }))
        .unwrap();
    assert!(
        tasks
            .create(&json!({ "title": "Bad", "rrule": "FREQ=DAILY" }))
            .is_err(),
        "a repeat needs a due date"
    );
    assert!(tasks
        .create(&json!({ "title": "Bad", "due": "2026-02-30" }))
        .is_err());

    let ids = |v: &Value| {
        v["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    // Top-level tasks only: the view also carries each task's subtasks.
    let top = |v: &Value| {
        v["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["parent"].as_str().is_none_or(str::is_empty))
            .map(|t| t["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        top(&tasks.view("today", today).unwrap()),
        vec![a["id"].as_str().unwrap()]
    );
    assert_eq!(
        ids(&tasks.view("upcoming", today).unwrap()),
        vec![b["id"].as_str().unwrap()]
    );
    let home = tasks.view("list:Home", today).unwrap();
    assert_eq!(home["tasks"].as_array().unwrap().len(), 3);
    // Today shows "Pay rent" and brings its undated subtask with it.
    let today_view = tasks.view("today", today).unwrap();
    assert!(today_view["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["id"] == sub["id"]));
    assert_eq!(home["lists"]["Home"], 3);
    let _ = c;

    // Completing a monthly task moves it to next month and keeps it open.
    let rev = tasks.get(a["id"].as_str().unwrap()).unwrap()["rev"]
        .as_u64()
        .unwrap();
    let done = tasks
        .complete(a["id"].as_str().unwrap(), rev, today)
        .unwrap();
    assert_eq!(done["next"], "2026-12-19");
    let moved = tasks.get(a["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        (moved["status"].clone(), moved["done_count"].clone()),
        (json!("open"), json!(1))
    );

    // A one-off task closes.
    let rev = tasks.get(b["id"].as_str().unwrap()).unwrap()["rev"]
        .as_u64()
        .unwrap();
    assert_eq!(
        tasks
            .complete(b["id"].as_str().unwrap(), rev, today)
            .unwrap()["next"],
        Value::Null
    );
    assert_eq!(
        ids(&tasks.view("done", today).unwrap()),
        vec![b["id"].as_str().unwrap()]
    );

    // The board, and a stale write refused.
    let s = tasks.get(sub["id"].as_str().unwrap()).unwrap();
    let rev = s["rev"].as_u64().unwrap();
    tasks
        .update(
            sub["id"].as_str().unwrap(),
            &json!({ "column": "doing" }),
            rev,
        )
        .unwrap();
    assert_eq!(
        tasks
            .update(
                sub["id"].as_str().unwrap(),
                &json!({ "column": "done" }),
                rev
            )
            .unwrap_err()["code"],
        "conflict"
    );

    // Deleting a task deletes its subtasks.
    let gone = tasks.delete(a["id"].as_str().unwrap()).unwrap();
    assert_eq!(gone["deleted"].as_array().unwrap().len(), 2);
}

#[test]
fn reminders_fall_due_once_and_missed_ones_fire_once_at_start() {
    let (dir, home, tasks) = start("remind");
    let now = now_ms();
    let due = tasks
        .create(&json!({ "title": "Call Ayşe", "due": "2026-11-19", "remind_at": now - 60_000 }))
        .unwrap();
    tasks
        .create(&json!({ "title": "Later", "remind_at": now + 3_600_000 }))
        .unwrap();
    let finished = tasks
        .create(&json!({ "title": "Already done", "remind_at": now - 60_000 }))
        .unwrap();
    let rev = tasks.get(finished["id"].as_str().unwrap()).unwrap()["rev"]
        .as_u64()
        .unwrap();
    tasks
        .complete(finished["id"].as_str().unwrap(), rev, "2026-11-19")
        .unwrap();

    // The detail panel is open on the task, holding its revision.
    let held = tasks.get(due["id"].as_str().unwrap()).unwrap()["rev"]
        .as_u64()
        .unwrap();

    // The window's timer asks what fell due: one notification, the open task's.
    let fired = tasks.due_reminders(now);
    assert_eq!(fired.len(), 1, "{fired:?}");
    assert_eq!(
        (fired[0]["id"].clone(), fired[0]["title"].clone()),
        (due["id"].clone(), json!("Call Ayşe"))
    );
    assert_eq!(fired[0]["body"], "Due 2026-11-19");
    // Firing is not an edit: the open panel's save still goes through.
    tasks
        .update(
            due["id"].as_str().unwrap(),
            &json!({ "notes": "typed while the reminder fired" }),
            held,
        )
        .unwrap();
    // Asked again, nothing: a reminder fires once.
    assert!(tasks.due_reminders(now + 1000).is_empty());
    // Closed and started again (a missed reminder is caught up at start): still once.
    drop(tasks);
    let again = Tasks::open(&home, None).unwrap();
    assert!(again.due_reminders(now + 2000).is_empty());
    // Moving the reminder makes it fire again, at its new time.
    let t = again.get(due["id"].as_str().unwrap()).unwrap();
    again
        .update(
            due["id"].as_str().unwrap(),
            &json!({ "remind_at": now + 10_000 }),
            t["rev"].as_u64().unwrap(),
        )
        .unwrap();
    assert!(again.due_reminders(now + 5_000).is_empty());
    assert_eq!(again.due_reminders(now + 10_000).len(), 1);
    // Revoking notifications stops them.
    home.shared()
        .unwrap()
        .revoke(eon_tasks::APP_ID, "notifications")
        .unwrap();
    assert!(!again.may_notify());
    drop(dir);
}

#[test]
fn ics_round_trips() {
    let (_dir, _home, tasks) = start("ics");
    // iCalendar out and back in: the same tasks, subtasks attached.
    let parent = tasks.create(&json!({ "title": "Trip; plan, book", "due": "2026-12-01", "rrule": "FREQ=WEEKLY;BYDAY=MO", "notes": "two\nlines" })).unwrap();
    tasks
        .create(&json!({ "title": "Passport", "parent": parent["id"] }))
        .unwrap();
    let ics = tasks.export_ics(None).unwrap();
    assert_eq!(ics["count"], 2);
    let report = tasks.import_ics(ics["ics"].as_str().unwrap()).unwrap();
    assert_eq!(report["imported"], 2);
    let all = tasks.view("all", "2026-11-19").unwrap();
    let copies: Vec<&Value> = all["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["title"] == "Trip; plan, book")
        .collect();
    assert_eq!(copies.len(), 2);
    let copy = copies.iter().find(|t| t["id"] != parent["id"]).unwrap();
    assert_eq!(
        (copy["rrule"].clone(), copy["notes"].clone()),
        (json!("FREQ=WEEKLY;BYDAY=MO"), json!("two\nlines"))
    );
    let child = all["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["title"] == "Passport" && t["parent"] == copy["id"]);
    assert!(
        child.is_some(),
        "the imported subtask points at the imported parent"
    );
}

#[test]
fn a_link_argument_opens_the_task() {
    let (_dir, _home, tasks) = start("link");
    let t = tasks.create(&json!({ "title": "Linked" })).unwrap();
    let id = t["id"].as_str().unwrap();
    assert_eq!(
        tasks.link(&format!("eon-office://org.eon.office.tasks/{id}")),
        Some(json!({ "open": id }))
    );
    assert_eq!(
        tasks.link("eon-office://org.eon.office.tasks"),
        Some(json!({ "focus": true }))
    );
    assert_eq!(tasks.link("eon-office://org.eon.office.notes/x"), None);
    assert_eq!(tasks.link("eon-office://org.eon.office.tasks/nope"), None);
}
