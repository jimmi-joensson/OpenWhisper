---
id: TASK-19
title: Local LLM transformation pass (caveman full/ultra + retskrivning)
status: To Do
assignee: []
created_date: '2026-04-23 10:09'
labels:
  - macos
  - post-processing
  - caveman
  - local-llm
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend OpenWhisper with a small local LLM that powers the 'full' and 'ultra' caveman compression modes AND can host the retskrivning (spelling/grammar repair) feature from TASK-17 without any cloud dependency. Fully offline, BYO-RAM instead of BYO-key. Complements TASK-18 (rule-based lite) — this task unlocks modes that rules cannot achieve (restructuring, semantic synonyms, cross-lingual repair).

Why local, not cloud: user principle is that cloud LLMs to save downstream LLM tokens is economic inversion. Local model = zero marginal cost per transcription. Opt-in disk + RAM cost only for users who enable it.

Model evaluation shortlist (pick one in spike, document rejection reasons for others):
- Gemma 3 1B (Q4) — ~0.7 GB disk, ~1 GB RAM, Apache-2.0, decent multilingual incl. Danish
- Qwen 3 0.6B (Q4) — ~0.4 GB disk, ~0.8 GB RAM, Apache-2.0, surprisingly strong for size
- Llama 3.2 1B (Q4) — ~0.7 GB disk, Llama license, weaker DA
- Phi-3.5-mini 3.8B (Q4) — ~2.2 GB, MIT, too big for default
- Apple Foundation Models — free, ANE-accelerated, but macOS 26+ only (wait-and-see)

Runtime: MLX Swift (github.com/ml-explore/mlx-swift-examples). Native Swift package, Metal-accelerated, in-process (no Ollama daemon). Reject llama.cpp (C++ binding overhead) and Ollama (requires separate install, bad packaged-app UX).

Enabled modes (user picks one active at a time, cycles via existing TASK-18 hotkey):
- 🪨 caveman-full — 50-60% compression, rewrites prose into terse fragments, preserves code/URLs/negations
- 🔥 caveman-ultra — 60-75% compression, telegraphic style, aggressive synonym substitution
- ✏️ retskrivning — grammar/spelling repair only, no compression (fulfills TASK-17 locally)
- Future: 📜 wenyan (classical Chinese), per-domain custom prompts, etc.

Prompt design (critical for quality):
- Strict system prompt with guardrails: preserve negations, code, URLs, paths, numbers, proper nouns, intentional code-switching
- User prompt includes detected language hint from TranscriptProcessor
- Output format: 'return corrected/compressed text only, no explanation, no quotes'
- Temperature 0.0 for determinism
- Steal guardrails from caveman-compress/SKILL.md verbatim

Integration:
- Opt-in download on first activation (pill message: 'downloading compression model ~700 MB')
- Model stored under ~/Library/Application Support/OpenWhisper/models/mlx/
- Same lifecycle as Parakeet (download, verify, load, unload on idle)
- Runs async in existing dictation pipeline after TranscriptProcessor, before injector
- Timeout guard (2 s) — on timeout/error, fall back to rule-cleaned text from TASK-18 and paste anyway
- RAM: model stays loaded while compression mode != off; unloaded when user switches to off
- Disk: user can delete model from Settings → Models tab (per TASK-11)

UX:
- Settings → Compression shows: Off | Lite (rules) | Full (local LLM) | Ultra (local LLM) | Retskrivning (local LLM)
- Cost display: disk MB used, current RAM MB, model name, download size
- Model picker (advanced): switch between Gemma / Qwen / Llama once multiple are supported

Risks and open questions:
- Danish compression/repair quality of 1B models is unproven — spike needed with real DA transcripts from TASK-15/16 smoke tests
- Adds ~1 GB runtime RAM. On 8 GB Macs + Parakeet loaded (~66 MB) + Xcode/Safari this gets tight. Default mode must stay off.
- Model upgrade cadence policy: when Gemma 4 ships, do we force-upgrade users or pin a version? Pin + offer upgrade in settings.
- Legal: Gemma Apache-2.0 is clean. Qwen Apache-2.0 clean. Llama needs license-file inclusion in About tab. Prefer Apache-2.0 models.

Acceptance criteria:
- [ ] MLX Swift added as SPM dependency
- [ ] Model evaluation spike documented under docs/spikes/ with recommendation
- [ ] Chosen model downloads on first activation, verifies SHA, loads
- [ ] At least one mode works end-to-end (caveman-full recommended as primary proof)
- [ ] Negation guardrails validated: adversarial tests that try to flip meaning fail to do so
- [ ] Latency <500ms on M-series Mac for 1-2 sentence input
- [ ] RAM returns to baseline when mode switched to off (model unloaded)
- [ ] Timeout fallback to TASK-18 rule output works
- [ ] Settings tab surfaces model info + delete button

Dependencies:
- Blocked by TASK-18 (shares hotkey cycle + pill indicator scaffolding)
- Related to TASK-17 (supersedes the cloud retskrivning idea once this ships — consider deprecating TASK-17 then)
- Related to TASK-11 (Settings window needs a Compression tab)
<!-- SECTION:DESCRIPTION:END -->
