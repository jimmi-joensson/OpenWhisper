---
id: TASK-88.2
title: 'Plan Task 2: stats_get_summary + stats_reset Tauri cmds + stats_changed event'
status: To Do
assignee: []
created_date: '2026-05-06 06:14'
labels:
  - 88-impl
dependencies:
  - TASK-88.1
parent_task_id: TASK-88
ordinal: 54000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 stats_get_summary returns StatsSummary with words_today/words_week/words_all_time/seconds_total; correct against fixture data covering zero rows, yesterday, this-week, year-old rows
- [ ] #2 stats_reset DELETEs all rows; subsequent stats_get_summary returns empty summary
- [ ] #3 stats_changed event fires within 50 ms of record_dictation insert AND of stats_reset; emitted via shell-registered callback so core stays Tauri-unaware
- [ ] #4 Day-boundary math uses chrono::Local; no manual UTC offset arithmetic in the codebase
<!-- AC:END -->
