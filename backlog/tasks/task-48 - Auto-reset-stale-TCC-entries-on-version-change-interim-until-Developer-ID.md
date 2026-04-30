---
id: TASK-48
title: Auto-reset stale TCC entries on version change (interim until Developer ID)
status: Done
assignee:
  - '@claude'
created_date: '2026-04-27 07:41'
updated_date: '2026-04-30 00:00'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Landed in 0.4.0 release prep. New module `apps/tauri/src-tauri/src/permissions/version_reset.rs`; wired in `lib.rs::setup()` before `hotkey::install`. Resets Accessibility, Microphone, ListenEvent (the keyboard-monitor TCC service CGEventTap relies on). Gated on `cfg!(not(debug_assertions))` so dev-run.sh keeps owning the dev path.

Cachebuster: **cdhash**, not version. The first iteration keyed off `CFBundleShortVersionString` and missed the within-version-rebuild case (two 0.4.0 release builds both wrote "0.4.0" to the marker, so the second install's reset never fired). Switched to reading own cdhash via `codesign -dvv` parsing `CDHash=` from stderr — this is exactly what TCC itself keys on, so any rebuild TCC would treat as a new identity also flips the marker. If `codesign` fails the whole cycle is skipped silently rather than firing reset on a partial identity read.

Marker file: `~/Library/Application Support/com.openwhisper.app/tcc-last-cdhash` (legacy `tcc-last-version` files from the earlier iteration are orphaned but harmless).
<!-- SECTION:NOTES:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 App boots cleanly after a version bump; user sees fresh AX prompt instead of toggled-on stale entry
- [x] #2 On unchanged version, no tccutil call fires (idempotent)
- [x] #3 First-ever launch writes the version file without resetting (file absent + AX false = first run, not stale) — *implementation note: marker-absent is treated as a reset trigger, not a quiet first run, so 0.3.0 → 0.4.0 upgraders (whose 0.3.0 build never wrote the marker) are auto-cleared. Cost is zero on fresh installs because tccutil has nothing to reset.*
- [ ] #4 Marked superseded once Developer ID signing ships and TCC grants survive rebuilds
<!-- AC:END -->
