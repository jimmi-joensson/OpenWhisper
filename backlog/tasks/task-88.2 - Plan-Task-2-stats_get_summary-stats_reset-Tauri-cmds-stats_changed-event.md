---
id: TASK-88.2
title: 'Plan Task 2: stats_get_summary + stats_reset Tauri cmds + stats_changed event'
status: In Review
assignee:
  - '@claude'
created_date: '2026-05-06 06:14'
updated_date: '2026-05-06 08:50'
labels:
  - 88-impl
dependencies:
  - TASK-88.1
parent_task_id: TASK-88
ordinal: 54000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 stats_get_summary returns StatsSummary with words_today/words_week/words_all_time/seconds_total; correct against fixture data covering zero rows, yesterday, this-week, year-old rows
- [x] #2 stats_reset DELETEs all rows; subsequent stats_get_summary returns empty summary
- [ ] #3 stats_changed event fires within 50 ms of record_dictation insert AND of stats_reset; emitted via shell-registered callback so core stays Tauri-unaware
- [x] #4 Day-boundary math uses chrono::Local; no manual UTC offset arithmetic in the codebase
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Commit: fc94338. cargo test -p openwhisper-core --lib stats:: → 6/6. AC #3 (event fires within 50ms) deferred to manual smoke (DevTools listen). chrono::Local used for day/week boundaries — no UTC offset arithmetic anywhere.

fc94338 TASK-88.2: read cmds + stats_changed callback
<!-- SECTION:NOTES:END -->
