---
id: TASK-16
title: Multilingual filler register + lang-aware stripping
status: To Do
assignee: []
created_date: '2026-04-23 09:48'
labels:
  - macos
  - post-processing
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Refactor TranscriptProcessor's flat fillers set into a per-language register keyed by FillerLang enum (.en, .da). Add a cheap heuristic for auto-detection (Danish-specific chars æ/ø/å ⇒ .da, else .en) so the right set is applied without waiting for FluidAudio to expose detected language (upstream issue #303).

Root cause fixed: 'er' was on the flat filler list, which silently stripped the Danish copula 'er' (is/are) from every DA transcript. Observed live during TASK-15 smoke tests — 'det er fedt' became 'det fedt'. Per-lang register keeps 'er' in .en only; 'erm/err/errm' stay in both because they're never Danish words.

Future: when FluidAudio issue #303 exposes the detected language per transcription result, TranscriptProcessor.process() already accepts a lang parameter — DictationService can forward it and drop the heuristic.

Acceptance criteria:
- [ ] Flat defaultFillers replaced with defaultFillersByLang keyed by FillerLang
- [ ] detectLang(_:) heuristic ships (æ/ø/å check)
- [ ] Danish 'er' survives processing in DA-detected text (e.g. 'det er fedt' → 'det er fedt')
- [ ] English 'er' still stripped ('yeah er this works' → 'yeah this works')
- [ ] 'øh/øhm' still stripped in DA
- [ ] App builds and existing EN dictation behavior is preserved
- [ ] process() accepts optional lang hint for future FluidAudio integration
<!-- SECTION:DESCRIPTION:END -->
