<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# EON Tasks

**What to do, and when.** Tasks and subtasks in lists, a board with three columns, tasks
that repeat, reminders that arrive as notifications, Today and the next seven days, and
iCalendar (`.ics`) import and export so your tasks move to and from Outlook, Thunderbird,
Apple Reminders or Nextcloud. Part of [EON Office](https://github.com/EON-Extensible-Open-Network/eon-docs/blob/main/office/roadmap.md)'s Wave 1.

> **0.2.0** — formats `v0`, early. **Stands alone:** download it, run it. Nothing else
> has to be installed or running. The window is checked end to end on Windows; Linux and
> macOS build and pass the same tests without a window.

## What it does

- **Lists** come from the tasks themselves; **Today** shows what is due or overdue,
  **Next 7 days** what comes after, **Done** what is finished.
- **Repeating tasks** — every day, every weekday, every week, month or year (the RFC 5545
  RRULE subset; anything else in an imported file is dropped, not mangled). Ticking one off
  moves it to its next date and keeps it open; its reminder moves with it.
- **Reminders** appear in your operating system's notification centre. Each fires once;
  one that fell due while Tasks was closed is shown when you next open it.
- **Subtasks** travel with their task in every view; deleting a task deletes them too.
- **Board** with *To do*, *Doing*, *Done*, and buttons to move a card — no dragging needed.
- **iCalendar** export (folded lines, escaped text, `RELATED-TO` for subtasks) and import
  from other producers.
- **Two windows never overwrite each other:** a change made on an out-of-date version is
  refused.
- **Keyboard:** the cursor starts in the new-task field; Enter adds; Ctrl+N returns there;
  arrows switch List/Board; Escape closes the details. Follows the language, theme, text
  size, contrast and motion preferences shared by EON Office applications, live.

- **One window.** Starting Tasks again, or following a link to a task, brings the open
  window forward.

**With other EON Office applications** (all optional): `eon-office://org.eon.office.tasks/<id>`
opens a task from any application that links to it. Tasks adds what you create, edit and
complete to the shared *Recent* list — titles only.

**What it asks for:** keep its data on this device (`storage`), make its tasks searchable
(`search.index`, private to Tasks), search (`search.query`), show reminders
(`notifications`, `schedule`), add titles to your recent list (`events.publish`).

## Install

Download `eon-tasks` from a release and run it — see the release notes for checksums. Or,
with EON Launcher: *Get applications* → *Include previews* → EON Tasks.

## Develop

```
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
node tools/ui-check.mjs --tasks target/debug/eon-tasks.exe --shots shots
```

Licence `GPL-3.0-or-later` with the [EON Module ABI Exception](LICENSE-EXCEPTION.md).
Contributing: [CONTRIBUTING.md](CONTRIBUTING.md). Security: [SECURITY.md](SECURITY.md).
