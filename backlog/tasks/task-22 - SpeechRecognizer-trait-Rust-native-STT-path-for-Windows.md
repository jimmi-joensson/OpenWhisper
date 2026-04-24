---
id: TASK-22
title: SpeechRecognizer trait + Rust-native STT path for Windows
status: To Do
assignee: []
created_date: '2026-04-24 06:07'
labels:
  - rust-core
  - stt
  - windows
dependencies:
  - TASK-21
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Abstract STT behind a Rust trait so the Windows shell can call transcribe without implementing engine integration itself. Design shape is deferred until TASK-21 surfaces what sherpa-onnx-rs (or chosen crate) actually returns — trait design backwards from reality, not forward from speculation. Mac continues using the host-push pattern (Swift owns FluidAudio, calls dictation_deliver_transcript) because FluidAudio is Swift-only and the async-over-sync FFI bridge adds complexity for zero user benefit. Both paths converge on the same dictation state transitions in core/src/dictation.rs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 trait SpeechRecognizer defined in core/src/stt.rs with sync transcribe(samples, sample_rate) -> Transcript
- [ ] #2 Transcript exposes text + confidence + optional language tag
- [ ] #3 sherpa-onnx impl lives under a cargo feature (don't pull ORT on Mac builds)
- [ ] #4 Rust exposes dictation_transcribe_pending or equivalent so Win shell doesn't need to reimplement sample-plumbing
- [ ] #5 Mac build is untouched — existing host-push path continues to work without recompile-time changes
- [ ] #6 Unit test: trait can be mocked, dictation flow covers the whole pending-samples → delivered-transcript cycle
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Context: after migrating DictationService phase machine into Rust (early April 2026), the Mac shell works end-to-end in host-push mode. Windows port is the forcing function for the trait. See feedback_rust_core_orchestration memory for why orchestration lives in Rust.
<!-- SECTION:NOTES:END -->
