---
id: TASK-23
title: Shared visual identity tokens
status: Done
updated_date: '2026-04-24 19:30'
assignee: []
created_date: '2026-04-24 18:45'
labels:
  - design
  - cross-platform
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Single source of truth for OpenWhisper's visual identity across macOS, Windows, and (future) Linux. Users must recognize the app as the same product on every platform without us chasing pixel-perfect material matching. Tokens cover: recording/brand colors, type scale, corner radii, motion curves, iconography, HUD geometry. Each shell consumes the spec via its native resource system (Swift `Color` extensions / `Assets.xcassets`, WinUI `ResourceDictionary`, GTK CSS) — no runtime/shared format required, but the spec document is the authority.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `docs/design/identity-tokens.md` exists with color, type, radii, motion, HUD geometry specs
- [x] #2 Recording orange `#E07000`, idle grays, material/backdrop choices, and text opacities documented
- [x] #3 Type scale (title/body/caption) documented with per-platform font stack (SF Pro / Segoe UI Variable / system)
- [x] #4 HUD pill geometry documented: 70×22 px, 14 px above taskbar/Dock, corner radius, border, material
- [x] #5 Motion specs: pill state-transition timings, level-meter redraw rate (20 Hz), grace-return delay (250 ms)
- [x] #6 macOS token sources cross-referenced against the spec — values match as of 2026-04-24 drift check (§9 of spec). No consolidation file added: brand color already isolated in `NSColor.openWhisperRecording`, geometry co-located with owning views. Moving constants into a separate tokens file would add indirection without payoff.
- [x] #7 Windows `App.xaml` `ResourceDictionary` populated with matching values; referenced by HUD + tray icon tasks
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Audit existing Mac visual values (PillOverlay.swift, OpenWhisperApp.swift icon drawing, LevelMeter.swift, ContentView.swift) and extract concrete numbers. 2. Write `docs/design/identity-tokens.md` as the spec. 3. Refactor Mac code to pull from a single `OpenWhisperTokens.swift` if values are scattered. 4. Add `App.xaml` `ResourceDictionary` on Windows with the same tokens. 5. Leave Linux as TODO — spec-only, no implementation yet.
<!-- SECTION:PLAN:END -->
