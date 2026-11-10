---
id: TASK-93.7
title: 'Plan Task 7: Live evaluation, real dictation, terms check'
status: In Review
assignee: []
created_date: '2026-11-10T09:14:00Z'
updated_date: '2026-11-10T09:48:00Z'
labels:
  - 93-impl
dependencies: []
parent_task_id: TASK-93
ordinal: 71000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Eval set covers command-like dictation, a question, Danish, mixed DA/EN, dictionary names, short-utterance skip; results recorded
- [x] #2 Measured latency per backend recorded in TASK-93 notes
- [x] #3 Anthropic terms on subscription use from a third-party app checked and summarized in TASK-93 notes
- [x] #4 Unverified items are listed as open
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Live eval, Claude CLI 2.1.322 + Haiku 5.5, Claude subscription, macOS, 2026-11-10 (openwhisper refine eval):
- Before the two prompt rules of : cold 12/12, warmed 11/12 (one missing question mark).
- After: warmed 3 runs 11/13, 11/13, 11/13; no run obeyed a command, answered a question, followed the injection, translated Danish or changed a dictionary name.
- Recurring misses: (1) request-to-assistant-en comes back unchanged (no capitals/punctuation) in about 2 of 3 runs: harmless, under-cleaned. (2) garbled-no-guessing: the model alters a garbled word. Before the rule it invented a different word (kigget pa, kort) in 2 of 4 tries; after, it only changes a letter of the garble. Guessing at garbles is the remaining meaning-drift risk and the guard cannot catch it.

Latency (send to answer): warmed 2.5 s before sending: median 0.83-0.90 s, min 0.70, max 1.6 (one outlier 2.9). Cold: median 1.19-1.20 s, min 1.05, max 2.3. OpenAI-compatible: not measured (no endpoint).

Audio end to end (macOS say -> openwhisper transcribe -> refine run), 2 English + 2 Danish clips: commands and the question came back as sentences, Danish stayed Danish, Jepp/haeppe -> Jeppe, payprof -> PayProff, Emin Oil/Emanuel -> Emanoil. NOT done: dictating into the running app (needs a person at the mic).

Terms (researched 2026-11-10, sources in the session report): running your own signed-in claude -p from your own app on your own machine is documented and currently draws on subscription limits. Public distribution is unclear: the design fits the "unmodified Claude Code binary with their own subscription" carve-out, but Anthropic also says products built for others should use API keys and that it does not allow third parties to offer claude.ai login or rate limits; Anthropic directs such questions to sales. Billing for claude -p is "paused, not settled" (June 2026), and the docs say --bare (no subscription login) will become the -p default in a future release, which would break this backend unless a flag to opt out exists.

Open: Windows run of everything; real OpenAI-compatible endpoint; dictation in the app; Codex CLI (installed, headless codex exec exists, not signed in, untested) and Gemini CLI (not installed) as later backends; file transcription is not refined; no way to cancel a Refine in flight.
<!-- SECTION:NOTES:END -->
