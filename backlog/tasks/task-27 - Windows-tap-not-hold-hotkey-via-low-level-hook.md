---
id: TASK-27
title: 'Windows tap-not-hold hotkey via low-level keyboard hook'
status: Won't Do
assignee: []
created_date: '2026-04-24 18:45'
updated_date: '2026-04-24 21:30'
labels:
  - windows
  - input
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
**Won't Do — superseded by deliberate platform-convention choice.**

Original premise (port Mac's Right-Command tap-not-hold to Windows) was
abandoned 2026-04-24. Windows uses `Ctrl + Space` chord via
`RegisterHotKey` — idiomatic Windows, zero keyboard-hook complexity, no AV
friction. Mac uses Right Command tap-not-hold via `CGEventTap`. The two
platforms intentionally differ on activation semantics even though they
share visual identity (TASK-23).

See `GlobalHotkey.cs` docstring and the "Hotkey differs per platform"
memory for the full reasoning.

Global Escape-to-cancel (the only other global-keyboard surface we want on
Windows) is handled by its own minimal hook in TASK-28 — not by the
broader tap-not-hold machinery originally proposed here.
<!-- SECTION:DESCRIPTION:END -->
