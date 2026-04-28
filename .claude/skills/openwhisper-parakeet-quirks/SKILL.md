---
name: openwhisper-parakeet-quirks
description: Known Parakeet-TDT transcription quirks — what looks like a bug but is actually a model-level token behavior. READ before chasing a transcription quality issue through `core/src/audio.rs`, the FFI boundary, or the model-loading path. The right fix for items in this list is post-processing (custom vocab, DA rules) under the custom-vocab task in `backlog/` (currently `task-10`), not engine swaps or routing investigation.
---

# Parakeet transcription quirks

These are model-level behaviors that have been observed and accepted. Don't misdiagnose them as routing bugs, FFI issues, or load-order problems. Don't try to fix them by swapping models — v3 has its own analog quirks; v2 swaps don't help with v3 issues.

## v2 (English default)

- **Splits novel compound brand names.** "OpenWhisper" → "Open Whisper". The tokenizer has no prior on uncommon compounds. Fix: custom vocab post-processing (custom-vocab task in `backlog/`).
- **Phonetic ambiguity on word endings, high confidence not a tell.** Synthesized "Engine" → "Engineer" at 0.96 confidence. High confidence ≠ correct on near-neighbors. Fix: custom-vocab task.
- **Capitalization and punctuation normalized.** "This"/"this" can differ from source; periods sometimes turn into commas. This is the model's own punctuation prediction, not a post-processing bug.
- **Drops low-prominence function words** ("a", "the") — TDT token model struggles with low-acoustic-prominence words.

## v3 (multilingual, opt-in)

- **Per-utterance auto-detect works** — DA → DA, EN → EN, no translation.
- **Intra-utterance EN↔DA code-switching is unreliable.** Treat as best-effort; don't design UX around it.
- **Drops the unstressed copula "er"** in fast Danish speech ("det er helt fint" → "det helt fint"). Same family as v2's a/the drops. Fix: DA rule under custom-vocab task — insert "er" in [pronoun + adj/noun] context.
- **Mis-hears close-phonetic Danish words.** Same root cause; same fix.

## How to apply

When the user reports a transcription quirk:

1. Check this list first. If it matches → the work goes to the custom-vocab task in `backlog/` (custom vocab / DA rules), not to `core/src/audio.rs` or the recognizer boundary.
2. If it doesn't match, then investigate routing/load/FFI.
3. Don't propose tuning sherpa decoding params (beam search, `max_active_paths`, etc.) for these issues — they're post-tokenization behaviors that decoder tweaks won't move.
4. Don't propose model swaps — v2 ↔ v3 trades one set of quirks for another, doesn't eliminate the category.

## Caveat — RDP audio confounds Danish testing

If the user is reporting Danish quality issues from a remote-desktop session, RDP audio compression can produce DA-specific degradations that *aren't* model quirks. Ask whether the test was on physical hardware before applying this list. (Details in machine-local memory; not part of this repo's truth.)
