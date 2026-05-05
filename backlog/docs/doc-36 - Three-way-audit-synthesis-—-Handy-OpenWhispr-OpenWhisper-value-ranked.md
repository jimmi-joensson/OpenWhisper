---
id: doc-36
title: Three-way audit synthesis — Handy + OpenWhispr + OpenWhisper (value-ranked)
type: research
created_date: '2026-05-05 05:41'
status: research
related:
  - milestones/m-1
  - tasks/task-85
  - docs/doc-34
  - docs/doc-35
sources:
  - https://github.com/cjpais/Handy (v0.8.3, MIT, 21k★) — see doc-34
  - https://github.com/OpenWhispr/openwhispr (v1.7.0, MIT, 2.9k★) — see doc-35
---

# Three-way audit synthesis — Handy + OpenWhispr + OpenWhisper

Synthesis layer over [doc-34 (Handy)](doc-34%20-%20Handy-comparison-—-feature-by-feature-audit.md) and [doc-35 (OpenWhispr)](doc-35%20-%20OpenWhispr-comparison-—-feature-by-feature-audit.md). Where the two source docs catalog *what exists*, this doc ranks *what's worth doing* on a value-vs-effort basis, split by horizon (short-term v1.0/v1.1 vs long-term v2.0/subscription tier).

**Project context (load-bearing):**
- v1.0 milestone is polish + OSS readiness, not new product surface.
- Local-first / free-by-default for the dictation core stays a hard line forever.
- Hosted-cloud subscription tier is on the roadmap (sync, accounts, OW-operated inference, REST + MCP). BYO-keys stays free even after subscription ships.
- Mac + Win shipping; Linux deferred.

## Three-way positioning

| Axis | Handy | OpenWhispr | OpenWhisper |
|---|---|---|---|
| Product scope | dictation only | notes + meetings + agents + dictation + cloud suite | dictation only (v1) → +hosted cloud sub (v2) |
| Shell | Tauri 2 + React | Electron 41 + React | Tauri 2 + React + Rust core crate |
| Recognizer | whisper-rs + transcribe-rs (Parakeet V3) | whisper.cpp sidecar + sherpa-onnx sidecar | FluidAudio (Mac/ANE) + sherpa-onnx (Win/CPU) behind trait |
| OS | Mac + Win + Linux | Mac + Win + Linux | Mac + Win |
| Cloud / account | none | Better Auth + Postgres (Neon) + REST + MCP | none today; planned for v2 subscription tier |
| LLM cleanup | none | per-scope: Cloud (4 providers + custom) + Self-Hosted + bundled llama-server (Vulkan) | TASK-63 in flight (in-process llama-cpp-2) |
| Custom vocab | Levenshtein + Soundex + n-gram (1–3) post-pass | flat list, fed to LLM cleanup as bias hint | TASK-10 Won't Do |
| Audio ducking | system-output mute (osascript / wpctl / Win COM) | GSMTC via Python WinRT sidecar + media-key fallback | SMTC pause + BT mono-blip mask (most aggressive) |
| Org / scale | solo, MIT, 21k★ | OpenWhispr/Gizmo Labs, MIT, 2.9k★, sponsor-funded | solo, MIT, pre-public |

**Reading the table:** Handy is the *closest peer* (same scope, same shell family, same recognizer choices). OpenWhispr is the *category-adjacent platform* (broader product, plus the cloud/account scaffolding OW will need long-term). OpenWhisper sits between them: dictation-only discipline today, with a believable path to OpenWhispr-shaped backend infra without ever becoming OpenWhispr the product.

## Three buckets, two horizons

For each candidate: **Source** (which audit it comes from), **Effort** (S/M/L/XL — rough engineering days), **Value** (S/M/H/HH — to OW specifically), **Risk** of regressing principles. Items ordered within each horizon by **value/effort ratio**.

### Bucket A — Short-term (v1.0 / v1.1) wins

These fit the polish + OSS-readiness theme of v1.0 (or are small enough to slip into v1.1) and don't open new product surface.

| # | Item | Source | Effort | Value | Risk | Why now |
|---|---|---|---|---|---|---|
| A1 | **TASK-85 rename sweep** (already in backlog) | OpenWhispr | L | HH | none — already planned | Name collision is unavoidable: 2.9k★, 1-yr head-start, owns `openwhispr.com`/`docs.openwhispr.com`/protocol scheme/npm/MCP namespace. Confirms scope, doesn't shrink it. **Do this first.** |
| A2 | **tauri-specta auto-generated TS bindings** | Handy | S | H | none | Replaces hand-typed `invoke<T>()`. Pure DX upgrade, fits OSS-readiness. ~1 day. |
| A3 | **Coordinator FSM as TASK-81.2 reference pattern** | Handy | reference-only | H | none | `transcription_coordinator.rs` (185 LOC) is exactly the shape TASK-81.2 needs — single thread + mpsc + 30 ms debounce + push-to-talk vs toggle disambiguation in coordinator (not shells). Copyable. |
| A4 | **Single-instance second-launch raises window** | Handy | S | M | none | Verify TASK-37 covers this. Pure UX bug-class prevention. |
| A5 | **Download infra (`downloadUtils.js`) shape** | OpenWhispr | reference-only | H | none | TASK-63.2 (GGUF download) needs exactly this: 3xx redirect follow + resume + retry+backoff + proxy-aware + sentinel-idempotent. Port shape into Rust (~150 LOC `reqwest`-based). |
| A6 | **Right-side modifier hotkey routing** | OpenWhispr | S | M | none | Regex-detect `Right(Control\|Alt\|...)` → route to native listener instead of globalShortcut. Pairs with A8. Small, contained. |
| A7 | **`paste_delay_ms` + `extra_recording_buffer_ms` knobs** | Handy | S | M | low | Defuses "first-char eaten / last-word clipped" complaints expected post-public-launch. Cheap insurance. |
| A8 | **Push-to-talk per binding** (`HotkeyConfig.push_to_talk: bool`) | Handy | M | M | low | Common ask. Coordinator FSM (A3) already disambiguates press vs release. Adds a checkbox to rebind UI. |
| A9 | **Auto-submit Enter (Enter / Ctrl+Enter / Cmd+Enter)** | Handy | M | M | low | Chat-app QOL (Slack / iMessage / Linear). Per-app heuristic optional. |
| A10 | **`SecretMap` redact-in-Debug pattern** | Handy | S | M | none | Trivial impl on whatever secrets land. Insurance against accidental log of API key in `tracing` output. |
| A11 | **Append-trailing-space → user-toggleable** | Handy | S | S | none | Currently unconditional in OW. One-flag fix. |

**A1–A5 are the must-do shortlist.** Each is either already planned (A1), pure DX (A2, A3), or directly de-risks an in-flight task (A5 → TASK-63.2). A6–A11 are nice-to-haves that only make sense to bundle into v1.1 if they don't slow v1.0 ship.

### Bucket B — Long-term (v2.0 / subscription tier) candidates

These become directly relevant when OW opens up the hosted-cloud subscription tier. None of them belong in v1.0 — but each one is a "study before designing" reference rather than a "build now" item.

| # | Item | Source | Effort | Value (when needed) | Risk | When to revisit |
|---|---|---|---|---|---|---|
| B1 | **API-key encryption at rest** (`safeStorage` Mac-Keychain / Win-DPAPI / Linux-libsecret) | OpenWhispr | M | HH | none | When *any* secret first lands — TASK-17, TASK-45, TASK-63-cloud, or subscription-tier auth. One-time silent migration + sentinel + round-trip-verify is a clean pattern; copy it before plaintext-key code accretes. **Earliest of the long-term items to need.** |
| B2 | **Better Auth + bearer-token-in-OS-keychain** | OpenWhispr | L | HH | medium — opens account flows | When subscription tier launches. Better Auth is OSS, the `auth.openwhispr.com`-style pattern is reproducible. Survives renderer crashes + session resets. `VITE_AUTH_URL` lets self-hosters point at their own server. |
| B3 | **Inference-provider three-way split (cloud / lan / local-bundled)** | OpenWhispr | M (UI) + M (backend) | H | low | When TASK-17/45 ever revives, OR when subscription tier exposes "cleanup runs on OW-operated inference" as the upgrade path. Three-way is the right shape; don't start from a two-way (cloud/local) split. UI ref: `SelfHostedPanel.tsx` + `InferenceConfigEditor.tsx`. |
| B4 | **Apple Intelligence local-LLM cleanup** (Mac-only, free, ~85 LOC FFI + Swift wrapper) | Handy | M | H | low | Best brand fit ("free by default" extended to LLM cleanup on Mac). macOS 26+ ARM64 only. Could revive TASK-17 as a Mac-first feature; defer BYO-cloud route until subscription tier. **Highest value-to-effort ratio in B.** |
| B5 | **Custom-vocab fuzzy post-pass** (~250 LOC + tests) | Handy | M | H | low | Revives TASK-10. MIT-licensed reference (`audio_toolkit/text.rs`). Levenshtein + Soundex + n-gram (1–3) — recognizer post-pass, no model changes. Pairs naturally with B4 (Mac AI cleanup) or B6. |
| B6 | **`transcribe_with_post_process` second hotkey** | Handy | S | M | none | Pairs with B4/B5. Same recording, downstream branch (raw vs cleaned). Avoids settings-toggle dance. |
| B7 | **Cloud sync schema shape** (Postgres, nullable-column-compat, idempotent migrations) | OpenWhispr | XL | HH | high — opens history-DB privacy questions | Only when subscription tier explicitly includes transcript history sync. **Tied to opt-in default-off discipline** (see B-block risks). Reference: `services/SyncService.ts` + `services/{Notes,Folders,Conversations,ApiKeys,Transcriptions}Service.ts`. |
| B8 | **Per-scope thinking-mode suppression** (`thinkingSuppression.ts`) | OpenWhispr | S | M | none | Becomes useful when TASK-63 exposes user-pickable models (Qwen-think etc.). Strip reasoning blocks per-scope; default suppressed for dictation. |
| B9 | **Public REST API + MCP server** | OpenWhispr | XL | H | medium — public-API maintenance burden | Once a hosted backend exists, exposing it as REST + MCP is table-stakes for power-user/agent-integration positioning. Worth keeping in mind when designing B7's sync schema so REST doesn't bolt on awkwardly later. |
| B10 | **Lang-aware filler register expansion (17-lang ISO-keyed)** | Handy | M | M | none | Pairs with B5. Same code path. Defer until product surface stabilises and at least 2–3 non-EN/DA languages have user demand. |
| B11 | **OAuth callback via custom URL scheme** (with disable-with-tooltip when handler unregistered) | OpenWhispr | S | M | none | Required for B2 (Better Auth) to work on desktop. UX guard against stuck OAuth flows. |
| B12 | **Streaming-provider abstraction (cloud STT)** | OpenWhispr | XL | M | high — pulls in meeting-product gravity | Only relevant if subscription tier offers paid hosted-cloud transcription as upgrade from local Parakeet (low-latency / 100+ language). Same three-way pluggable shape as B3 — apply consistently, but don't build until clear demand. |

**Bucket B sequencing:** B1 ships first (whenever first secret lands). B4 + B5 + B6 can ship as a Mac-first cluster pre-subscription. B2 + B3 + B11 are the subscription-tier launch cluster. B7 + B9 + B12 are post-launch expansion that only make sense once subscription has paying users.

### Bucket C — Skip / wrong-fit (with flip conditions)

These are explicitly off-path. Each entry includes the condition that would re-open the question.

| # | Item | Source | Why skip | Would flip if… |
|---|---|---|---|---|
| C1 | Notes / folders / semantic search | OpenWhispr | Dilutes "open Superwhisper alternative" positioning | Subscription tier explicitly targets Granola-style notes adjacency |
| C2 | Meeting transcription / live diarization / Zoom-Teams-FaceTime auto-detect | OpenWhispr | Different audio capture (system-audio tap, multi-stream merge, AEC); product gravity well | Subscription tier explicitly targets meeting-bot/Otter category |
| C3 | AI agent / tool registry / named voice assistant | OpenWhispr | Conflicts with dictation-only positioning | Never (would erase wedge) |
| C4 | History DB on disk by default | both | Privacy negative; "mic on only during dictation" implicit contract extends to "transcripts on disk only if asked" | Cloud sync history added as opt-in default-off (B7) |
| C5 | Always-on mic / `lazy_stream_close` / 30s STREAM_IDLE_TIMEOUT | Handy | Privacy negative; breaks mic-on-only-during-dictation contract | Never (would erase wedge) |
| C6 | Mute-while-recording | Handy | OW's SMTC + BT-mono-blip is more sophisticated; don't regress | Never |
| C7 | Translate-to-English | Handy | Needs Whisper engine swap (Parakeet doesn't have task=translate) | OW ever ships Whisper as alt engine |
| C8 | i18n (9-locale or 19-locale) | both | Premature until product surface stabilises | Subscription tier launches with measurable non-EN demand |
| C9 | `portable.rs` config (relocate next to binary) | Handy | Niche use case | Never |
| C10 | Brew cask / winget / community packaging | Handy | Community-maintained; not maintainer work | Community offers maintenance |
| C11 | Linux SIGUSR1/2 toggle, gtk-layer-shell, PTT setup card | Handy + OpenWhispr | Linux not on roadmap | Linux ever joins ship list (post-v2 at earliest) |
| C12 | Electron migration | OpenWhispr | Tauri picked deliberately for native footprint + Rust core | Never |
| C13 | Bundled llama-server sidecar | OpenWhispr | TASK-63 picked in-process llama-cpp-2; sidecar bookkeeping (`sidecarPidFile`/`sidecarReaper`/`sidecarRegistry`) is overhead we don't want | TASK-63 hits a wall in-process and externalising becomes the only path |
| C14 | Sponsor / referral / usage tier UI | OpenWhispr | Not v1.0; UI references useful for B2 cluster but don't build now | Subscription tier launches (then becomes a B-bucket UI reference, not a copy) |

## Value-ranked do-list (the headline)

If you read only one section, read this. Eight items, ordered by when to act.

1. **A1 — TASK-85 rename sweep** (now). Already in backlog. Unblocks public launch. Highest value, already planned.
2. **A5 — Download infra Rust port for TASK-63.2** (now). Unblocks TASK-63 GGUF download path. Reference pattern from OpenWhispr; 1–2 days of Rust.
3. **A2 — tauri-specta TS bindings** (now). 1-day DX win, fits OSS-readiness theme.
4. **A3 — Coordinator FSM reference for TASK-81.2** (now). Reference-only; no extra work, just a shape to copy from Handy when TASK-81.2 begins.
5. **A6–A11 cluster** (v1.1). Push-to-talk, paste-delay knobs, right-modifier routing, secret-redact, auto-submit. Bundle into one v1.1 sweep if they don't slow v1.0.
6. **B1 — API-key encryption at rest** (whenever first secret lands). Earliest B-item to need; cheaper to build once than to bolt on later.
7. **B4 + B5 + B6 cluster — Mac AI cleanup + custom vocab + second hotkey** (when ready to revive TASK-17 path). Highest value-to-effort ratio in B. Mac-first, free-on-Mac, no subscription dependency.
8. **B2 + B3 + B11 cluster — Better Auth + provider three-way split + URL-scheme OAuth** (subscription-tier launch). Build together; they share infra and UX flow.

## What the three sources tell us together

- **Handy is the engineering reference**: copyable Rust code, MIT, focused scope. Where doc-34 lists a feature with LOC count, treat that as a green-light for OW to lift the implementation.
- **OpenWhispr is the architecture-and-roadmap reference**: don't copy code (Electron + Node), do study patterns (provider abstraction, encryption-at-rest, sync schema, auth flow). Where doc-35 lists a v2.0 candidate, treat it as a "study before designing" pin for whenever subscription tier opens.
- **OpenWhisper's discipline is the moat**: BT-aware audio ducking, Rust core, Mac-ANE recognizer, TASK-85 disambiguating the name, planned subscription-tier-as-additive-not-paywall posture. None of the three competitors has all of these. Don't trade them away for surface-area parity.

## One-line take

Short-term: ship A1 (rename) + A2/A3/A5 (DX + reference for in-flight tasks) — five engineering days total, no new product surface, fits v1.0 polish theme. Long-term: when subscription tier opens, B1 → (B4+B5+B6) → (B2+B3+B11) is the sequencing — encryption first, Mac-AI cleanup as the free-on-Mac wedge, then the Better-Auth subscription cluster. Skip everything in C unless its named flip condition triggers.
