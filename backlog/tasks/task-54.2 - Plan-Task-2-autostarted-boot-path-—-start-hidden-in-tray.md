---
id: TASK-54.2
title: 'Plan Task 2: --autostarted boot path — start hidden in tray'
status: Won't Do
assignee: []
created_date: '2026-04-29 17:44'
updated_date: '2026-04-30 16:32'
labels:
  - 54-impl
dependencies: []
parent_task_id: TASK-54
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 setup() parses --autostarted from std::env::args and records the boolean
- [ ] #2 When the flag is present, the main window is not shown / focused on boot; tray + pill remain functional
- [ ] #3 When the flag is absent, the existing boot behavior is unchanged
- [ ] #4 Manual smoke confirms both boot paths
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review as Won't Do. Parent TASK-54 closed in favor of TASK-60.
<!-- SECTION:FINAL_SUMMARY:END -->
