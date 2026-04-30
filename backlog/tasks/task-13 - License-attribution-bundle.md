---
id: TASK-13
title: License attribution bundle
status: Won't Do
assignee: []
created_date: '2026-04-22 21:26'
updated_date: '2026-04-30 16:35'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review as Won't Do. Post-v0.4.0 priorities reset; CC-BY-4.0 attribution still required and will be re-planned from current state if/when revisited (likely as part of an About-pane / LICENSES.md task once we have a Settings → About surface to host it).
<!-- SECTION:FINAL_SUMMARY:END -->
