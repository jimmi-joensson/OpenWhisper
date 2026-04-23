---
id: TASK-2
title: Rust core crate scaffold
status: Done
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-23 18:16'
labels:
  - core
  - setup
dependencies: []
priority: high
ordinal: 7000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Cargo workspace at repo root with openwhisper-core crate. Exposes C ABI for Swift (via swift-bridge later) and will host audio capture, VAD, config, post-processing, cloud provider clients.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 cargo build succeeds from workspace root
- [x] #2 Crate exposes a single hello-world C ABI symbol callable from Swift as smoke test
- [x] #3 cbindgen or swift-bridge chosen and configured
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Cargo workspace at repo root. openwhisper-core crate builds as staticlib + rlib. swift-bridge 0.1.59 chosen over cbindgen for ergonomic Swift interop (handles RustString/RustStr wrappers automatically). build.rs emits generated Swift + C headers directly into apps/macos/Generated/ so the Xcode target consumes them without a manual copy step.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rust workspace + core crate + swift-bridge pipeline all working end-to-end. Smoke test: Swift calls hello_from_rust() and core_version() through the C ABI; symbols verified linked in debug dylib.
<!-- SECTION:FINAL_SUMMARY:END -->
