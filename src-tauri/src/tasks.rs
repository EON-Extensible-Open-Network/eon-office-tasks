// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! Tasks, kept in the EON Office core.
//!
//! A task: `{ title, notes, list, parent, due, rrule, remind_at, status,
//! column, created, updated, done_count }`. `due` is a civil date; a
//! reminder is an absolute instant the interface computes in the person's
//! own time zone, delivered by the core's job scheduler as a notification --
//! so it arrives even if the window was closed when it was set, as soon as
//! Tasks runs. Completing a repeating task moves it to its next date instead
//! of closing it.

use crate::recur::{Date, Rule};
use eon_office_sdk::{Client, Error};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

pub const COLLECTION: &str = "tasks";
pub type Outcome = Result<Value, Value>;

fn fail(code: &str, message: impl Into<String>) -> Value {
    json!({ "code": code, "message": message.into() })
}

fn sdk<T>(result: Result<T, Error>) -> Result<T, Value> {
    result.map_err(|e| {
        let mut out = fail(e.code(), e.to_string());
        if let Error::Protocol { data, .. } = &e {
            out["data"] = data.clone();
        }
        out
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0))
}

static COUNTER: AtomicU64 = AtomicU64::new(0);
fn new_id() -> String {
    format!(
        "t{:x}{:x}",
        now_ms(),
        COUNTER.fetch_add(1, Ordering::Relaxed) % 4096
    )
}

fn check_fields(task: &Value) -> Result<(), Value> {
    let title = task["title"].as_str().unwrap_or_default().trim();
    if title.is_empty() || title.chars().count() > 300 {
        return Err(fail("invalid_params", "a title is 1-300 characters"));
    }
    let due = task["due"].as_str().unwrap_or_default();
    if !due.is_empty() && Date::parse(due).is_none() {
        return Err(fail("invalid_params", "a due date is YYYY-MM-DD"));
    }
    let rule = task["rrule"].as_str().unwrap_or_default();
    if !rule.is_empty() {
        Rule::parse(rule).map_err(|e| fail("invalid_params", e))?;
        if due.is_empty() {
            return Err(fail(
                "invalid_params",
                "a repeating task needs a due date to repeat from",
            ));
        }
    }
    if !matches!(
        task["column"].as_str().unwrap_or("todo"),
        "todo" | "doing" | "done"
    ) {
        return Err(fail(
            "invalid_params",
            "a board column is todo, doing or done",
        ));
    }
    Ok(())
}

pub struct Tasks {
    pub client: Client,
}

impl Tasks {
    pub fn new(client: Client) -> Tasks {
        Tasks { client }
    }

    fn all(&self) -> Result<Vec<Value>, Value> {
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let mut params = json!({ "collection": COLLECTION, "limit": 1000 });
            if let Some(a) = &after {
                params["after"] = json!(a);
            }
            let page = sdk(self.client.call("storage.list", params))?;
            let items = page["items"].as_array().cloned().unwrap_or_default();
            for item in &items {
                let mut task = item["doc"].clone();
                task["id"] = item["id"].clone();
                task["rev"] = item["rev"].clone();
                out.push(task);
            }
            if items.len() < 1000 {
                break;
            }
            after = items
                .last()
                .and_then(|i| i["id"].as_str())
                .map(str::to_owned);
        }
        Ok(out)
    }

    /// A view: `all`, `today`, `upcoming` (next 7 days), `done`, or
    /// `list:<name>`. `today` is the person's date, which only the interface
    /// knows. Open tasks first, by due date, undated last.
    pub fn view(&self, view: &str, today: &str) -> Outcome {
        let today =
            Date::parse(today).ok_or_else(|| fail("invalid_params", "today is YYYY-MM-DD"))?;
        let week = today.add_days(7).to_string();
        let today = today.to_string();
        let all = self.all()?;
        let mut lists: BTreeMap<String, usize> = BTreeMap::new();
        for t in &all {
            if let Some(l) = t["list"].as_str().filter(|l| !l.is_empty()) {
                *lists.entry(l.to_owned()).or_default() += usize::from(t["status"] != "done");
            }
        }
        let mut tasks: Vec<Value> = all
            .iter()
            .filter(|t| t["parent"].as_str().is_none_or(str::is_empty))
            .filter(|t| {
                let open = t["status"] != "done";
                let due = t["due"].as_str().unwrap_or_default();
                match view {
                    "today" => open && !due.is_empty() && due <= today.as_str(),
                    "upcoming" => {
                        open && !due.is_empty() && due > today.as_str() && due <= week.as_str()
                    }
                    "done" => !open,
                    "all" => true,
                    other => other.strip_prefix("list:").is_some_and(|l| t["list"] == l),
                }
            })
            .cloned()
            .collect();
        // Every listed task brings its subtasks, whatever the view: a subtask
        // belongs with its task, not with a date.
        let shown: std::collections::BTreeSet<String> = tasks
            .iter()
            .filter_map(|t| t["id"].as_str().map(str::to_owned))
            .collect();
        tasks.extend(
            all.iter()
                .filter(|t| t["parent"].as_str().is_some_and(|p| shown.contains(p)))
                .cloned(),
        );
        tasks.sort_by(|a, b| {
            let key = |t: &Value| {
                (
                    t["status"] == "done",
                    t["due"]
                        .as_str()
                        .filter(|d| !d.is_empty())
                        .unwrap_or("9999-99-99")
                        .to_owned(),
                    t["created"].as_u64().unwrap_or(0),
                )
            };
            key(a).cmp(&key(b))
        });
        Ok(json!({ "tasks": tasks, "lists": lists }))
    }

    fn write(&self, id: &str, task: &Value, rev: Option<u64>) -> Outcome {
        check_fields(task)?;
        let written = sdk(self.client.put(COLLECTION, id, task, rev))?;
        let text = format!(
            "{} {}",
            task["notes"].as_str().unwrap_or_default(),
            task["list"].as_str().unwrap_or_default()
        );
        let _ = self
            .client
            .index(id, task["title"].as_str().unwrap_or_default(), &text, false);
        self.reminder(id, task);
        Ok(json!({ "id": id, "rev": written["rev"] }))
    }

    /// Keep the core's job for this task's reminder in step with the task.
    fn reminder(&self, id: &str, task: &Value) {
        let name = format!("remind-{id}");
        match task["remind_at"]
            .as_u64()
            .filter(|_| task["status"] != "done")
        {
            Some(at) if at > now_ms() => {
                let _ = self.client.call(
                    "schedule.add",
                    json!({ "name": name, "at": at, "payload": { "task": id } }),
                );
            }
            _ => {
                let _ = self.client.call("schedule.cancel", json!({ "name": name }));
            }
        }
    }

    fn activity(&self, id: &str, title: &str, kind: &str) {
        let _ = self.client.publish(
            "activity",
            &json!({ "id": id, "title": title, "kind": kind }),
        );
    }

    pub fn create(&self, fields: &Value) -> Outcome {
        let id = new_id();
        let now = now_ms();
        let task = json!({
            "title": fields["title"].as_str().unwrap_or_default().trim(),
            "notes": fields["notes"].as_str().unwrap_or_default(),
            "list": fields["list"].as_str().unwrap_or_default().trim(),
            "parent": fields["parent"].as_str().unwrap_or_default(),
            "due": fields["due"].as_str().unwrap_or_default(),
            "rrule": fields["rrule"].as_str().unwrap_or_default(),
            "remind_at": fields["remind_at"].as_u64(),
            "status": "open", "column": "todo",
            "created": now, "updated": now, "done_count": 0,
        });
        let out = self.write(&id, &task, Some(0))?;
        self.activity(&id, task["title"].as_str().unwrap_or_default(), "created");
        Ok(out)
    }

    pub fn get(&self, id: &str) -> Outcome {
        let found = sdk(self.client.get(COLLECTION, id))?
            .ok_or_else(|| fail("not_found", "no such task"))?;
        let mut task = found["doc"].clone();
        task["id"] = json!(id);
        task["rev"] = found["rev"].clone();
        Ok(task)
    }

    /// Change some fields; the rest stay. Refused if `rev` is out of date.
    pub fn update(&self, id: &str, changes: &Value, rev: u64) -> Outcome {
        let mut task = self.get(id)?;
        let Some(object) = task.as_object_mut() else {
            return Err(fail("internal", "not an object"));
        };
        for key in [
            "title",
            "notes",
            "list",
            "due",
            "rrule",
            "remind_at",
            "column",
        ] {
            if let Some(value) = changes.get(key) {
                object.insert(key.to_owned(), value.clone());
            }
        }
        if changes["column"] == "done" {
            object.insert("status".into(), json!("done"));
        } else if changes.get("column").is_some() {
            object.insert("status".into(), json!("open"));
        }
        object.insert("updated".into(), json!(now_ms()));
        object.remove("id");
        object.remove("rev");
        let out = self.write(id, &task, Some(rev))?;
        self.activity(id, task["title"].as_str().unwrap_or_default(), "edited");
        Ok(out)
    }

    /// Done. A repeating task instead moves to its next date after the later
    /// of its due date and today, and stays open.
    pub fn complete(&self, id: &str, rev: u64, today: &str) -> Outcome {
        let mut task = self.get(id)?;
        let rule = task["rrule"].as_str().unwrap_or_default().to_owned();
        let due = task["due"].as_str().and_then(Date::parse);
        let today =
            Date::parse(today).ok_or_else(|| fail("invalid_params", "today is YYYY-MM-DD"))?;
        let count = task["done_count"].as_u64().unwrap_or(0) + 1;
        let mut repeated = None;
        if let (false, Some(due)) = (rule.is_empty(), due) {
            let rule = Rule::parse(&rule).map_err(|e| fail("invalid_params", e))?;
            let start = task["series_start"]
                .as_str()
                .and_then(Date::parse)
                .unwrap_or(due);
            let next = rule.next(start, due.max(today));
            // A reminder moves with the task, by the same number of days.
            if let Some(at) = task["remind_at"].as_u64() {
                let shift = (next.days() - due.days()).max(0).cast_unsigned() * 86_400_000;
                task["remind_at"] = json!(at + shift);
            }
            task["series_start"] = json!(start.to_string());
            task["due"] = json!(next.to_string());
            task["column"] = json!("todo");
            repeated = Some(next.to_string());
        } else {
            task["status"] = json!("done");
            task["column"] = json!("done");
            task["completed"] = json!(now_ms());
        }
        task["done_count"] = json!(count);
        task["updated"] = json!(now_ms());
        if let Some(o) = task.as_object_mut() {
            o.remove("id");
            o.remove("rev");
        }
        let out = self.write(id, &task, Some(rev))?;
        let _ = self.client.publish("task.completed", &json!({ "id": id }));
        Ok(json!({ "id": id, "rev": out["rev"], "next": repeated }))
    }

    pub fn reopen(&self, id: &str, rev: u64) -> Outcome {
        self.update(id, &json!({ "column": "todo" }), rev)
    }

    /// Delete a task and its subtasks.
    pub fn delete(&self, id: &str) -> Outcome {
        let mut removed = Vec::new();
        for task in self.all()? {
            let tid = task["id"].as_str().unwrap_or_default().to_owned();
            if tid == id || task["parent"] == id {
                let _ = self.client.call(
                    "storage.delete",
                    json!({ "collection": COLLECTION, "id": tid }),
                );
                let _ = self.client.call("search.remove", json!({ "id": tid }));
                let _ = self.client.call(
                    "schedule.cancel",
                    json!({ "name": format!("remind-{tid}") }),
                );
                removed.push(tid);
            }
        }
        Ok(json!({ "deleted": removed }))
    }

    /// A reminder fell due: tell the person, through the shared notification list.
    pub fn remind(&self, payload: &Value) {
        let Some(id) = payload["task"].as_str() else {
            return;
        };
        if let Ok(task) = self.get(id) {
            if task["status"] != "done" {
                let body = task["due"]
                    .as_str()
                    .filter(|d| !d.is_empty())
                    .map(|d| format!("Due {d}"))
                    .unwrap_or_default();
                let _ = self.client.call(
                    "notifications.post",
                    json!({ "title": task["title"], "body": body, "urgency": "normal" }),
                );
            }
        }
    }

    pub fn export_ics(&self, list: Option<&str>) -> Outcome {
        let tasks: Vec<Value> = self
            .all()?
            .into_iter()
            .filter(|t| list.is_none_or(|l| t["list"] == l))
            .collect();
        let stamp = {
            let ms = now_ms();
            let date = Date::from_days(i64::try_from(ms / 86_400_000).unwrap_or(0));
            let secs = (ms / 1000) % 86_400;
            format!(
                "{}T{:02}{:02}{:02}Z",
                date.to_string().replace('-', ""),
                secs / 3600,
                secs / 60 % 60,
                secs % 60
            )
        };
        Ok(json!({ "ics": crate::ics::export(&tasks, &stamp), "count": tasks.len() }))
    }

    /// Import VTODOs; subtasks keep their parent through RELATED-TO.
    pub fn import_ics(&self, text: &str) -> Outcome {
        let items = crate::ics::import(text);
        let mut ids: BTreeMap<String, String> = BTreeMap::new();
        let mut skipped = 0;
        // Parents first, so a subtask can point at its parent's new id.
        let (parents, children): (Vec<Value>, Vec<Value>) = items
            .into_iter()
            .partition(|t| t.get("parent_uid").is_none());
        for mut item in parents.into_iter().chain(children) {
            if let Some(uid) = item["parent_uid"].as_str() {
                item["parent"] = json!(ids.get(uid).cloned().unwrap_or_default());
            }
            if item["rrule"]
                .as_str()
                .is_some_and(|r| Rule::parse(r).is_err())
            {
                item["rrule"] = json!(""); // a rule we do not run is dropped, not mangled
            }
            let done = item["status"] == "done";
            match self.create(&item) {
                Ok(created) => {
                    let id = created["id"].as_str().unwrap_or_default().to_owned();
                    if done {
                        let _ = self.update(
                            &id,
                            &json!({ "column": "done" }),
                            created["rev"].as_u64().unwrap_or(1),
                        );
                    }
                    if let Some(uid) = item["uid"].as_str() {
                        ids.insert(uid.to_owned(), id);
                    }
                }
                Err(_) => skipped += 1,
            }
        }
        Ok(json!({ "imported": ids.len(), "skipped": skipped }))
    }

    /// Answer other applications: `open`, `focus`, `task.create`.
    pub fn answer(&self, id: &Value, method: &str, params: &Value) -> Option<Value> {
        let (answer, show) = match method {
            "open" => {
                let task = params["id"].as_str().unwrap_or_default();
                match self.get(task) {
                    Ok(_) => (Ok(json!({ "opened": task })), Some(json!({ "open": task }))),
                    Err(e) => (Err(e), None),
                }
            }
            "focus" => (
                Ok(json!({ "focused": true })),
                Some(json!({ "focus": true })),
            ),
            "task.create" => {
                let mut fields = Map::new();
                for key in ["title", "notes", "list", "due"] {
                    if let Some(v) = params.get(key) {
                        fields.insert(key.into(), v.clone());
                    }
                }
                match self.create(&Value::Object(fields)) {
                    Ok(created) => {
                        let task = created["id"].clone();
                        (Ok(created), Some(json!({ "open": task })))
                    }
                    Err(e) => (Err(e), None),
                }
            }
            _ => (Err(fail("unknown_method", "not provided")), None),
        };
        let reply = answer.map_err(|e| {
            eon_office_sdk::AppError::new(
                e["code"].as_str().unwrap_or("error"),
                e["message"].as_str().unwrap_or_default(),
            )
        });
        let _ = self.client.answer(id, reply);
        show
    }
}
