---
id: TASK-24
title: Windows floating pill overlay
status: In Review
updated_date: '2026-04-24 20:00'
assignee: []
created_date: '2026-04-24 18:45'
labels:
  - windows
  - ui
dependencies:
  - TASK-23
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Windows equivalent of the macOS pill HUD (TASK-6). Borderless, topmost, click-through-when-active WinUI 3 window showing dictation state: idle dots → recording level meter → transcribing spinner. Uses shared identity tokens (TASK-23) so the pill is recognizable as the same visual indicator users see on Mac. Mirrors the Mac pill's functional language — same states, same orange `#E07000`, same 12-bar level meter — with Windows-native implementation (Mica/Acrylic instead of `NSVisualEffectView`, WinUI Composition instead of SwiftUI `Canvas`).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Pill window is borderless, topmost, no taskbar entry, does not steal focus
- [ ] #2 Pill positioned at bottom-center of active monitor, 14 px above the Windows taskbar
- [ ] #3 Three states visible: idle (three dots, low opacity), recording (orange 12-bar level meter), transcribing (spinner)
- [ ] #4 Click-through enabled during recording/transcribing; idle state is clickable (opens main window)
- [ ] #5 Stays above normal windows; HIDES when a fullscreen app (game, video, presentation, borderless-fullscreen) is foreground. Mirrors Mac where fullscreen apps live on their own Space so the pill is naturally hidden behind them. Re-shows when the user leaves fullscreen.
- [ ] #6 Level meter redraws at 20 Hz using the same dB normalization as Mac (`LevelMeter.dbNormalize`)
- [ ] #7 Pill state driven by polling the Rust core phase snapshot, not Windows-side state duplication
- [ ] #8 250 ms grace delay before returning to idle after transcription, matching Mac
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. New `PillWindow.xaml` + code-behind in `apps/windows/App`. Use `AppWindow` with `OverlappedPresenter` set to borderless + topmost + no-maximize/minimize. 2. Set `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` via P/Invoke `SetWindowLongPtrW` to hide from Alt-Tab and prevent focus steal. 3. Enable click-through via `WS_EX_TRANSPARENT | WS_EX_LAYERED` toggled on phase change. 4. Mica/Acrylic backdrop via `SystemBackdrop`. 5. Custom `LevelMeter` UserControl: `Microsoft.UI.Composition` visuals or `Canvas` with 12 `Rectangle` children. 6. Subscribe to DictationService phase changes; drive states identically to `PillOverlay.swift`. 7. Verify click-through doesn't break with focused fullscreen apps — may need fallback to non-click-through with pointer-events flag.
<!-- SECTION:PLAN:END -->

## Known pitfalls

<!-- SECTION:NOTES:BEGIN -->
- WinUI 3 `AppWindow` topmost + click-through combo is known-fiddly; DWM composition can interfere with layered-window hit-testing. Budget a day for dead-ends.
- Acrylic/Mica require a valid `DispatcherQueue` and may not render on remote desktop (dev box is RDP — test pill appearance on a local session before judging).
- Windows 11 22H2+ required for best Mica result.
<!-- SECTION:NOTES:END -->
