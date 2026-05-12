---
id: doc-45
title: HeyEmma comparison — feature gap via OpenAI Whisper API
type: research
created_date: '2026-05-09 06:10'
status: research
related:
  - milestones/m-1
sources:
  - https://tryheyemma.com/ (fetched 2026-05-09)
---

# HeyEmma comparison — feature gap via OpenAI Whisper API

Audit of [HeyEmma](https://tryheyemma.com/) against OpenWhisper, framed as: *if we adopted OpenAI's Whisper API as a BYO-key cloud provider, how far is OpenWhisper from feature-parity with Emma?*

HeyEmma is a closed-source, OpenAI-only voice assistant for Mac/Windows. Sold as a $119 one-time license (BYO OpenAI key, ~$1–2/mo API spend, 14-day refund). Created by Ruben Juncher. Positioned as "press, talk, done" with three personas: dictation, code prompting, voice assistant Q&A.

Captured to scope a possible v1.1+ direction. Nothing here is queued for v1.0 — v1.0 milestone remains polish + OSS readiness, not new product surface.

## HeyEmma feature inventory

| Surface | Capability |
|---|---|
| **Dictation** | `fn` (Mac) / `F1` (Win) toggle. Speak → text inserted into focused app. Claimed "4× faster than typing". |
| **AI cleanup** | Auto grammar/cleanup of dictation output before insert. |
| **VibeCode mode** | Conversational speech → technical AI-agent prompt. Compatible w/ Lovable + Claude Code. |
| **Custom modes** | User-defined modes beyond Dictation/VibeCode/Professional (e.g. "translate to English", "format as LinkedIn post"). |
| **Custom actions** | Trigger words route dictation to Google Sheets / Numbers / calendars / Make / n8n / custom HTTP endpoints. |
| **Hey Emma Q&A** | Wake-phrase "Hey Emma…" → answer in-place without leaving current app. |
| **Hey Emma Vision** | "Hey Emma, look at my screen" → screen capture sent to OpenAI vision model, answers about visible content. |
| **Reminders** | "Hey Emma, set a reminder…" → native macOS / Windows notification. |
| **Document analysis** | Scan PDFs/contracts for lock-in periods, cancellation terms, auto-renewals, hidden fees. Returns natural-language summary. |
| **Languages** | "90+ languages" (claim, no detail). |
| **Licensing** | $119 one-time, 2 devices, lifetime updates, no subscription. |
| **Privacy posture** | "Your API key, no middlemen. We store nothing." |

## OpenWhisper relevant capabilities (today)

Drawn from `core/`, `apps/tauri/`, `cli/`, `backlog/tasks/` as of 2026-05-09 (`main` post-v0.6.0).

- **Local STT** — Parakeet-TDT v3 on ANE (Mac, FluidAudio) + ONNX CPU/EP probe (Win, sherpa-onnx) behind a `Recognizer` trait in `core/src/recognizer/mod.rs`. Production. Free.
- **Toggle hotkey + paste** — phase machine in `core/src/dictation.rs` (IDLE → LOADING_MODEL → RECORDING → TRANSCRIBING → DONE/ERROR). Rebindable hotkey (chords + single-modifier).
- **Transcript post-processing** — `core/src/transcript.rs`: filler strip (EN + DA registers), user substitutions, adjacent-duplicate collapse, whitespace.
- **Settings** — Home / General / Models / Diagnostics / Dev Tools panes. Launch-at-Login, theme, fullscreen override, WPM, keep-models-warm, memory budget, storage.
- **Model lifecycle** — auto-unload after 5-min idle (TASK-62 Done). Stats strip (TASK-78 / TASK-88 Done): WPM, time saved, count.
- **CLI** — `transcribe`, `enumerate-devices`, `recognizer-info`, `memory`, `settings`, `crash-dump` (placeholder).
- **Diagnostics** — RSS, peak memory, crash list/detail.
- **Platform parity** — Mac + Win shipping; Linux planned.

## Feature-by-feature gap

Effort = rough engineering estimate against current architecture. T-shirt sizing.

| HeyEmma feature | OpenWhisper status | Effort | Notes |
|---|---|---|---|
| OpenAI Whisper STT (BYO key) | not built | **S** ~1 wk | Slot a `WhisperApiRecognizer` behind existing `Recognizer` trait. HTTP POST to `/v1/audio/transcriptions`. Settings row pattern already exists. |
| LLM cleanup (`gpt-4o-mini` etc.) | scaffolded local-only — TASK-63 (Qwen via llama-cpp-2) To Do; TASK-17 + TASK-45 (cloud cleanup + Ollama bridge) Won't Do | **S** ~1 wk | Reviving TASK-45's settings schema (`cleanup.backend / endpoint / model / language`) is the unlock. Cloud branch reuses local pipeline. |
| VibeCode mode | not built | **XS** 1–2 days | System-prompt preset routed through cleanup. Pure prompt engineering once cleanup pipeline exists. |
| Custom modes (Dictation / Pro / user-defined) | not built | **M** ~2 wks | New settings pane, per-mode prompt + per-mode hotkey-or-picker UI. Per project rule, headless-first: must also surface in `cli/src/commands/`. |
| Document analysis (PDF / contract scan) | not built | **M** ~2 wks | File-picker → chat completion w/ extraction prompt. New pane. Standalone surface — doesn't depend on dictation flow. |
| Custom actions / webhook output (Sheets, n8n, custom HTTP) | not built | **M-L** ~3 wks | Per `openwhisper-orchestration-in-rust`, action dispatch must live in Rust core. Settings UI for action defs + trigger-word parser. |
| Reminders → OS notifications | not built | **M** ~1.5 wks | Tauri notification plugin available. Need natural-language parsing ("set reminder for X at Y") + OS scheduler. |
| "Hey Emma" wake-word Q&A | **out of scope per principles** | **L + product conflict** ~4+ wks | `openwhisper-project-principles` rules out wake-word and always-on listening. Would require principle revision, not just engineering. |
| Vision ("look at my screen") | not built, not on roadmap | **L + product conflict** ~4+ wks | ScreenCaptureKit (Mac) + DXGI (Win) + new TCC permission flow + vision API call. Tension with local-first ethos when image leaves device. |

## Effort summary

Three tiers:

1. **Cheap path — Emma-equivalent dictation product, ~4–6 wks.**
   OpenAI Whisper STT + cloud cleanup + VibeCode + custom modes + document analysis. All slot into existing trait / pipeline / settings architecture. No principle conflicts. Net: OpenWhisper gains every Emma *text* feature while keeping local STT default and BYO-key cloud opt-in — strictly better positioning than Emma (which is OpenAI-only).

2. **Mid path — add reminders + custom actions, ~+4–5 wks.**
   Pure new surface, no architectural fights. Reminders is a small, demoable hook. Custom-actions/webhooks is the real lock-in feature.

3. **Hard path — wake-word + vision, ~+8 wks and a principle revision.**
   These are the two demos that make Emma feel like an "assistant" rather than dictation. Both collide with current OpenWhisper principles. Doable, but a product call, not just engineering.

## Unlock dependencies

- **Revive TASK-17 + TASK-45** (cloud LLM cleanup + bridge UI). Currently Won't Do. Without these the Emma-shaped surface has no foundation.
- **Decide cloud-STT story.** Whisper API as a `Recognizer` trait impl is the smallest possible patch. Open question: does it sit alongside Parakeet (per-mode override), replace it for specific languages, or appear only when local is unavailable?
- **Custom modes need a UX language** — how a user binds a mode to a hotkey, picks one mid-session, and how it interacts with the cleanup pipeline. Worth a design pass before any code.
- **Wake-word + vision need a principles update first.** Don't start engineering until `openwhisper-project-principles` is rewritten or explicitly carved out.

## Surprise findings

- **TASK-17 (cloud LLM cleanup) + TASK-45 (Ollama bridge UI) are both Won't Do.** Earlier deferral. Reviving them is the unlock for half the Emma surface.
- **Custom vocabulary (TASK-10) is also Won't Do.** Per `openwhisper-parakeet-quirks`, post-processing is the right fix anyway. Custom-mode prompts effectively absorb that need without a vocab UI.
- **OpenWhisper has a CLI; Emma does not.** Headless-first architecture means custom-mode + cleanup features land as scriptable surfaces "for free" — a meaningful differentiator vs Emma for power users.
- **Diagnostics + crash-dump panes have no Emma equivalent.** Open-source-credibility surface that Emma cannot trivially match.
- **Emma's $119 license is one-time, no subscription.** OpenWhisper's stated principle is "free local + BYO-key cloud + opt-in hosted convenience". The natural pricing wedge is *free vs $119* if local-only matches Emma's text features.

## Net read

OpenWhisper is **closer than it looks**. STT engine, hotkey, paste, settings, diagnostics, multi-platform shell are all done. Adding OpenAI Whisper API as a BYO-key cloud `Recognizer` is days of work. The Emma feature wedge that's actually valuable (modes + cleanup + custom actions) is ~6 wks of focused work and slots into the existing architecture without principle conflicts. The "voice assistant" wedge (wake-word + vision) is where Emma is genuinely ahead, and matching it requires a product call, not just engineering effort.
