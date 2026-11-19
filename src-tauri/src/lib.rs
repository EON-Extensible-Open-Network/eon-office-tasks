// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! EON Tasks' host, without the window: dates and repetition (`recur`),
//! iCalendar (`ics`), the tasks themselves (`tasks`), and the interface checks
//! `--selftest` runs (`checks`).

pub mod checks;
pub mod ics;
pub mod recur;
pub mod tasks;

pub const APP_ID: &str = "org.eon.office.tasks";
pub const MANIFEST: &str = include_str!("../../manifest.json");
pub const EN: &str = include_str!("../i18n/en.json");
pub const TR: &str = include_str!("../i18n/tr.json");

#[must_use]
pub fn strings(language: &str) -> serde_json::Value {
    let text = if language.starts_with("tr") { TR } else { EN };
    serde_json::from_str(text).unwrap_or(serde_json::Value::Null)
}
