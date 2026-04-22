---
id: TASK-2
title: Rust core crate scaffold
status: To Do
assignee: []
created_date: '2026-04-22 21:11'
labels:
  - core
  - setup
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Cargo workspace at repo root with openwhisper-core crate. Exposes C ABI for Swift (via swift-bridge later) and will host audio capture, VAD, config, post-processing, cloud provider clients.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 cargo build succeeds from workspace root
- [ ] #2 Crate exposes a single hello-world C ABI symbol callable from Swift as smoke test
- [ ] #3 cbindgen or swift-bridge chosen and configured
<!-- AC:END -->
