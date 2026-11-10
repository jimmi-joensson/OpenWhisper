---
id: TASK-102
title: Only one OpenWhisper build runs at a time
status: In Review
assignee: []
created_date: '2026-11-10T15:52:00Z'
updated_date: '2026-11-10T15:53:00Z'
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
- [x] #1 Starting the Dev build while production runs closes production
- [x] #2 Starting production while the Dev build runs closes the Dev build
- [x] #3 A build with an unknown identifier closes nothing
- [ ] #4 Startup is not delayed while the other build quits
- [ ] #5 Windows path compiles; run on a Windows machine before leaving In Review
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
apps/tauri/src-tauri/src/sibling.rs, called at the start of setup on its own thread.
Tried on macOS with both builds installed (2026-11-10): starting the Dev build closed production within 2 s; opening production closed the Dev build within 1 s. Unit tests cover the identifier pairing and check the constants against both Tauri configs.
AC 4 holds by construction (own thread) but was not measured. AC 5: compiles for x86_64-pc-windows-gnu, not run. On Windows the other build is ended with TerminateProcess, which is abrupt; a recording in progress in that build is lost.
<!-- SECTION:NOTES:END -->
