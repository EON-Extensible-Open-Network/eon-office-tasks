// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 EON contributors
//
// Additional permission under GNU GPL version 3 section 7:
// see LICENSE-EXCEPTION.md (EON Module ABI Exception 1.0).

//! What a machine can check about the interface without opening it.
//!
//! These are not a substitute for using the window with a keyboard and a
//! screen reader (madde 35); they catch the regressions that need no person
//! to find: a string missing in one language, a button with no name, an input
//! with no label, script where the CSP forbids it, the interface reaching the
//! network on its own.

use serde_json::Value;

/// Both catalogues have the same keys, no empty value, the same `{placeholders}`.
#[must_use]
pub fn catalogue_problems(en: &str, tr: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let (Ok(Value::Object(en)), Ok(Value::Object(tr))) = (
        serde_json::from_str::<Value>(en),
        serde_json::from_str::<Value>(tr),
    ) else {
        return vec!["a catalogue is not a JSON object".into()];
    };
    for (key, value) in &en {
        match tr.get(key) {
            None => problems.push(format!("tr is missing `{key}`")),
            Some(other) => {
                if placeholders(value) != placeholders(other) {
                    problems.push(format!("`{key}` has different placeholders in en and tr"));
                }
            }
        }
    }
    for key in tr.keys() {
        if !en.contains_key(key) {
            problems.push(format!("en is missing `{key}`"));
        }
    }
    for (language, catalogue) in [("en", &en), ("tr", &tr)] {
        for (key, value) in catalogue {
            if value.as_str().is_none_or(|s| s.trim().is_empty()) {
                problems.push(format!("{language} `{key}` is empty"));
            }
        }
    }
    problems
}

fn placeholders(value: &Value) -> Vec<String> {
    let text = value.as_str().unwrap_or_default();
    let mut out: Vec<String> = text
        .split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}').map(|(name, _)| name.to_owned()))
        .collect();
    out.sort();
    out
}

/// Every opening tag named `tag` in `html`, as its full text up to `>`.
fn tags<'a>(html: &'a str, tag: &str) -> Vec<(usize, &'a str)> {
    let needle = format!("<{tag}");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = html[from..].find(&needle) {
        let start = from + at;
        let after = html[start + needle.len()..].chars().next();
        if matches!(after, Some(' ' | '>' | '\n' | '\t' | '/')) {
            if let Some(end) = html[start..].find('>') {
                out.push((start, &html[start..=start + end]));
            }
        }
        from = start + needle.len();
    }
    out
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!(" {name}=\"");
    let at = tag.find(&needle)? + needle.len();
    tag[at..].split('"').next()
}

/// Static accessibility and safety checks on `index.html` and the script.
#[must_use]
pub fn ui_problems(html: &str, script: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if tags(html, "html")
        .first()
        .and_then(|(_, t)| attribute(t, "lang"))
        .is_none()
    {
        problems.push("<html> has no lang".into());
    }
    // Buttons need a name: text inside, aria-label, or data-i18n that fills it.
    for (at, tag) in tags(html, "button") {
        let rest = &html[at + tag.len()..];
        let text = rest.split("</button>").next().unwrap_or_default().trim();
        let named = !text.is_empty()
            || attribute(tag, "aria-label").is_some()
            || attribute(tag, "data-i18n").is_some()
            || attribute(tag, "data-i18n-label").is_some();
        if !named {
            problems.push(format!("a button has no accessible name: {tag}"));
        }
    }
    // Inputs and selects need a label: <label for=id>, aria-label, or
    // aria-labelledby.
    for kind in ["input", "select"] {
        for (_, tag) in tags(html, kind) {
            if attribute(tag, "type") == Some("hidden") {
                continue;
            }
            let labelled = attribute(tag, "aria-label").is_some()
                || attribute(tag, "aria-labelledby").is_some()
                || attribute(tag, "data-i18n-label").is_some()
                || attribute(tag, "id").is_some_and(|id| html.contains(&format!("for=\"{id}\"")));
            if !labelled {
                problems.push(format!("an {kind} has no label: {tag}"));
            }
        }
    }
    // The CSP allows only script files from the app itself.
    for (at, tag) in tags(html, "script") {
        let body = html[at + tag.len()..]
            .split("</script>")
            .next()
            .unwrap_or_default();
        if attribute(tag, "src").is_none() || !body.trim().is_empty() {
            problems.push("inline script (the CSP forbids it)".into());
        }
    }
    if html.contains(" onclick=") || html.contains(" onload=") || html.contains(" oninput=") {
        problems.push("inline event handler (the CSP forbids it)".into());
    }
    // The interface has no network of its own; everything goes through the host.
    for call in ["fetch(", "XMLHttpRequest", "WebSocket(", "EventSource("] {
        if script.contains(call) {
            problems.push(format!("the interface calls `{call}` directly"));
        }
    }
    problems
}

/// Every `invoke("name"` in the script names a command the host registers.
/// Tauri matches the Rust function's name exactly -- only arguments are
/// camel-cased -- so `invoke("exportIcs")` for `fn export_ics` fails at the
/// moment a person presses the button, and at no other.
#[must_use]
pub fn unknown_commands(script: &str, known: &[&str]) -> Vec<String> {
    let mut problems = Vec::new();
    for call in script.split("invoke(\"").skip(1) {
        let name = call.split('"').next().unwrap_or_default();
        if !known.contains(&name) {
            problems.push(format!(
                "the interface invokes `{name}`, which the host does not register"
            ));
        }
    }
    problems.sort();
    problems.dedup();
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_catalogues_are_complete() {
        assert_eq!(
            catalogue_problems(crate::EN, crate::TR),
            Vec::<String>::new()
        );
    }

    #[test]
    fn the_shipped_interface_passes() {
        let html = include_str!("../../src/index.html");
        let script = include_str!("../../src/app.js");
        assert_eq!(ui_problems(html, script), Vec::<String>::new());
    }

    #[test]
    fn missing_keys_and_placeholders_are_caught() {
        let en = r#"{"a":"Hello {name}","b":"x"}"#;
        let tr = r#"{"a":"Merhaba {isim}"}"#;
        let problems = catalogue_problems(en, tr);
        assert!(problems.iter().any(|p| p.contains("tr is missing `b`")));
        assert!(problems
            .iter()
            .any(|p| p.contains("different placeholders")));
    }

    #[test]
    fn unnamed_controls_and_inline_script_are_caught() {
        let html = r#"<html><button></button><input id="q"><script>alert(1)</script></html>"#;
        let problems = ui_problems(html, "fetch('x')");
        assert_eq!(problems.len(), 5, "{problems:?}");
    }

    #[test]
    fn a_misspelt_command_is_caught() {
        let script = r#"invoke("export_ics", {}); invoke("importIcs", {}); invoke("importIcs")"#;
        assert_eq!(
            unknown_commands(script, &["export_ics", "import_ics"]),
            vec!["the interface invokes `importIcs`, which the host does not register".to_owned()]
        );
    }

    #[test]
    fn a_labelled_page_passes() {
        let html = r#"<html lang="en"><label for="q">Search</label><input id="q" type="search">
            <button data-i18n="search.open"></button><button aria-label="Close">×</button>
            <script src="app.js"></script></html>"#;
        assert_eq!(ui_problems(html, "invoke('x')"), Vec::<String>::new());
    }
}
