---
id: TASK-36
title: Tauri Phase 5 — Dictation flow + text injection
status: To Do
assignee: []
created_date: '2026-04-24 22:07'
labels:
  - tauri
  - phase-5
  - injection
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
End-to-end dictation: hotkey → record → recognize → post-process → paste to focused field.

Wire the recognizer from Phase 2 into the full dictation flow. Audio capture (core::audio, cpal) → streaming recognize (sherpa-rs or FluidAudio fallback) → transcript post-processing (core::transcript) → delivery via paste.

Text injection: port apps/macos/App/TextInjector.swift (79 lines) to Rust. Use arboard for clipboard + platform key-event synthesis (evaluate enigo vs direct platform APIs). Clipboard-save → paste → 200 ms restore delay matches Mac. Mirror same flow on Windows (Ctrl+V).

Fullscreen-aware: pill hides when fullscreen app is foreground; hotkey unregisters so fullscreen app receives Ctrl+Space / Right Cmd normally. Re-register on exit. Reference: apps/windows/OpenWhisper/ PillWindow.xaml.cs fullscreen detect logic + apps/macos/App/PillOverlay.swift grace-return.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Full dictation flow works end-to-end on Mac Tauri: hotkey → pill appears → record → recognize → post-process → paste into focused app
- [ ] #2 Full dictation flow works end-to-end on Windows Tauri
- [ ] #3 Clipboard preserved: original clipboard content restored after 200 ms
- [ ] #4 Fullscreen app detection hides pill + unregisters hotkey; re-registers on exit
- [ ] #5 Verified across Chromium fullscreen, video apps, and at least one fullscreen game
- [ ] #6 Injection path uses arboard + platform key-event synthesis; no shell-out to osascript/powershell
<!-- AC:END -->
