<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# Changelog

## 0.2.0 — stands alone

- **No core service, no launcher needed.** Tasks links the `eon-office-core` library and
  opens its own store in the EON Office home folder (plan madde 43;
  `eon-docs/office/design/2026-11-19-library-core.md`).
- **Reminders** go to the operating system's notification centre, checked every 20
  seconds while Tasks is open and at start: one that fell due while it was closed is shown
  once when it next opens. A moved reminder fires again at its new time. Which reminder
  was shown is kept beside the task (a `reminders` collection), not in it: firing one
  does not change the task's version, so a detail panel open on it still saves.
- **One window:** a second start, or an `eon-office://org.eon.office.tasks/<id>` link, goes
  to the running window. Replaces the `open`, `focus` and `task.create` methods and the
  `task.completed` event; completing a task adds it to the recent list instead.
- **Export and import iCalendar work from the window.** 0.1.0's buttons invoked
  `exportIcs`/`importIcs`, names the host never registered (Tauri matches the Rust
  function name, `export_ics`), so both failed when pressed. The self-test now fails the
  build when the interface invokes a command the host does not register, and the window
  check exports and re-imports through the forms.
- Tests no longer need a running core; the window check starts Tasks on its own, hands it
  a link from a second start, and watches a due reminder fire.

## 0.1.0 — first version

Tagged, never published: the release run's macOS job was never given a runner.

- Tasks and subtasks in lists; Today, Next 7 days, All, Done; a three-column board.
- Repeating tasks (RRULE subset: daily, weekdays, weekly, monthly by day, yearly incl.
  29 February); completing one moves it and its reminder to the next date.
- Reminders through the core's scheduler, delivered as notifications.
- iCalendar VTODO export and import, subtasks kept.
- Saves refused on an out-of-date version.
- Provides `task.create`, `open`, `focus`; publishes `activity`, `task.completed`.
- Tests: date arithmetic and repetition, iCalendar round trips, against a real core, and the
  window driven through DevTools.
