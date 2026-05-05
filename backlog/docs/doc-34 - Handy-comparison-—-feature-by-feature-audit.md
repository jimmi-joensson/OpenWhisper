---
id: doc-34
title: Handy comparison — feature-by-feature audit
type: research
created_date: '2026-05-05 04:56'
status: research
related:
  - milestones/m-1
sources:
  - https://github.com/cjpais/Handy (v0.8.3, MIT, 21k★, pushed 2026-04-30)
---

# Handy comparison — feature-by-feature audit

Audit of [`cjpais/Handy`](https://github.com/cjpais/Handy) against OpenWhisper. Handy is the closest peer in the local-first dictation space — Tauri 2 + React, Whisper + Parakeet, Mac/Win/Linux. Compared at v0.8.3 (latest release 2026-04-28) against `main` of OpenWhisper as of 2026-05-05.

Captured to defer action — nothing here is queued for v1.0. v1.0 milestone is polish + OSS readiness, not new product surface. Revisit before v1.1 scoping.

## Architectural delta

| Axis | OpenWhisper | Handy |
|---|---|---|
| Shell layout | `core/` Rust crate + `apps/tauri/` shell | single Tauri project (no shared core) |
| Recognizer | FluidAudio (Mac/ANE) + sherpa-onnx (Win/CPU) behind trait | `whisper-rs` + `transcribe-rs` (Parakeet V3) directly |
| Languages shipped | EN + DA filler register | 17-lang filler register, 19 i18next locales |
| Hotkey slots | `toggle` + `cancel` (2) | `transcribe` + `transcribe_with_post_process` + `cancel` (3) |
| Settings store | hand-rolled JSON, sibling-block merge | `tauri-plugin-store` w/ dual-format de + migration helpers |
| TS/Rust contract | hand-typed `invoke<T>()` | `tauri-specta` auto-generated `bindings.ts` |
| OS coverage | Mac + Win | Mac + Win + Linux (gtk-layer-shell overlay) |

Handy is **wide and feature-rich**. OpenWhisper is **narrow and disciplined** (clean Rust core, BT-aware audio ducking, deeper platform polish per OS).

## Feature parity matrix

| Feature | OpenWhisper | Handy | Notes |
|---|---|---|---|
| Toggle hotkey | ✅ | ✅ | |
| Push-to-talk | ❌ | ✅ per-binding | Handy default `push_to_talk=true` |
| Cancel hotkey | ✅ | ✅ | |
| VAD | TASK-14 (Silero) | Silero + custom `SmoothedVad` (prefill+onset+hangover) | Handy adds onset frame counter + pre-roll buffer |
| Filler strip | EN+DA, æ/ø/å lang-detect | 17-lang ISO-keyed register + custom-list override | |
| Stutter collapse | ✅ TASK-44 (adjacent only) | ✅ 3+ run threshold | |
| Substitutions | hardcoded ("open whisper" → "OpenWhisper") | user-editable list w/ fuzzy match | |
| Custom vocabulary | ❌ TASK-10 Won't Do | ✅ Levenshtein + Soundex + n-gram (1–3) | see §"TASK-10 reopen reference" |
| LLM cleanup | ❌ TASK-17 Won't Do | ✅ 8 providers + Apple Intelligence | see §"TASK-17 reopen reference" |
| Audio ducking | ✅ SMTC pause + BT mono-blip mask | ✅ system-output mute (osascript/wpctl/Win COM) | OW more sophisticated |
| Audio feedback chimes | ❌ | ✅ Marimba/Pop/Custom + volume + output device | |
| Pill/overlay | ✅ follow-active-screen | ✅ Top/Bottom/None | |
| Tray | ✅ Mac+Win | ✅ Mac+Win+Linux | |
| Single-instance | TASK-37 | ✅ + remote-control via arg-pipe | confirm OW raises window on second launch |
| CLI | TASK-81 (separate binary) | same binary remote-controls running instance | complementary, not equivalent |
| SIGUSR1/2 toggle | ❌ | ✅ Wayland-friendly | Linux-only relevance |
| History DB | ❌ | ✅ SQLite + retention (3d/2w/3m/never) | privacy concern |
| Auto-submit Enter | ❌ | ✅ Enter/Ctrl+Enter/Cmd+Enter | chat-app QOL |
| Append trailing space | ✅ unconditional | user-toggleable | |
| Paste method | mac CGEvent / win SendInput | 6-way: CtrlV/Direct/None/ShiftIns/CtrlShiftV/ExternalScript | |
| Linux typing-tool | N/A (deferred) | Auto/wtype/kwtype/dotool/ydotool/xdotool | |
| Clipboard handling | ❌ | DontModify (default) / CopyToClipboard | |
| Clamshell mic switch | ❌ | ✅ `ioreg AppleClamshellState` + `clamshell_microphone` setting | |
| Always-on mic | ❌ | ✅ `lazy_stream_close` + 30s STREAM_IDLE_TIMEOUT | privacy negative for OW |
| Mute-while-recording | conflicts w/ pause-during-dictation | ✅ separate setting | |
| Model unload timeout | ❌ | Never/Imm/2/5/10/15min/1h | |
| `extra_recording_buffer_ms` | ❌ | ✅ user-tunable | trailing-audio knob |
| `paste_delay_ms` | ❌ | ✅ default 60 ms | first-char-eaten knob |
| Word-correction threshold | N/A | ✅ default 0.18 | |
| Update checker toggle | ❌ | ✅ | |
| Autostart toggle | TASK-? launch-at-login | ✅ tauri-plugin-autostart | parity |
| i18n | EN-only | 19 locales, ESLint-enforced no-hardcoded-strings | |
| Translate-to-English | ❌ | Whisper task=translate (N/A on Parakeet) | |
| Tauri-specta TS bindings | ❌ | ✅ auto-generated `bindings.ts` | DX upgrade |
| Coordinator FSM | implicit (atomics in dictation.rs) | explicit thread + mpsc + 30 ms debounce | |
| `portable.rs` config | ❌ | ✅ relocate next to binary | niche |
| Minisign release verify | ❌ | ✅ + pubkey in tauri.conf.json | |
| Distribution | DMG + MSI direct | + Homebrew cask + winget (community-maintained) | |
| `SecretMap` redact | ❌ | ✅ Debug impl returns `[REDACTED]` | API-key safety |

## TASK-10 reopen reference (custom vocabulary)

**File:** `src-tauri/src/audio_toolkit/text.rs` (~570 LOC incl. tests, MIT).

**Approach** — exactly the post-pass route TASK-10 proposed (Parakeet has no native biasing):

- `apply_custom_words(text, custom_words, threshold)` — n-gram (1–3 words) greedy match, longest first.
- Each n-gram cleaned via `build_ngram` (strip non-alphanumerics, lowercase, concat). Match "Charge B" → "ChargeBee", "Chat G P T" → "ChatGPT".
- Per candidate vs `custom_words_nospace[i]`:
  - Length filter: `len_diff > max(2, max_len * 0.25)` skip — prevents "openaigpt" matching "openai".
  - Levenshtein distance normalised by max length.
  - Soundex phonetic match boosts score `× 0.3` when phonetic-equal.
  - `combined_score < threshold && < best_score` → win.
- Replacement preserves case pattern from original first word + extracts punctuation prefix/suffix from boundary tokens.

**Tests cover** — exact match, fuzzy ("helo wrold"), case preservation (HELLO → WORLD; Hello → World), 2-/3-gram matches, longer-ngram preference (Open AI GPT → OpenAI GPT), trailing-number not double-counted (GPT-44 regression), case from input ("CHARGE B" → "CHARGEBEE"), space-in-custom ("Mac Book Pro" → "MacBook Pro").

**Why TASK-10 was Won't-Do**: model can't bias natively. **Why this still works**: it's a post-pass on the recognizer output, no model changes. Implementation is small (~250 LOC core + tests), copyable, MIT.

**Re-evaluation trigger**: this is exactly the gap `openwhisper-parakeet-quirks` skill points users at. Could ship without engine changes.

## TASK-17 reopen reference (LLM cleanup)

**Files:** `src-tauri/src/llm_client.rs` (278 LOC), `apple_intelligence.rs` (85 LOC), `actions.rs` (27 KB orchestration), `swift/` Apple Intelligence wrapper.

**Provider model** — `PostProcessProvider { id, label, base_url, allow_base_url_edit, models_endpoint, supports_structured_output }`. Defaults ship: OpenAI / Z.AI / OpenRouter / Anthropic / Groq / Cerebras / AWS Bedrock (Mantle) / Apple Intelligence (Mac ARM64) / Custom (Ollama, base URL editable).

**Auth** — Bearer for everyone; Anthropic special-cased (`x-api-key` + `anthropic-version: 2023-06-01`). API keys stored in `SecretMap` (redacted in Debug). Common headers include `Referer` + `X-Title` (OpenRouter convention).

**Request shape** — OpenAI chat completions. Optional structured outputs (`response_format: { type: "json_schema", json_schema: { name, strict, schema } }`). Optional reasoning controls (top-level `reasoning_effort` for OpenAI, nested `reasoning {effort, exclude}` for OpenRouter).

**Default repair prompt** — explicitly bounded ("Preserve exact meaning and word order. Do not paraphrase or reorder content. Return only the cleaned transcript."). Fix spelling/cap/punct, num-words → digits, spoken-punct → symbols, drop fillers, **preserve language**.

**Apple Intelligence** — Swift FFI to `SystemLanguageModel.default` on macOS 26+ ARM64. `is_apple_intelligence_available()` lazy-checked at use-time, **not** at init (avoids early-boot SIGABRT on macOS 26 beta). Free, on-device, no network. `process_text_with_system_prompt(system, user, max_tokens) -> Result<String, String>`.

**Two-hotkey UX** — `transcribe` (raw) and `transcribe_with_post_process` (LLM cleanup). Same recording; downstream branch. Avoids settings-toggle dance.

**Re-evaluation trigger** — Mac-only/free Apple-Intelligence path is pure-upside for OW's "local-first, free by default" pillar. Could revive TASK-17 as a Mac-first feature, defer BYO-cloud route. New product surface but zero disk/RAM/$ cost on Mac for users on macOS 26+.

## TASK-81 cross-reference (orchestration extraction)

`transcription_coordinator.rs` (185 LOC) is a clean reference for TASK-81.2 (extract orchestration into core/):

- Single thread + `mpsc::channel<Command>`. `Command::{Input, Cancel, ProcessingFinished}`.
- FSM `Stage::{Idle, Recording(binding_id), Processing}`.
- 30 ms press debounce (key-repeat / double-tap protection); releases pass through for push-to-talk.
- Push-to-talk vs toggle disambiguation lives in the coordinator, not in shells.
- `ACTION_MAP` registry of binding-id → action — equivalent to OW's binding-target dispatch.

Map their `Stage::Recording` → OW's `PHASE_RECORDING`, `Stage::Processing` → `PHASE_TRANSCRIBING`. Coordinator owns transitions; shells fire `send_input(binding_id, source, is_pressed, push_to_talk)`. CLI/signals route through same entry point.

OW already has phase atomics; the coordinator pattern adds a single-writer guarantee without explicit atomics. Worth referencing — not necessarily copying.

## Recommendations bucket

### Strongly relevant — fits v1.0 polish theme

1. **Tauri-specta auto-generated bindings** — replaces manual `invoke<T>()` typing. Pure DX, no product change. ~1-day land, fits OSS-readiness theme.
2. **Coordinator FSM as TASK-81.2 reference** — copyable pattern, MIT-licensed.
3. **Single-instance window-restore confirm** — verify TASK-37 covers "second launch raises existing window" parity.

### v1.1+ candidates — would revive de-scoped tasks

4. **Apple Intelligence local-LLM cleanup** — best brand fit ("free by default" extended to LLM cleanup on Mac). ~85 LOC FFI + Swift wrapper. Mac-only, macOS 26+ gate.
5. **Custom-vocab fuzzy post-pass** — revives TASK-10 with working reference. Small, tested, fits Parakeet-quirks-skill direction.
6. **`transcribe_with_post_process` second hotkey** — pairs with #4/#5; clean UX without settings dance.
7. **Push-to-talk per binding** — common request; add `push_to_talk: bool` to `HotkeyConfig`.
8. **`paste_delay_ms` + `extra_recording_buffer_ms`** — knobs for "first/last word eaten" complaints expected post-public.
9. **Auto-submit Enter** — chat-app QOL (Slack/iMessage/Linear).
10. **Lang-aware filler register expansion** beyond EN/DA — pairs with #5; same code path.

### Skip / wrong fit

- **Always-on mic** — privacy negative; breaks "mic on only during dictation" implicit contract.
- **History DB** — privacy concern; if added must be opt-in default-off.
- **Mute-while-recording** — OW's SMTC + BT-mono-blip pause is more sophisticated; don't regress.
- **Translate-to-English** — needs Whisper engine swap.
- **i18n** — premature until product surface stabilises.
- **`portable.rs`** — niche.
- **Brew cask / winget** — community-maintained; not maintainer work.

## Files to study before re-opening tasks

- `src-tauri/src/audio_toolkit/text.rs` — TASK-10 reopen reference
- `src-tauri/src/llm_client.rs` + `actions.rs` — TASK-17 reopen reference
- `src-tauri/src/apple_intelligence.rs` + `swift/` wrapper — free local LLM cleanup
- `src-tauri/src/transcription_coordinator.rs` — TASK-81.2 reference pattern
- `src-tauri/src/settings.rs` (`SecretMap`, dual-format LogLevel deserializer, `ensure_post_process_defaults` migration) — settings-store discipline
- `src/bindings.ts` shape + `Cargo.toml` `tauri-specta` deps — DX upgrade
- `src-tauri/src/cli.rs` + `signal_handle.rs` — single-instance remote-control pattern (different from TASK-81 headless CLI)

## One-line take

Keep the discipline (Rust core, BT-aware pause, Mac+Win parity, free-by-default). Steal three things: tauri-specta bindings (now), Apple-Intelligence cleanup (revives TASK-17 free-on-Mac), custom-vocab fuzzy post-pass (revives TASK-10 — Parakeet quirks won't fix themselves).
