---
id: TASK-68
title: Windows custom titlebar (Slack-style) + sidebar continuity
status: Done
assignee: []
created_date: '2026-05-01 16:54'
updated_date: '2026-05-01 18:46'
labels: []
dependencies: []
documentation:
  - backlog/docs/specs/2026-05-01-windows-custom-titlebar.md
  - backlog/docs/plans/2026-05-01-windows-custom-titlebar.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the OS-default Windows title bar with a single dark, app-drawn titlebar (min/max/close on right). Restructure layout so the sidebar runs from y=0; titlebar inset over content column only. Mac stays on Overlay (traffic-lights). Fixes the visible seam between titlebar and sidebar on both platforms.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 On Windows, no OS title bar is drawn — single dark titlebar carries app chrome (back-arrow, Settings title, WindowControls cluster on right).
- [ ] #2 WindowControls cluster (min/max/close) is functional on Windows; close-hover is Win 11 red; maximize swaps to restore icon when window is maximized.
- [ ] #3 Sidebar column runs from window y=0 to bottom; titlebar inset is over the content column only.
- [ ] #4 Mac behavior unchanged — Overlay traffic-lights at top-left of sidebar; sidebar's first item clears the overlay zone via 38px padding-top.
- [ ] #5 Aero-snap (Win+←/→/↑/↓) works on Windows; Win 11 rounded corners visible.
- [ ] #6 openwhisper-platform-gotchas updated with the decorations + capabilities + WS_THICKFRAME gotcha.
<!-- AC:END -->
