---
id: TASK-55
title: Pill follows active screen
status: Done
assignee: []
created_date: '2026-04-29 07:57'
updated_date: '2026-05-01 08:13'
labels: []
dependencies:
  - TASK-58
documentation:
  - backlog/docs/specs/2026-04-29-pill-follows-active-screen.md
  - backlog/docs/plans/2026-04-29-pill-follows-active-screen.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Pill HUD jumps to bottom-center of whichever monitor the focused app lives on. Default ON, with an opt-out toggle in Settings → General. Reuses the 500 ms foreground-poll thread already in the fullscreen module so no new thread is spawned. Cross-platform: AX position+size on mac, MonitorFromWindow on win.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Watcher detects focused app's monitor every 500 ms on both mac and win
- [ ] #2 Pill window repositions to bottom-center of new monitor when foreground monitor changes
- [ ] #3 Pill follows during recording and transcribing without disrupting the SVG tween
- [ ] #4 Settings → General has 'Follow active screen' toggle, default ON, persisted across restarts
- [ ] #5 When toggle is OFF, pill stays on whichever monitor it last landed on
- [ ] #6 Single-monitor setups incur zero repositioning churn
- [ ] #7 AX-permission revoked or no focused window: pill stays put (no fallback to primary)
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review. All seven plan-tasks (55.1-55.7) shipped in v0.4.0. Pill follows active screen by cursor tracking, with Settings → General opt-out toggle and Playwright coverage.
<!-- SECTION:FINAL_SUMMARY:END -->
