---
id: TASK-34
title: Tauri Phase 3 — Main window parity with Mac ContentView
status: To Do
assignee: []
created_date: '2026-04-24 22:07'
labels:
  - tauri
  - phase-3
  - ui
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Port apps/macos/App/ContentView.swift (175 lines) to apps/tauri/src/App.tsx using shadcn/ui components + generated tokens.

Elements: header, model-load progress banner, Record button, live level meter (dB math reused verbatim from LevelMeter.swift), transcript area, status line.

Wire state stream from Rust core → React via Tauri events at 20 Hz (50 ms tick). Level meter redraw and elapsed-time display rely on this cadence.

Build a build-time script that generates apps/tauri/src/lib/tokens.ts + Tailwind theme values FROM docs/design/identity-tokens.md. Do not hardcode colors/dimensions — tokens flow from the spec.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 apps/tauri/src/App.tsx renders header, model-load banner, Record button, level meter, transcript, status line
- [ ] #2 Layout matches ContentView.swift visually (side-by-side comparison on Mac)
- [ ] #3 State stream from Rust core via Tauri events at 20 Hz; React re-renders at that cadence
- [ ] #4 Level meter uses the LevelMeter.swift dB formula verbatim (ported to TypeScript)
- [ ] #5 apps/tauri/scripts/gen-tokens.ts generates tokens.ts + Tailwind theme from docs/design/identity-tokens.md
- [ ] #6 Hardcoded color/dimension values in source tree = 0 (all flow from generated tokens)
- [ ] #7 Phase transitions from DictationService.swift surface correctly in UI
<!-- AC:END -->
