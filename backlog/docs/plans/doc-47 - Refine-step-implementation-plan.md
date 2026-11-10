---
id: doc-47
title: Refine step - implementation plan
type: other
created_date: '2026-11-10T09:13:00Z'
---


**Backlog parent:** TASK-93 (Refine step - cloud LLM cleanup between dictionary and insert)
**Spec:** none as a separate file. The design decisions came from a planning session and are recorded in TASK-93's description and in "Design decisions" below.
**Loose visual reference:** design canvas tab "12 - Models" (pipeline idea and bridge config with connection test only; the local model sheets and memory bar are out).

## Where it hooks in (verified in code, 2026-11-10)

- Dictionary correction: `core/src/transcript.rs` `process_tracked` (fillers, dictionary, dedupe, whitespace, trailing space).
- Caller: `core/src/dictation.rs` `run_stop_pipeline`.
- Handoff: `dictation_deliver_transcript` sets the snapshot transcript (UI reads it via the `dictation_tick` emitter in `apps/tauri/src-tauri/src/lib.rs`), calls the registered `Injector` (`apps/tauri/src-tauri/src/injection/mod.rs`: clipboard set, synthesized paste, clipboard restore), then writes stats.
- Refine runs in `run_stop_pipeline` between `process_tracked` and `dictation_deliver_transcript`. Nothing downstream changes.

## Design decisions

1. **Interface.** `core::refine::Refiner`: text in, text out, plus optional `warm` / `discard` so a backend can overlap its startup with the recording. A later cloud transcriber gets its own trait of the same shape next to `recognizer`; nothing in Refine assumes local STT.
2. **Claude CLI backend.** Spawns the user's own `claude` binary. Flags verified against Claude Code 2.1.322: `-p --model haiku --effort low --safe-mode --tools "" --strict-mcp-config --disable-slash-commands --no-session-persistence --input-format stream-json --output-format stream-json --verbose --system-prompt <composed>`. `--bare` is not usable (it never reads the subscription login). Login probe: `claude auth status --json` (`loggedIn`, 0.14 s).
3. **Warm strategy.** One process per dictation, spawned at record start, fed at stop, killed on cancel or when Refine is skipped. Measured 0.7-0.8 s send-to-result versus 2.8-5.6 s cold. No conversation is shared across dictations.
4. **OpenAI-compatible backend.** `POST {base_url}/chat/completions` through `ureq`. URL and model in `settings.json`; key in the OS keychain (`keyring` crate), never in a file. `OPENWHISPER_REFINE_API_KEY` overrides for headless use.
5. **Prompt.** One system prompt (fixed guard rules + user instructions + dictionary terms) and one user message carrying the transcript inside `<transcript>` tags. One call per dictation.
6. **Fallback.** Timeout (default 5 s), backend error, missing backend, empty output, output far longer or shorter than the input, or output that shares too few words with the input all deliver the unrefined text. The reason is exposed on the snapshot.
7. **Skip.** Fewer than 4 words: no call.
8. **After the model.** The refined text goes through the dictionary matcher once more and gets the same whitespace normalization and trailing space as the rule pass.
9. **Raw transcript.** Kept in memory on `DictationSnapshot` (`raw_transcript`) and shown in the UI. Not persisted in v1.
10. **Feature flag.** Cargo feature `refine` on core (pulled in by `tauri`, enabled by the CLI) so the SwiftUI staticlib flavor does not grow HTTP or keychain deps.
11. **Not in v1.** File transcription (TASK-91 path) is not refined. Codex and Gemini CLIs are not backends yet. No streaming.

## Task 1: Core refine module - trait, prompt, orchestration, settings

Files: `core/src/refine/{mod,prompt,guard}.rs` (new), `core/src/settings/refine.rs` (new), `core/src/settings/mod.rs`, `core/src/dictionary.rs` (expose current terms), `core/src/transcript.rs` (post-refine finalize), `core/Cargo.toml`, `core/src/prelude.rs`.

Verification: `cargo test -p openwhisper-core --no-default-features --features tauri refine`.

Outcome ACs:
- `Refiner` trait and `RefineOutcome` (text, raw, status, detail, elapsed_ms) are public and re-exported from the prelude
- Composed prompt contains the guard rules, the user instructions (or the conservative default), and the dictionary terms; the transcript only appears in the user message
- Orchestrator returns the unrefined text on timeout, error, unavailable backend and guard rejection, each with a distinct status
- Utterances under the minimum word count never reach a backend
- `refine` block round-trips through `settings.json` without clobbering sibling blocks; default is disabled

## Task 2: Claude CLI backend with warm process

Files: `core/src/refine/claude_cli.rs` (new).

Verification: unit tests against a fake `claude` script (success, slow, error exit, logged out); live run through the CLI in Task 5.

Outcome ACs:
- Binary is found in the usual install locations and PATH, or via the settings override; not-installed and not-signed-in are reported as distinct states
- A warm process spawned at record start answers a later request; a discarded one is killed and leaves no child behind
- A cold call works when no warm process exists
- Timeout kills the child and returns a timeout error

## Task 3: OpenAI-compatible backend and keychain

Files: `core/src/refine/{openai_compat,secrets}.rs` (new).

Verification: unit tests against a local mock HTTP server (success, 401, slow, malformed body).

Outcome ACs:
- Request carries model, system and user messages and the bearer key; response text is extracted from `choices[0].message.content`
- Key is read from and written to the OS keychain; `settings.json` never contains it
- HTTP errors and timeouts surface as backend errors with a readable message

## Task 4: Wire into the dictation pipeline

Files: `core/src/dictation.rs`, `apps/tauri/src-tauri/src/lib.rs` (tick payload).

Verification: dictation unit tests with a stub refiner; cargo check for the Tauri crate.

Outcome ACs:
- With Refine off, `run_stop_pipeline` output and timing are unchanged and no process or request is started
- With Refine on, the delivered text is the refined text and the snapshot carries `raw_transcript`, refine status and detail
- Record start warms the backend; cancel and empty recordings discard it
- Status message reads "refining…" while the call is in flight

## Task 5: CLI surface

Files: `cli/src/commands/refine.rs` (new), `cli/src/main.rs`, `cli/src/commands/mod.rs`, `cli/Cargo.toml`.

Verification: `cargo run -p openwhisper-cli -- refine …` by hand, plus CLI smoke test for `refine status --json`.

Outcome ACs:
- `openwhisper refine run` refines text from an argument or stdin and prints text or a JSON outcome
- `openwhisper refine status` reports settings and backend availability
- `openwhisper refine test` runs the connection test the UI uses
- `openwhisper refine config` reads and writes the settings block; `set-key` / `clear-key` manage the keychain entry
- `openwhisper refine eval` runs the live eval set and reports pass/fail and latency per case

## Task 6: Tauri commands and Refine settings pane

Files: `apps/tauri/src-tauri/src/settings/mod.rs`, `apps/tauri/src-tauri/src/lib.rs`, `apps/tauri/src/components/refine-pane.tsx` (new), `apps/tauri/src/lib/use-refine.ts` (new), `apps/tauri/src/lib/settings-panes.ts`, `Settings.tsx`, Home transcript row.

Verification: `pnpm tsc --noEmit`, `pnpm test:ui` with a new `refine.spec.ts`.

Outcome ACs:
- Refine pane has the on/off switch (off by default), backend picker, instructions field with reset, and a test-connection button showing success with latency or the failure reason
- The pane states in plain words where the text is sent for the selected backend
- Claude CLI not installed and not signed in each show a clear message with what to do
- API key field writes to the keychain and never echoes the stored key
- Home shows when the last transcript was refined or fell back, and gives access to the unrefined text
- Playwright spec covers the pane and the Home marker

## Task 7: Live evaluation, real dictation, terms check

Verification: `openwhisper refine eval` against both backends where credentials exist; dictation in the dev app; written findings in the task notes.

Outcome ACs:
- Eval set covers: command-like dictation, a question, Danish, mixed Danish/English, dictionary names, short-utterance skip; results recorded
- Measured latency per backend recorded in TASK-93 notes
- Anthropic terms on subscription use from a third-party app checked and summarized in TASK-93 notes
- Anything unverified (Windows, OpenAI endpoint without a key, Codex/Gemini) is listed as open

## Sequencing

Task 1 first. Tasks 2 and 3 are independent of each other. Task 4 needs 1 (and 2 to be useful). Task 5 needs 1-3. Task 6 needs 4 and 5's core functions. Task 7 last.
