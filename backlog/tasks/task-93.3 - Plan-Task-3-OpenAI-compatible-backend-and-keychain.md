---
id: TASK-93.3
title: 'Plan Task 3: OpenAI-compatible backend and keychain'
status: In Review
assignee: []
created_date: '2026-11-10T09:14:00Z'
updated_date: '2026-11-10T09:48:00Z'
labels:
  - 93-impl
dependencies: []
parent_task_id: TASK-93
ordinal: 67000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Request carries model, system and user messages and bearer key; text extracted from choices[0].message.content
- [x] #2 Key lives in the OS keychain; settings.json never contains it
- [x] #3 HTTP errors and timeouts surface as backend errors with a readable message
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
openai_compat + secrets.
Verified: mock HTTP server unit tests (success, 401, 500, slow, malformed, temperature retry, unreachable); macOS Keychain round trip via openwhisper refine set-key / clear-key, key absent from settings.json.
NOT verified: a real OpenAI-compatible endpoint (no key and no local server on the dev machine); Windows Credential Manager (compiles, not run).
<!-- SECTION:NOTES:END -->
