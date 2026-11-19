<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# EON Tasks

**What to do, and when.** Tasks and subtasks in lists, a board with three columns, tasks
that repeat, reminders that arrive as notifications, Today and the next seven days, and
iCalendar (`.ics`) import and export so your tasks move to and from Outlook, Thunderbird,
Apple Reminders or Nextcloud. Part of [EON Office](https://github.com/EON-Extensible-Open-Network/eon-docs/blob/main/office/roadmap.md)'s Wave 1.

> **0.1.0** — protocol `v0`, early. The window is checked end to end on Windows; Linux and
> macOS build and pass the same tests without a window.

## What it does

- **Lists** come from the tasks themselves; **Today** shows what is due or overdue,
  **Next 7 days** what comes after, **Done** what is finished.
- **Repeating tasks** — every day, every weekday, every week, month or year (the RFC 5545
  RRULE subset; anything else in an imported file is dropped, not mangled). Ticking one off
  moves it to its next date and keeps it open; its reminder moves with it.
- **Reminders** are handed to the EON Office core's scheduler and arrive in the shared
  notification list — set one, close Tasks, and it is delivered when Tasks next runs.
- **Subtasks** travel with their task in every view; deleting a task deletes them too.
- **Board** with *To do*, *Doing*, *Done*, and buttons to move a card — no dragging needed.
- **iCalendar** export (folded lines, escaped text, `RELATED-TO` for subtasks) and import
  from other producers.
- **Two windows never overwrite each other:** a change made on an out-of-date version is
  refused.
- **Keyboard:** the cursor starts in the new-task field; Enter adds; Ctrl+N returns there;
  arrows switch List/Board; Escape closes the details. Follows EON Launcher's language,
  theme, text size, contrast and motion preferences, live.

**For other applications** (if you allow them `call:org.eon.office.tasks`): `task.create`,
`open`, `focus`. Tasks announces `activity` (title only) and `task.completed`.

**What it asks for:** `storage`, `search.index`, `search.query`, `notifications` (for
reminders), `schedule` (to be woken for them), `events.publish`.

## Develop

```
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
EON_OFFICE_CORE_BIN=/path/to/eon-office-core cargo test -- --include-ignored
node tools/ui-check.mjs --tasks target/debug/eon-tasks.exe --core /path/to/eon-office-core.exe --shots shots
```

Licence `GPL-3.0-or-later` with the [EON Module ABI Exception](LICENSE-EXCEPTION.md).
Contributing: [CONTRIBUTING.md](CONTRIBUTING.md). Security: [SECURITY.md](SECURITY.md).
