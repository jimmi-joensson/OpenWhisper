# Claude handoff — Windows port context

Context snapshot for Claude instance running on Jimmi's Windows machine. Assumes reader has already scanned `README.md`, `INSTALL.md`, and `backlog/` (task tracking lives there via the Backlog.md CLI — `backlog board`, `backlog task list`). This doc covers what **isn't** in the repo: project intent, values, cross-platform architecture decisions, and Windows-specific targets.

## Project intent

OpenWhisper = open-source alternative to Superwhisper (macOS dictation). Strong local transcription is free by default; users BYO API keys for any cloud integrations. MIT license for code; Parakeet weights redistributed under NVIDIA CC-BY-4.0 (attribution required in About + `LICENSES.md`).

**Monetization stance:** Paid tiers only justified for features that cost the project to run (hosted sync, managed billing). Local dictation must never be paywalled. Reject design proposals that gate core local features behind payment.

## Stack — shared Rust core + native UI per OS

Rejected: Electron, Tauri, Flutter, React Native. Native feel per OS is a core product goal.

| OS | UI | Inference |
|---|---|---|
| macOS | Swift + SwiftUI/AppKit | CoreML on Apple Neural Engine (via FluidAudio) |
| **Windows** | **C# + WinUI 3** | **ONNX Runtime + DirectML EP** |
| Linux | Rust + gtk4-rs + libadwaita | ONNX Runtime (CUDA/ROCm/CPU EPs) |

Rust core (`core/` crate) owns: audio capture (cpal), VAD (Silero ONNX or webrtc-vad), config, custom vocab, post-processing, BYO-key cloud provider clients, IPC, **orchestration / phase machines**. Exposes C ABI; FFI via swift-bridge on macOS, **P/Invoke on Windows**, direct link on Linux.

**Inference stays in the platform shell, NOT the core.** CoreML is Swift-native; ONNX .NET bindings on Windows; ONNX Rust bindings on Linux. Keeps FFI boundary clean.

**MVP shipped on macOS first.** Windows port kicked off 2026-04-24. Expect the macOS shell (`apps/macos/`) to be the reference for what the C# shell must mirror.

Model: `nvidia/parakeet-tdt-0.6b-v2` (CC-BY-4.0). Converted NeMo → CoreML for macOS, **NeMo → ONNX for Windows/Linux**. Conversion scripts live in `models/`. Artifacts downloaded at first-run from Hugging Face, not bundled (installer stays small, avoids weight-redistribution friction).

## Architecture rule — orchestration belongs in Rust

The temptation on a new shell is to move only "pure" things to Rust (audio DSP, regex post-processing) and keep orchestration (phase machines, state transitions, status strings, gating logic) in the shell "because it's tied to UI." **Resist.**

If orchestration lives per shell, every semantic decision (canToggle during transcribing, cancel-only-while-recording, "preparing…" vs "loading Parakeet…" status, i18n) gets re-implemented and drifts across Swift, C#, GTK. Solo dev + multi shell = inevitable drift.

Rules:
- Pure computation, no OS deps → Rust (e.g. `transcript::process`).
- State machine + flow coordination driving UI → **still Rust**. Expose as opaque snapshot type + event-style entry points. Shells poll (20 Hz with `Mutex<State>` is trivially cheap) and push events. Phase values as `u32`-encoded enums — simpler than native enum FFI.
- Stays in shell: actual OS APIs (Win32 P/Invoke for hotkey + text injection, WinUI widgets, clipboard, media session), UI widgets, thin observable glue mirroring the Rust snapshot into `INotifyPropertyChanged` (C# equivalent of Swift's `@Observable`).

Accept the FFI overhead. Don't argue "it's just 50 lines of state, duplicating is fine" — status strings, gating rules, error transitions, and edge cases (empty sample drain, cancel timing) accrete nuance.

## Activation UX

**Toggle semantics**, not press-and-hold:
1. First hotkey press → start recording + show pill overlay.
2. User talks.
3. Second press → stop recording → local transcription → inject text into focused input field.

macOS default: Right Command. Windows equivalent TBD — likely Right Ctrl or Right Alt; must be fully rebindable including single modifier keys and double-tap chords. Don't propose press-and-hold or a different metaphor (continuous dictation, wake-word) unless Jimmi reopens the question.

## Parakeet behavior to expect

v2 (English default):
- **Splits novel compound brand names.** "OpenWhisper" → "Open Whisper". Tokenizer has no prior. Fixed by custom vocab post-processing (TASK-10), not by swapping models. Don't misdiagnose this as a model-load or routing bug.
- **Phonetic ambiguity on word endings.** Synthesized "Engine" → "Engineer" with 0.96 confidence. High confidence ≠ correct on near-neighbors.
- Capitalization and punctuation normalized — "This"/"this" can differ from source; periods sometimes → commas.

v3 multilingual (opt-in):
- Per-utterance auto-detect works (DA→DA, EN→EN, no translation).
- Intra-utterance EN↔DA code-switching unreliable — assume best-effort, don't design UX around it.
- Systemic DA: drops unstressed copula "er" in fast speech ("det er helt fint" → "det helt fint"); mis-hears close-phonetic DA words. Family with v2's English "a/the" drops — TDT token model struggles with low-acoustic-prominence function words. Fix via TASK-10 post-processing (DA rule: insert "er" in [pronoun + adj/noun]), not model swap.

## Values — apply before proposing features

**Zero-config over toggles.** Default to auto-detect/seamless behavior. Only add a setting when auto-detection genuinely can't disambiguate, or as a power-user escape hatch. Lead proposals with the auto path, treat settings as fallback. Applies to hotkey defaults, model selection, language detection, output formatting.

**Local-first for cost-saving features.** Any feature whose value is "reduce token cost for the user" must be local-only. Cloud LLM to pre-compress/filter/clean in order to save tokens on another cloud LLM = economic inversion — don't build. Rules first, small local LLM if needed. Cloud is okay only for *capability* features the user explicitly opts into (alternative STT backends), never for cost optimization. Values call, not a technical limitation.

## Dev loop gotcha — build Rust core in release

On the macOS side, `scripts/dev-run.sh` defaults `openwhisper-core` to `PROFILE=release`. The Swift shell stays Debug (breakpoint-friendly).

**Why:** Debug Rust is 50–120× slower than Release on DSP-heavy paths. Concrete case from 2026-04-24: `audio_drain_samples()` blocked 900–2066 ms for 16–38 s of audio in Debug vs **17 ms in Release**. The rubato `SincFixedIn` resampler (`sinc_len=128`, `oversampling_factor=64`) is a debug performance cliff — opt-level=0 kills it. Release core + Debug shell rebuilds are incremental/cached: first cold build ~7 s, subsequent ~1 s.

**Apply to Windows dev loop:** Whatever `scripts/dev-run.*` equivalent you build for Windows (PowerShell or cmd), default the core build to `--release` before launching the C# app in Debug. If "app feels slow" comes up, first confirm the linked `.lib`/`.dll` isn't Debug-core — check `target/release` mtime or file timestamp before investigating code. Only flip to `PROFILE=debug` for sessions where you genuinely need to step through Rust.

## Task tracking

`backlog/` dir at repo root, managed by the Backlog.md CLI (`backlog` command, npm global). Don't suggest GitHub Issues, Linear, or ad-hoc TODO.md. Tasks live in `backlog/tasks/`, decisions in `backlog/decisions/`, drafts in `backlog/drafts/`.

## Suggested first read on Windows side

1. `README.md` — public overview.
2. `backlog/` via `backlog board` — current task state.
3. `core/` — Rust crate. Identify the phase-machine / snapshot types exposed via C ABI; those are what the C# P/Invoke layer binds to.
4. `apps/macos/` — reference shell. Mirror its orchestration contract in C#, don't re-derive.
5. `docs/spikes/` — background investigations (e.g. `task-3-parakeet-on-apple-silicon.md`).
