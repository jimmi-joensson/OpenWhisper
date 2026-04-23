---
id: TASK-15
title: Swap Parakeet v2 → v3 multilingual model
status: Done
assignee: []
created_date: '2026-04-23 09:10'
updated_date: '2026-04-23 18:30'
labels:
  - macos
  - stt
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Upgrade the default STT model from Parakeet-TDT-0.6b-v2 (English only) to v3 (25-language multilingual with built-in language auto-detection). Driven by user requirement: dictation should seamlessly handle Danish, English, and other European languages without any manual language setting. v3 is a drop-in swap — same ANE runtime, same ~500 MB footprint, same CC-BY-4.0 licensing — and ships pre-converted by FluidInference. FluidAudio's AsrModelVersion enum already exposes .v3 and defaults to it.

Per-utterance auto-detect works reliably (single-language recordings transcribe in the matching language, no translation). Upstream limitation: intra-utterance code-switching may produce errors — acceptable for MVP; revisit only if real-world friction emerges.

Acceptance criteria:
- [ ] DictationService loads AsrModels with version: .v3
- [ ] models/README.md lists v3 as default (languages, code-switching caveat)
- [ ] Smoke test: Danish utterance transcribes to Danish text (no auto-translate to English)
- [ ] Smoke test: English utterance still transcribes to English text
- [ ] First-run download works (fresh Application Support/OpenWhisper/models/)
- [ ] App builds and existing EN dictation behavior is preserved
<!-- SECTION:DESCRIPTION:END -->
