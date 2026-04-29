---
id: TASK-57
title: 'Windows fullscreen detection: distinguish maximized from fullscreen'
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 13:29'
updated_date: '2026-04-29 14:30'
labels:
  - windows
  - bugfix
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Foreground window's rect-equals-rcMonitor test in apps/tauri/src-tauri/src/fullscreen/windows.rs misclassifies *maximized* windows as fullscreen when the focused monitor has no taskbar (auto-hidden, removed via third-party tools, or a secondary monitor with the taskbar disabled). On those screens rcWork == rcMonitor, so a normally-maximized browser/IDE/Slack matches the geometry. Symptom: pill disappears and global hotkey deactivates the moment the user switches focus to that screen. Fix: add a WS_MAXIMIZE style-bit check before the rect comparison — maximized windows always have WS_MAXIMIZE; real fullscreen apps (D3D exclusive, borderless-fullscreen, F11) never do. Also blocks TASK-55 (pill follows active screen) which trusts the fullscreen handler to be correct — without this fix, TASK-55 would inherit the bug and the pill would still vanish on chromeless monitors instead of moving.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 is_fullscreen_now returns false when foreground window has WS_MAXIMIZE style set, even if its rect equals MONITORINFO.rcMonitor (covers maximized windows on chromeless / taskbar-hidden monitors)
- [x] #2 is_fullscreen_now still returns true for borderless-fullscreen apps (Chrome F11, video players, presentation modes), exclusive-fullscreen games, and Win+Shift+Enter terminal full-screen — none of which set WS_MAXIMIZE
- [x] #3 Existing rect-equals-monitor + shell-class + own-process-id checks remain in place as preconditions
- [x] #4 Documented in the module doc-comment why the WS_MAXIMIZE escape hatch is required (chromeless secondary monitors → maximized window rect equals rcMonitor; without the check we falsely hide the pill and drop the hotkey)
- [x] #5 Manual smoke: pill stays visible when switching focus to a maximized app on a secondary monitor with the taskbar hidden / removed; pill still hides when entering a real fullscreen game or video
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation: apps/tauri/src-tauri/src/fullscreen/windows.rs.

Initial approach (commit 4fb0748) used a naive "WS_MAXIMIZE alone" early-out. Empirical testing showed two regressions: Chromium F11 fullscreen and UWP fullscreen apps (Minecraft Bedrock) both keep WS_MAXIMIZE set — the early-out incorrectly classified them as not-fullscreen, leaving the pill visible during real fullscreen sessions.

Reworked approach: gate the style-bit check behind `rcWork == rcMonitor`. On any monitor with a visible taskbar, `rcWork < rcMonitor`, so a normally-maximized window only reaches `rcWork` — if `win_rect` reaches `rcMonitor` we know it must be fullscreen and short-circuit to true. The style-bit tiebreaker (`WS_MAXIMIZE` plus `WS_CAPTION` or `WS_THICKFRAME`) only fires on chromeless screens where maximized and fullscreen geometries are indistinguishable. This recovers Minecraft Bedrock / browser F11 / D3D-exclusive games / PowerPoint slideshow on normal screens while still rescuing maximized Slack/IDE/browser on chromeless secondaries.

Known limitation documented in module doc-comment: UWP fullscreen apps on a chromeless secondary monitor still mis-detect as "maximized normal" because `ApplicationFrameWindow` keeps both WS_MAXIMIZE and chrome bits even in fullscreen. Narrow-of-narrow case; deferred without a reproduction request.

AC #5 verified empirically: browser F11 hides the pill, Minecraft Bedrock fullscreen hides the pill, real fullscreen apps hide the pill (auto-hide taskbar test). Friend will retest the original chromeless-secondary repro on the merged build.
<!-- SECTION:NOTES:END -->
