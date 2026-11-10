---
id: TASK-93.11
title: 'Plan Task 11: Model and effort drop-downs, token and plan usage in diagnostics'
status: In Review
assignee: []
created_date: '2026-11-10T11:16:00Z'
labels:
  - 93-impl
dependencies: []
parent_task_id: TASK-93
ordinal: 75000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Claude model and effort are drop-downs offering every model alias and effort level the CLI takes, plus a free model name
- [ ] #2 Each run stores tokens, list price and the plan's 5-hour and 7-day window readings
- [ ] #3 Diagnostics shows usage per dictation, where the plan stands, and estimates that state their limits
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
250 core tests, 163 Playwright; Windows cross-check clean. Live on a Claude subscription: Opus low 1.45 s, Fable low 2.98 s, Sonnet xhigh 1.42 s, Sonnet max 2.29 s, Haiku max 1.17 s (single cold connection tests). One dictation with Sonnet: about 1,680 tokens in (1,676 cached), 28 out, list price 0.00045 USD. Awaiting user QA in the Dev app.
<!-- SECTION:NOTES:END -->
