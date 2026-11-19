<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# Contributing to EON Tasks

The organisation-wide guide,
[`.github/CONTRIBUTING.md`](https://github.com/EON-Extensible-Open-Network/.github/blob/main/CONTRIBUTING.md),
applies: DCO sign-off (`git commit -s`), Conventional Commits (`feat(tasks): ...`).
By contributing you license your work under `GPL-3.0-or-later` with the EON Module ABI
Exception (`LICENSE-EXCEPTION.md`).

- **Every string in both languages.** Add it to `src-tauri/i18n/en.json` and `tr.json`
  with the same `{placeholders}`; `--selftest` fails otherwise.
- **Every control has a name and works from the keyboard.** `checks.rs` and
  `tools/ui-check.mjs` check what a machine can; say in the pull request how you checked
  the rest.
- **Task text is untrusted when imported.** It reaches the page only through `textContent`.
  CI enforces it.
- **Repetition and iCalendar are standards.** A change to `recur.rs` or `ics.rs` comes with a
  test against the RFC's own example or a real producer's file.

