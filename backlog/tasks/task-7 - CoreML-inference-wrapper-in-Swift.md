---
id: TASK-7
title: CoreML inference wrapper in Swift
status: To Do
assignee: []
created_date: '2026-04-22 21:11'
labels:
  - macos
  - model
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Swift class that loads Parakeet mlpackage and runs inference on PCM buffers. Returns text + timestamps. Prefers ANE compute unit.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Wrapper loads model at app start with progress feedback
- [ ] #2 transcribe(pcm: Data) -> String works end to end
- [ ] #3 MLComputeUnits set to .all (ANE preferred)
- [ ] #4 First-token latency under 300ms on M1
<!-- AC:END -->
