---
id: TASK-54.3
title: 'Plan Task 3: Hook GeneralPane Switch through plugin'
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
- [ ] #1 New use-autostart.ts hook fronts the Rust plugin via invoke + listen
- [ ] #2 GeneralPane Switch state comes from useAutostart(); local useState(true) is gone
- [ ] #3 Dev builds render the Switch disabled with 'Available in release builds' hint; release builds render it enabled and live
- [ ] #4 pnpm tsc --noEmit clean
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review as Won't Do. Parent TASK-54 closed in favor of TASK-60.
<!-- SECTION:FINAL_SUMMARY:END -->
