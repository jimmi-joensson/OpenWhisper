---
id: TASK-13
title: License attribution bundle
status: To Do
assignee: []
created_date: '2026-04-22 21:26'
labels:
  - compliance
  - release
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Aggregate all bundled third-party license notices into a single LICENSES.md that ships with the app, and surface attribution in the About tab of settings. Required by CC-BY-4.0 (NVIDIA / Parakeet weights) and best practice for Apache-2.0 deps (FluidAudio, any Rust crates).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 LICENSES.md assembled covering NVIDIA Parakeet (CC-BY-4.0), FluidAudio (Apache-2.0), Apple CoreML, and all Rust crate licenses
- [ ] #2 About tab in settings displays NVIDIA Parakeet CC-BY-4.0 attribution prominently
- [ ] #3 Any Apache-2.0 NOTICE files included verbatim
- [ ] #4 CI lint or script verifies LICENSES.md is regenerated when dependencies change
<!-- AC:END -->
