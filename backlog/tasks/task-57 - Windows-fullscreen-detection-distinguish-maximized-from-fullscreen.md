---
id: TASK-57
title: 'Windows fullscreen detection: distinguish maximized from fullscreen'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-04-29 13:29'
updated_date: '2026-04-29 13:31'
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
- [ ] #5 Manual smoke: pill stays visible when switching focus to a maximized app on a secondary monitor with the taskbar hidden / removed; pill still hides when entering a real fullscreen game or video
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation: apps/tauri/src-tauri/src/fullscreen/windows.rs — added GetWindowLongPtrW + GWL_STYLE + WS_MAXIMIZE imports; inserted style-bit early-out between shell-class skip and rect comparison. Updated module doc-comment with the chromeless-monitor rationale. cargo check + cargo test --lib both green. AC #5 (manual smoke on a chromeless secondary monitor) cannot be verified from this dev box (RDP, single virtual display per openwhisper-platform-gotchas) — left to user / reporter to confirm before flipping to Done.
<!-- SECTION:NOTES:END -->
