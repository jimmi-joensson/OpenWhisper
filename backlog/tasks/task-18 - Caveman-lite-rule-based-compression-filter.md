---
id: TASK-18
title: Caveman-lite rule-based compression filter
status: Won't Do
assignee: []
created_date: '2026-04-23 10:09'
updated_date: '2026-04-30 16:35'
labels:
  - macos
  - post-processing
  - caveman
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add an opt-in rule-based compression stage after TranscriptProcessor and before text injection. Reduces token cost when dictating into LLM chats by stripping filler, hedging, and verbose phrasing without changing meaning. Fully local — no model dependency, no network.

Motivation: the user dictates verbosely (natural spoken style adds 30%+ low-signal words vs. typed). Those tokens get sent to downstream Claude/GPT chats and add cost. A local rule pass trims without compromising substance.

Scope covers only the 'lite' intensity level (20-30% compression, english-first). 'full' and 'ultra' require a local LLM and are scoped out here — see TASK-19.

Transforms (ordered):
1. Drop articles ('a', 'an', 'the') as standalone tokens
2. Drop filler prose ('just', 'really', 'basically', 'actually', 'simply', 'essentially', 'generally', 'kind of', 'sort of')
3. Drop hedging ('I think', 'I feel like', 'probably', 'perhaps', 'it might be', 'you could consider', 'it would be good to')
4. Drop pleasantries ('please', 'thanks', 'thank you' — configurable, some users want these)
5. Common verbosity substitutions ('in order to' → 'to', 'make sure to' → 'ensure', 'the reason is because' → 'because', 'utilize' → 'use', 'due to the fact that' → 'because')
6. Connective fluff ('however', 'furthermore', 'additionally', 'in addition' — optional, may change tone)

Hard never-drop safe-list (catastrophic if dropped):
- Negations: not, n't, never, no, don't, doesn't, cannot, without, except, unless, neither, nor
- Quantifiers in specific positions: all, every, any, some, none
- Proper nouns, code, URLs, paths (already protected by substitution regex)
- Numbers, dates, versions

UX:
- Settings toggle: Compression → off | lite
- Pill icon 🪶 when lite is active, blank when off
- Dedicated global hotkey to cycle modes (default ⌃⌥C) — configurable later
- Hotkey press flashes new icon briefly in pill
- Mode persisted to UserDefaults

Language behavior:
- Reuse TranscriptProcessor.detectLang. If .da detected, skip lite (Danish has no articles, most rules don't port cleanly). Document as EN-optimized in settings help text. DA compression ships with TASK-19 (local LLM).

Acceptance criteria:
- [ ] CompressionFilter struct separate from TranscriptProcessor, pure function, testable
- [ ] Unit tests: negation preserved ('don't delete the users table' → 'don't delete users table'), articles dropped, filler stripped, substitutions applied, safe-list respected
- [ ] DictationService pipes through CompressionFilter after TranscriptProcessor when mode != off
- [ ] Pill shows 🪶 when active
- [ ] Global hotkey cycles off ↔ lite (extensible to more modes when TASK-19 lands)
- [ ] DA utterances bypass compression (document why)
- [ ] Setting persists across launches
- [ ] Off by default; zero behavior change unless user opts in
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review as Won't Do. Post-v0.4.0 priorities reset; rule-based caveman compression will be re-planned from current state if/when revisited.
<!-- SECTION:FINAL_SUMMARY:END -->
