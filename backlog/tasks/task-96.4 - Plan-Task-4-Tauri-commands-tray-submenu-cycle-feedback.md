---
id: TASK-96.4
title: 'Plan Task 4: Tauri commands, tray submenu, cycle feedback'
status: In Review
assignee: []
created_date: '2026-11-10T16:14:00Z'
updated_date: '2026-11-10T17:37:00Z'
labels:
  - 96-impl
dependencies: []
parent_task_id: TASK-96
ordinal: 97000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tray menu has a Mode submenu with the selected mode checked
- [ ] #2 A cycle emits modes_changed and the pill shows the mode name briefly
- [x] #3 The dictation tick carries mode name, reason and app
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
655f14d shell commands in apps/tauri/src-tauri/src/modes.rs (thin wrappers over core), Mode submenu in the tray (rebuilt by the existing tray watcher thread), pill label after a switch (text chip above the capsule, 1.5 s, fades, takes no clicks). 41 shell tests, Windows target check clean (orchestrator runs). Not verified, needs a person: the tray submenu and its check mark, the label in the real pill window. ACs 1 and 2 stay unchecked until seen in the app.
<!-- SECTION:NOTES:END -->
