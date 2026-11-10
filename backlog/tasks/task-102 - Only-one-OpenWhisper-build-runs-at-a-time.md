---
id: TASK-102
title: Only one OpenWhisper build runs at a time
status: In Progress
assignee: []
created_date: '2026-11-10T15:52:00Z'
labels:
  - tauri
  - dev-workflow
dependencies: []
priority: medium
ordinal: 93000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The production app and the Dev build have different identifiers, so the single-instance plugin does not keep them apart. Run side by side they both answer the same hotkey and every dictation is recorded, pasted and saved twice. On the author's machine they also share one dictionary and one history folder.

Asked for on 2026-11-10: when one build opens, the other closes. The newest launch wins. macOS: the other build is found by bundle identifier and asked to quit, then force-quit if it does not. Windows: any other process with our exe file name is ended (same-build second copies are already stopped by the single-instance plugin).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Starting the Dev build while production runs closes production
- [ ] #2 Starting production while the Dev build runs closes the Dev build
- [ ] #3 A build with an unknown identifier closes nothing
- [ ] #4 Startup is not delayed while the other build quits
- [ ] #5 Windows path compiles; run on a Windows machine before leaving In Review
<!-- AC:END -->
