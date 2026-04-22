---
id: TASK-3
title: Parakeet CoreML conversion pipeline
status: To Do
assignee: []
created_date: '2026-04-22 21:11'
labels:
  - model
  - macos
dependencies: []
references:
  - 'https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2'
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Convert nvidia/parakeet-tdt-0.6b-v2 weights from NeMo/PyTorch to CoreML mlpackage for Apple Neural Engine execution. Script lives in models/ and is reproducible.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Script downloads NeMo checkpoint and produces .mlpackage artifact
- [ ] #2 CoreML artifact runs on ANE on Apple silicon (verified via Instruments)
- [ ] #3 WER on a sample clip matches NeMo reference within tolerance
- [ ] #4 CC-BY-4.0 attribution string bundled with artifact
<!-- AC:END -->
