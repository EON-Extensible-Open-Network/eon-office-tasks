// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! EON Tasks against a real core: the roadmap's acceptance criteria that a
//! machine can check. Needs `EON_OFFICE_CORE_BIN`.

#![allow(
    clippy::too_many_lines,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::format_push_string
)]

use eon_office_sdk::{Client, Incoming};
use eon_tasks::tasks::Tasks;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Core {
    child: Child,
    home: PathBuf,
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn start(name: &str) -> (Core, Client, Tasks) {
    let binary = std::env::var_os("EON_OFFICE_CORE_BIN").expect("set EON_OFFICE_CORE_BIN");
    let home = std::env::temp_dir().join(format!("eon-tasks-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let child = Command::new(binary)
        .args(["serve", "--home"])
        .arg(&home)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while eon_office_sdk::endpoint(&home).is_err() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(25));
    }
    let owner = Client::connect_owner(&home).unwrap();
    let manifest: Value = serde_json::from_str(eon_tasks::MANIFEST).unwrap();
    owner
        .call(
            "apps.install",
            json!({ "manifest": manifest, "grant": manifest["capabilities"] }),
        )
        .unwrap();
    let token =
        std::fs::read_to_string(home.join("apps").join(eon_tasks::APP_ID).join("token")).unwrap();
    let client = Client::connect(
        &eon_office_sdk::endpoint(&home).unwrap(),
        Some(eon_tasks::APP_ID),
        token.trim(),
    )
    .unwrap();
    (Core { child, home }, owner, Tasks::new(client))
}

#[test]
#[ignore = "needs EON_OFFICE_CORE_BIN"]
fn tasks_views_repeats_board_and_conflicts() {
    let (_core, _owner, tasks) = start("basic");
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
#[ignore = "needs EON_OFFICE_CORE_BIN"]
fn a_reminder_becomes_a_notification_and_ics_round_trips() {
    let (_core, owner, tasks) = start("remind");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let t = tasks
        .create(&json!({ "title": "Call Ayşe", "due": "2026-11-19", "remind_at": now + 400 }))
        .unwrap();
    // The core's scheduler wakes Tasks; Tasks posts the notification.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut reminded = false;
    while Instant::now() < deadline && !reminded {
        if let Some(Incoming::JobDue { name, payload, .. }) =
            tasks.client.next(Some(Duration::from_millis(200)))
        {
            assert_eq!(name, format!("remind-{}", t["id"].as_str().unwrap()));
            tasks.remind(&payload);
            reminded = true;
        }
    }
    assert!(reminded, "no job.due for the reminder");
    let listed = owner.call("notifications.list", json!({})).unwrap();
    assert_eq!(listed["items"][0]["title"], "Call Ayşe");

    // iCalendar out and back in: the same tasks, subtasks attached.
    let parent = tasks.create(&json!({ "title": "Trip; plan, book", "due": "2026-12-01", "rrule": "FREQ=WEEKLY;BYDAY=MO", "notes": "two\nlines" })).unwrap();
    tasks
        .create(&json!({ "title": "Passport", "parent": parent["id"] }))
        .unwrap();
    let ics = tasks.export_ics(None).unwrap();
    assert_eq!(ics["count"], 3);
    let report = tasks.import_ics(ics["ics"].as_str().unwrap()).unwrap();
    assert_eq!(report["imported"], 3);
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
#[ignore = "needs EON_OFFICE_CORE_BIN"]
fn another_application_creates_a_task_through_the_core() {
    let (_core, owner, tasks) = start("call");
    let answering = std::thread::spawn(move || {
        while let Some(message) = tasks.client.next(Some(Duration::from_secs(10))) {
            if let Incoming::Invoke {
                id, method, params, ..
            } = message
            {
                return (tasks.answer(&id, &method, &params), tasks);
            }
        }
        panic!("no invoke");
    });
    let created = owner
        .call_app(
            eon_tasks::APP_ID,
            "task.create",
            json!({ "title": "From Notes", "due": "2026-11-20" }),
        )
        .unwrap();
    let (show, tasks) = answering.join().unwrap();
    assert_eq!(show.unwrap()["open"], created["id"]);
    assert_eq!(
        tasks.get(created["id"].as_str().unwrap()).unwrap()["due"],
        "2026-11-20"
    );
}
