---
id: TASK-32
title: Tauri Phase 1 — Pill HUD spike
status: Done
assignee: []
created_date: '2026-04-24 22:07'
updated_date: '2026-04-24 22:25'
labels:
  - tauri
  - phase-1
  - cross-platform
  - ui
  - risk-burn-down
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build the floating pill window as a spike BEFORE the main window. Pill is the signature visual and the piece that failed visual parity on WinUI 3 — burn down risk early.

Second Tauri window: decorations off, transparent, always-on-top, skip taskbar. CSS capsule via border-radius + backdrop-filter for material. Click-through via set_ignore_cursor_events toggled by mock state. Three states (idle dots, orange 12-bar level meter, transcribing spinner) driven by hardcoded mock input — NO core integration in this phase.

Visual reference: apps/macos/App/PillOverlay.swift (215 lines). Must look and feel like that.

Gate: if visual parity unachievable across Mac + Windows (including the RDP dev box), pause and reconsider approach before Phase 2.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Second Tauri window configured borderless, transparent, always-on-top, skip taskbar
- [ ] #2 CSS capsule shape with backdrop-filter blur; flat fallback when prefers-reduced-transparency or RDP detected
- [ ] #3 Three states render from a mock state variable: idle dots, orange 12-bar meter, spinner
- [ ] #4 Click-through toggles correctly — clickable when idle, pass-through during recording/transcribing mock states
- [ ] #5 Visual diff vs PillOverlay.swift acceptable on Mac (side-by-side screenshot check)
- [ ] #6 Pill renders correctly on Windows 11 local session
- [ ] #7 Pill renders acceptably on Windows 11 RDP (material may fall back to flat; geometry must hold)
- [ ] #8 Positioned bottom-center above Dock/taskbar on both OSes
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Phase 1 pill HUD spike code complete. Second Tauri window labeled "pill" configured borderless / transparent / always-on-top / skip-taskbar / non-focusable in tauri.conf.json. PillOverlay.tsx renders three states (idle dots, orange 12-bar level meter, transcribing spinner+meter) driven by tauri events from the main window emitTo(). CSS capsule via border-radius:9999px + backdrop-filter blur with prefers-reduced-transparency flat fallback for RDP. Rust commands: set_pill_click_through (toggles ignore_cursor_events) + position_pill_bottom_center (current_monitor → 80px-from-bottom centered, logical coords). Capabilities/default.json grants both windows event + window perms. main.tsx routes by getCurrentWindow().label. Compile-checked: cargo check -p openwhisper-tauri + pnpm build green.

GUI verification (ACs 5/6/7/8 — Mac visual diff vs PillOverlay.swift, Windows local, Windows RDP, bottom-center positioning) deferred to user — run `cd apps/tauri && pnpm tauri dev`. Known gaps: (1) Tauri alwaysOnTop sets PopUp level, not NSWindow.statusBar — Figma-style HUDs may sit above the pill on Mac; address in Phase 7 via objc2 if it matters. (2) 80px bottom margin is naive; Phase 7 swaps for NSScreen.visibleFrame / GetMonitorInfo rcWork. (3) StrictMode double-mounts effects in dev — pill positioning + click-through invokes fire twice; idempotent so harmless.
<!-- SECTION:FINAL_SUMMARY:END -->
