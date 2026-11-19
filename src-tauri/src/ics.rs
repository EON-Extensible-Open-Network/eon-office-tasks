// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! Tasks as iCalendar `VTODO` (RFC 5545), so they move to and from Outlook,
//! Thunderbird, Apple Reminders, Nextcloud and anything else that speaks it.
//!
//! Written: UID, SUMMARY, DESCRIPTION, DUE (date), STATUS, COMPLETED, RRULE,
//! RELATED-TO (parent), CATEGORIES (the list). Read: the same, from any
//! producer -- lines unfolded, text unescaped, unknown properties ignored.

use serde_json::{json, Value};

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push('\n'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Lines longer than 75 octets are folded, as the RFC requires.
fn fold(line: &str, out: &mut String) {
    let mut width = 0;
    for c in line.chars() {
        if width + c.len_utf8() > 75 {
            out.push_str("\r\n ");
            width = 1;
        }
        out.push(c);
        width += c.len_utf8();
    }
    out.push_str("\r\n");
}

fn compact(date: &str) -> String {
    date.replace('-', "")
}

/// One calendar with every task given.
pub fn export(tasks: &[Value], stamp: &str) -> String {
    let mut out = String::new();
    for line in [
        "BEGIN:VCALENDAR",
        "VERSION:2.0",
        "PRODID:-//EON//EON Tasks//EN",
        "CALSCALE:GREGORIAN",
    ] {
        fold(line, &mut out);
    }
    for task in tasks {
        fold("BEGIN:VTODO", &mut out);
        fold(
            &format!(
                "UID:{}@eon-office-tasks",
                task["id"].as_str().unwrap_or_default()
            ),
            &mut out,
        );
        fold(&format!("DTSTAMP:{stamp}"), &mut out);
        fold(
            &format!(
                "SUMMARY:{}",
                escape(task["title"].as_str().unwrap_or_default())
            ),
            &mut out,
        );
        if let Some(notes) = task["notes"].as_str().filter(|n| !n.is_empty()) {
            fold(&format!("DESCRIPTION:{}", escape(notes)), &mut out);
        }
        if let Some(due) = task["due"].as_str().filter(|d| !d.is_empty()) {
            fold(&format!("DUE;VALUE=DATE:{}", compact(due)), &mut out);
        }
        if let Some(rule) = task["rrule"].as_str().filter(|r| !r.is_empty()) {
            fold(&format!("RRULE:{rule}"), &mut out);
        }
        if let Some(parent) = task["parent"].as_str().filter(|p| !p.is_empty()) {
            fold(&format!("RELATED-TO:{parent}@eon-office-tasks"), &mut out);
        }
        if let Some(list) = task["list"].as_str().filter(|l| !l.is_empty()) {
            fold(&format!("CATEGORIES:{}", escape(list)), &mut out);
        }
        let done = task["status"] == "done";
        fold(
            if done {
                "STATUS:COMPLETED"
            } else {
                "STATUS:NEEDS-ACTION"
            },
            &mut out,
        );
        fold("END:VTODO", &mut out);
    }
    fold("END:VCALENDAR", &mut out);
    out
}

fn date(value: &str) -> Option<String> {
    let digits: String = value.chars().take(8).collect();
    (digits.len() == 8 && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| format!("{}-{}-{}", &digits[..4], &digits[4..6], &digits[6..8]))
}

/// Every VTODO in `text`, as task fields. Ids are the producer's UIDs.
pub fn import(text: &str) -> Vec<Value> {
    // Unfold: a line starting with space or tab continues the previous one.
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = raw.strip_prefix(' ').or_else(|| raw.strip_prefix('\t')) {
            if let Some(last) = lines.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        lines.push(raw.to_owned());
    }
    let mut out = Vec::new();
    let mut current: Option<Value> = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let key = name
            .split(';')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        match (key.as_str(), current.as_mut()) {
            ("BEGIN", None) if value.eq_ignore_ascii_case("VTODO") => {
                current = Some(json!({ "status": "open" }));
            }
            ("END", Some(_)) if value.eq_ignore_ascii_case("VTODO") => {
                if let Some(task) = current.take() {
                    if task.get("title").is_some() {
                        out.push(task);
                    }
                }
            }
            ("UID", Some(t)) => t["uid"] = json!(value.trim_end_matches("@eon-office-tasks")),
            ("SUMMARY", Some(t)) => t["title"] = json!(unescape(value)),
            ("DESCRIPTION", Some(t)) => t["notes"] = json!(unescape(value)),
            ("DUE", Some(t)) => {
                if let Some(d) = date(value) {
                    t["due"] = json!(d);
                }
            }
            ("RRULE", Some(t)) => t["rrule"] = json!(value),
            ("RELATED-TO", Some(t)) => {
                t["parent_uid"] = json!(value.trim_end_matches("@eon-office-tasks"));
            }
            ("CATEGORIES", Some(t)) => {
                t["list"] = json!(unescape(value.split(',').next().unwrap_or_default()));
            }
            ("STATUS", Some(t)) if value.eq_ignore_ascii_case("COMPLETED") => {
                t["status"] = json!("done");
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_survives_a_round_trip() {
        let tasks = vec![
            json!({
                "id": "t1", "title": "Pay rent; then, relax", "notes": "line one\nline two",
                "due": "2026-12-01", "rrule": "FREQ=MONTHLY", "list": "Home", "status": "open",
            }),
            json!({ "id": "t2", "title": "Sub", "parent": "t1", "status": "done" }),
        ];
        let text = export(&tasks, "20261119T090000Z");
        assert!(text.lines().all(|l| l.len() <= 76), "folded");
        let back = import(&text);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0]["title"], "Pay rent; then, relax");
        assert_eq!(back[0]["notes"], "line one\nline two");
        assert_eq!(
            (
                back[0]["due"].clone(),
                back[0]["rrule"].clone(),
                back[0]["list"].clone()
            ),
            (json!("2026-12-01"), json!("FREQ=MONTHLY"), json!("Home"))
        );
        assert_eq!(
            (back[1]["parent_uid"].clone(), back[1]["status"].clone()),
            (json!("t1"), json!("done"))
        );
    }

    #[test]
    fn other_producers_files_are_read() {
        // Folded lines, a date-time DUE, extra properties, a VEVENT to ignore.
        let text = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nSUMMARY:Not a task\r\nEND:VEVENT\r\nBEGIN:VTODO\r\nUID:abc@example.org\r\nSUMMARY:Call the bank about the \r\n mortgage\r\nDUE;TZID=Europe/Istanbul:20261203T170000\r\nPRIORITY:1\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";
        let tasks = import(text);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["title"], "Call the bank about the mortgage");
        assert_eq!(tasks[0]["due"], "2026-12-03");
    }
}
