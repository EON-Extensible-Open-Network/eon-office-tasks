<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# Changelog

## 0.1.0 — first version

- Tasks and subtasks in lists; Today, Next 7 days, All, Done; a three-column board.
- Repeating tasks (RRULE subset: daily, weekdays, weekly, monthly by day, yearly incl.
  29 February); completing one moves it and its reminder to the next date.
- Reminders through the core's scheduler, delivered as notifications.
- iCalendar VTODO export and import, subtasks kept.
- Saves refused on an out-of-date version.
- Provides `task.create`, `open`, `focus`; publishes `activity`, `task.completed`.
- Tests: date arithmetic and repetition, iCalendar round trips, against a real core, and the
  window driven through DevTools.
