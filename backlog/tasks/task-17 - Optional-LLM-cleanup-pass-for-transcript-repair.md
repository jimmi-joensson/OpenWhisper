---
id: TASK-17
title: Optional LLM cleanup pass for transcript repair
status: To Do
assignee: []
created_date: '2026-04-23 09:52'
labels:
  - macos
  - post-processing
  - byo-cloud
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add an opt-in post-processing stage that sends the cleaned transcript through a user-configured LLM (OpenAI, Anthropic, Google, or local via Ollama) for grammar/spelling repair. Fits the 'free local core + BYO cloud keys' model — zero cost by default, power users enable it with their own key for better quality on hard languages.

Motivation surfaced during TASK-15/16 Danish smoke tests. Parakeet v3 on Danish has systemic issues that rule-based post-processing cannot fix:
- Mangled segmentations ('Men det også det der flest. Gang sker ikke' — words jumbled across clause boundaries)
- Close-phonetic mis-hearings ('sindssygt' → 'sindssynt', 'besværligt' → 'beskider')
- Grammar gaps where copula survives but other function words drop
- Same problems exist for English at lesser severity

Rule-based cleanup (TranscriptProcessor + TASK-10 custom vocab) handles literal substitutions and filler removal but cannot reason about sentence structure. An LLM with a tight 'repair only, do not rewrite' prompt can fix these without hallucinating.

Scope:
- Settings toggle: 'Use AI to improve transcripts (requires API key)'
- Provider picker: OpenAI / Anthropic / Ollama (local) — extensible later
- API key stored in Keychain per provider
- Model picker per provider (default to cheapest reasonable: gpt-4o-mini / claude-haiku-4-5 / llama3.1:8b)
- Strict prompt: 'Correct spelling and grammar in the transcript below. Preserve meaning, preserve intentional code-switching, do not add content, do not change tone. Return only the corrected text.'
- Called after TranscriptProcessor.process() in DictationService, before injector.inject()
- Pipe the detected language (TranscriptProcessor.detectLang result) into the prompt as a hint
- Timeout guard (2-3 s max) — on timeout, fall back to the rule-cleaned text so paste isn't blocked
- Surface cost estimate in settings (cents per minute of dictation)
- Latency note in UI: expect +0.5–1.5 s added before paste

Acceptance criteria:
- [ ] Feature is opt-in, default off; zero network traffic when disabled
- [ ] API keys stored in Keychain, never in plist/UserDefaults
- [ ] At least one provider (OpenAI) works end-to-end
- [ ] Timeout/error falls back to rule-cleaned text with an inline warning in the pill/status
- [ ] Prompt preserves code-switching (DA+EN mix doesn't get homogenized)
- [ ] Danish smoke test: 'Det er fedt, yeah this works' round-trips unchanged; 'sindssynt' → 'sindssygt'; 'beskider' → plausible repair

Out of scope (future tasks):
- Streaming LLM output to reduce perceived latency
- Local model auto-download/manage (separate task)
- Per-domain custom prompts (coding vs. emailing vs. chatting)
<!-- SECTION:DESCRIPTION:END -->
