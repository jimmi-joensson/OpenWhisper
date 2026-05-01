---
id: TASK-65
title: Main window — Home pane + sidebar nav (v1)
status: In Review
assignee: []
created_date: '2026-04-30 22:40'
updated_date: '2026-05-01 14:26'
labels: []
dependencies: []
documentation:
  - backlog/docs/specs/2026-05-01-home-pane-sidebar-nav.md
  - backlog/docs/plans/2026-05-01-home-pane-sidebar-nav.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the current debug-style MainWindowShell with a clean Home pane + outer sidebar nav (Home / Settings / Diagnostics). v1 is intentionally minimal: empty-state hero on first launch, single latest-transcript row after the first dictation, no history list, no stats. The existing FFI/perms/dictation debug dashboard is preserved verbatim — just relocated to a new Diagnostics pane reachable from the sidebar so users reporting bugs still have a surface to copy from. Out of scope: persistence, history list, stats, re-insert from row, sidebar collapse, any new Rust commands.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Outer sidebar nav rendered with Home / Settings / Diagnostics items; clicks switch the active route.
- [ ] #2 Settings gear icon removed from the titlebar; Settings is reachable only via the sidebar (and the existing tray Preferences… → ow_navigate path).
- [ ] #3 Home pane shows hero (app icon + 'Ready when you are' + live hotkey hint pulled from current toggle binding) when no transcription exists yet.
- [ ] #4 Health banners (mic / hotkey / recognizer-load) render at the top of the Home pane above the hero.
- [ ] #5 Diagnostics pane renders the existing FFI / perms / dictation debug dashboard (32-bar meter, transcript box, RecordButton) verbatim — no DEV gating.
- [ ] #6 Playwright tests pass (pnpm test:ui): home spec covers hero + banners + latest-row + copy; diagnostics spec covers debug dashboard; main-window spec covers shell / sidebar routing / scroll.
- [ ] #7 After a dictation finalizes with a non-empty transcript, a single transcript row appears below the hero with the text and a relative timestamp ('just now' / '2m ago'); the row is replaced (not appended) on each subsequent dictation. Hover reveals a copy-to-clipboard button. State is in-memory only.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
All 7 subtasks landed on worktree-task-65-home-pane (commits abed3d9..f4bcec5). 56/56 Playwright; tsc clean. AC #1–#5, #7 covered by tests; AC #6 verified locally. Pending pre-PR: live-shell smoke on Mac (per plan Task 7 Step 5).
<!-- SECTION:NOTES:END -->
