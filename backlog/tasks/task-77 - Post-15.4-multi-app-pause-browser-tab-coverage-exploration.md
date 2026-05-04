---
id: TASK-77
title: Post-15.4 multi-app pause + browser-tab coverage exploration
status: To Do
assignee: []
created_date: '2026-05-03 10:19'
updated_date: '2026-05-04 08:03'
labels: []
dependencies: []
priority: low
ordinal: 32000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Follow-up from TASK-61 v0.5 implementation. macOS 15.4 closed MediaRemote SET + READ APIs to non-Apple-signed processes, leaving AppleScript-per-app as the only deterministic Mac path for v0.5. Limitations shipped in 0.5.0:

- Browser-tab media (Safari/Chrome/Firefox) is not paused on Mac (no per-tab AppleScript)
- One-time Automation TCC prompt per AppleScript-driven app (Spotify, Music)
- Multi-app pause (e.g. Spotify + browser tab simultaneously) is not deterministic

Three v0.5 release-blocker rework attempts (commits 0e387c6, 96a8986, 9a7fa66) were reverted in 3fefd57 — see TASK-61.3 implementation notes for the full sequence and why none worked post-15.4. Iteration-budget rule fired at attempt #4.

## Possible paths for a future release

1. ungive/mediaremote-adapter Perl-bridge — the only working post-15.4 third-party route; well past 'non-hacky' for what is a v0.5 nicety. Productionising would mean shipping + invoking a Perl helper at runtime.
2. Wait for Apple to re-open MediaRemote — unlikely.
3. Per-browser DevTools-protocol attach — Chrome/Safari debug ports. Heavy, fragile, per-browser, opens its own consent surface.
4. Audio-tap / process-audio detection (CoreAudio 14.4+ tap APIs) — could detect 'is anything outputting' more deterministically. Still doesn't solve pausing browser tabs.

## When to pick this up

Open only if the limitation actually bites users (file user reports here as evidence). Otherwise leave deferred.
<!-- SECTION:DESCRIPTION:END -->
