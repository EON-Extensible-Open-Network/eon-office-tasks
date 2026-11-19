<!--
SPDX-License-Identifier: GPL-3.0-or-later
SPDX-FileCopyrightText: 2026 EON contributors
-->

# Security

Report privately through GitHub's **Security → Report a vulnerability** on this repository;
the organisation-wide policy is
[`.github/SECURITY.md`](https://github.com/EON-Extensible-Open-Network/.github/blob/main/SECURITY.md).

- **Imported iCalendar is untrusted.** It is parsed into plain fields; anything unknown is
  ignored, and text reaches the page only as text. The interface has no network (CI fails the build on `fetch`, `XMLHttpRequest`,
  `WebSocket`, `EventSource`, and on `innerHTML`).
- **Tasks holds its own token only** (from the core's launch or its token file) and asks
  for six capabilities; it cannot read other applications' data.
- **The DevTools port is debug-only** (`EON_WEBVIEW_ARGS`, ignored by release builds).
- Known, tracked: the Linux build inherits Tauri's GTK 3 stack and its `glib` advisory
  (GHSA-wrw7-89jp-8q8g), as described in eon-office-launcher's `SECURITY.md`.

The core's threat model:
[`eon-office-core/SECURITY.md`](https://github.com/EON-Extensible-Open-Network/eon-office-core/blob/main/SECURITY.md).
