---
id: TASK-63.2
title: 'Plan Task 2: GGUF model download'
status: To Do
assignee: []
created_date: '2026-04-30 22:26'
labels:
  - 63-impl
dependencies: []
parent_task_id: TASK-63
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 ensure_gguf_present(variant) is async and returns the on-disk path
- [ ] #2 Download progress flows into existing dictation snapshot's download_bytes_done/download_bytes_total fields
- [ ] #3 Existing-file fast path skips download when SHA-256 matches
- [ ] #4 Hardcoded catalog covers qwen3.5-0.8b-q4 and qwen3.5-2b-q4 with documented source URLs
- [ ] #5 Manual: deleting cached file and triggering cleanup dictation produces a download with progress visible in UI
<!-- AC:END -->
