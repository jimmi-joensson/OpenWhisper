---
id: TASK-63
title: LLM-based disfluency cleanup (Qwen 3.5 0.8B via llama-cpp-2)
status: To Do
assignee: []
created_date: '2026-04-30 22:17'
updated_date: '2026-04-30 22:27'
labels: []
dependencies: []
documentation:
  - docs/superpowers/plans/2026-05-01-llm-disfluency-cleanup.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add an opt-in local-LLM post-processing pass on top of the existing rule-based transcript filter (TASK-16/43/44). Uses Qwen 3.5 0.8B Q4_K_M GGUF via llama-cpp-2 with LLGuidance constrained JSON edit-list output to prevent hallucination. Pre-warmed on recording start so cleanup is hot when STT finishes. Depends on the lifecycle foundation parent.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 llama-cpp-2 integrated as cleanup engine; Qwen 3.5 0.8B Q4_K_M GGUF loadable via ModelHandle on Mac (Metal) and Windows (Vulkan/CPU)
- [ ] #2 Cleanup uses LLGuidance/GBNF-constrained JSON edit-list schema (delete-spans only); no free-text rewrite
- [ ] #3 Recording-start pre-warms cleanup model in parallel; cleanup runs after rule-based pass
- [ ] #4 Multilingual support: 25 EU langs covered by base model; primary-language hint passed in prompt
- [ ] #5 Settings: enable/disable cleanup, model variant (0.8B default / 2B opt-in), aggressive-cleanup toggle for soft fillers, primary languages
- [ ] #6 Pill shows placeholder loading state during cold cleanup load (real animation deferred to follow-up task)
- [ ] #7 Playwright covers settings toggles; manual end-to-end smoke on Mac + Windows passes p95 ≤ 1 s for 0.8B
<!-- AC:END -->
