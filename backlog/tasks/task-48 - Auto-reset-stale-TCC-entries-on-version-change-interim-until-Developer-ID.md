---
id: TASK-48
title: Auto-reset stale TCC entries on version change (interim until Developer ID)
status: To Do
assignee: []
created_date: '2026-04-27 07:41'
labels: []
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Ad-hoc signing makes TCC bind grants to cdhash. Each rebuild = new cdhash = stale toggle in System Settings, leaving users confused after updates ("OpenWhisper" entry already toggled on, yet AX prompt fires again).

On boot:
1. Read `CFBundleShortVersionString` from own bundle.
2. Read prior value from `~/Library/Application Support/com.openwhisper.app/last-version` (Mac) / `%APPDATA%\\OpenWhisper\\last-version` (Win).
3. If different OR missing AND `AXIsProcessTrusted()` is false: spawn `tccutil reset Accessibility com.openwhisper.app` and `tccutil reset Microphone com.openwhisper.app`.
4. Write current version to the file.
5. Proceed with normal AX/mic prompt flow — fresh entries get added cleanly.

Why: removes the "ghost stale entry" UX dead-end without users running tccutil from Terminal. Costs one re-grant per update, same as today.

Obsoleted by: Developer ID signing (in flight). Once builds anchor to Team ID + bundle id, TCC grants survive rebuilds and this whole detector becomes unnecessary — delete it then.

Scope:
- Mac only initially (CGEventTap path is the painful one). Win has no equivalent TCC service for keyboard input.
- Wire into `apps/tauri/src-tauri/src/permissions/mac.rs` boot path, before `request_microphone`.
- No UI; silent reset.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 App boots cleanly after a version bump; user sees fresh AX prompt instead of toggled-on stale entry
- [ ] #2 On unchanged version, no tccutil call fires (idempotent)
- [ ] #3 First-ever launch writes the version file without resetting (file absent + AX false = first run, not stale)
- [ ] #4 Marked superseded once Developer ID signing ships and TCC grants survive rebuilds
<!-- AC:END -->
