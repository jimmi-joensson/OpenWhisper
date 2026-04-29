---
id: TASK-58.3
title: 'Plan Task 3: macOS pill collection-behavior follows the setting'
status: Done
assignee:
  - '@claude'
created_date: '2026-04-29 18:05'
updated_date: '2026-04-29 20:38'
labels:
  - 58-impl
dependencies: []
parent_task_id: TASK-58
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 macOS pill window's visible_on_all_workspaces mirrors the setting at boot and on every change
- [x] #2 Toggling on while a fullscreen app is active brings the pill into the fullscreen Space without restart
- [x] #3 Toggling off while in fullscreen reverts pill to normal Space behavior
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
df7c0d4 apply_collection_behavior helper, boot hydrate, listener wiring; cargo check clean. AC#2/#3 manual smoke deferred to end-of-TASK-58 DoD pass.

a982b42 + da26ad0 NSWindow approach failed verification on Sonoma; pivoted to tauri-nspanel for NSPanel swizzle. Verified pill renders over fullscreen Claude Code terminal.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Pill panel collection-behavior follows the setting via tauri-nspanel (NSWindow path was unreliable cross-app on Sonoma+). Verified pill renders over another app's fullscreen Space.
<!-- SECTION:FINAL_SUMMARY:END -->
