---
id: doc-35
title: OpenWhispr comparison — feature-by-feature audit
type: research
created_date: '2026-05-05 05:08'
status: research
related:
  - milestones/m-1
  - tasks/task-85
sources:
  - https://github.com/OpenWhispr/openwhispr (v1.7.0, MIT, 2.9k★, pushed 2026-05-04)
---

# OpenWhispr comparison — feature-by-feature audit

Audit of [`OpenWhispr/openwhispr`](https://github.com/OpenWhispr/openwhispr) against OpenWhisper. Same-name collision driving TASK-85 (rename sweep) — also a useful product foil. Compared at v1.7.0 (released 2026-05-04, last push 2026-05-04) against `main` of OpenWhisper as of 2026-05-05.

Captured to defer action — nothing here is queued for v1.0. v1.0 milestone is polish + OSS readiness, not new product surface. Revisit before v1.1 scoping.

## Architectural delta

| Axis | OpenWhisper | OpenWhispr |
|---|---|---|
| Shell | Tauri 2 + Rust core (`core/` crate + `apps/tauri/`) | Electron 41 (single project, JS/TS hybrid; `main.js` 51 KB) |
| Renderer | React 19 + TS, Tailwind v4 (Vite) | React 19 + TS, Tailwind v4, shadcn/ui (Vite) |
| Recognizer | FluidAudio (Mac/ANE) + sherpa-onnx (Win/CPU) behind trait | whisper.cpp + sherpa-onnx (Parakeet) — sidecar binaries downloaded at build time |
| Local LLM | TASK-63 in flight (llama-cpp-2 in-process) | llama-server sidecar binary, Vulkan acceleration on Win, GGUF download infra |
| Languages shipped | EN + DA filler register | 9 i18n locales (de/en/es/fr/it/ja/pt/ru/zh-CN/zh-TW), ESLint `check-i18n.js` gate |
| OS coverage | Mac + Win | Mac + Win + Linux (AppImage/deb/rpm + GNOME/KDE/Hyprland/wlroots paths) |
| Surface area | dictation only | dictation + notes + folders + chat + AI agent + meeting transcription + speaker diarization + semantic search + cloud sync + MCP server + public REST API |
| Storage | hand-rolled JSON settings | better-sqlite3 + Qdrant sidecar (vector index) + cloud Postgres (Neon, optional) |
| Auth | none | Google/Apple/Microsoft/email via Better Auth on `auth.openwhispr.com`, OS-keychain bearer tokens |
| Native code | Rust (workspace) + Swift FFI (mic/globe) | C (Win key/paste/mic listeners), Swift (mac mic/globe/audio-tap), Python (Win media-control sidecar) |
| Distribution | DMG + MSI direct | DMG (arm64+x64) + EXE + AppImage + deb + rpm |
| Org/biz | solo OSS, MIT | OpenWhispr/Gizmo Labs Inc., MIT, Neon-sponsored, Cloud product, sign-up flow, referral program, usage tier UI |

OpenWhispr is **product-suite scope** (notes/agents/meetings/dictation) running on Electron + sidecars + cloud backend. OpenWhisper today is **dictation-only**, Rust core, Tauri shell, Mac+Win, no account — but a future subscription tier is in scope (BYO-cloud → hosted-cloud upgrade path). That makes OpenWhispr's account/auth/sync/billing surface a relevant reference, not just a "wrong-fit" foil. Local-first stays the default forever; cloud is additive.

## Naming collision (TASK-85)

OpenWhispr predates OpenWhisper on GitHub (created 2025-06-19, 2.9k★, registered npm/cask/winget/MCP packages, owns `openwhispr.com` + `docs.openwhispr.com` + `support@openwhispr.com` + `openwhispr://` URL scheme). Confusion is unavoidable on the current name. TASK-85 already exists; this audit confirms scope. See TASK-85.2 (external namespace reservation) — `OpenWhispr` org is taken on GitHub, npm name `open-whispr` is taken, and they hold the protocol scheme.

## Feature parity matrix

| Feature | OpenWhisper | OpenWhispr | Notes |
|---|---|---|---|
| Toggle hotkey | ✅ | ✅ | OW has ≥4 slots: dictation/agent/meeting/cancel; GNOME-native gsettings routing for `agent`+`meeting` |
| Push-to-talk | ❌ | ✅ Win low-level kbd hook + Linux native | Win `windows-key-listener.c`, Mac `globe-listener.swift` |
| Cancel hotkey | ✅ | ✅ | |
| Right-side modifier hotkey | ❌ | ✅ regex-routed via native listener | bypasses globalShortcut for RightCtrl/RightAlt etc. |
| Modifier-only combo (e.g. Control+Super) | partial | ✅ | bypasses globalShortcut on Win, uses native hook |
| Fallback hotkey on registration fail | ❌ | ✅ F8/F9/Ctrl+Shift+Space | |
| VAD | TASK-14 (Silero) | localSpeechGate + acoustic gate (meeting echo-leak detector w/ tests) | |
| Filler strip | EN+DA, æ/ø/å lang-detect | unclear (transcript text normalizer w/ low-signal token set) | |
| Custom vocabulary | ❌ TASK-10 Won't Do | ✅ `customDictionary` setting + `DictionaryView.tsx` (flat list, no fuzzy) | see §"TASK-10 reopen reference" |
| LLM cleanup | ❌ TASK-17 Won't Do | ✅ per-scope (dictation/agent/note/chat) cloud + self-hosted + bundled llama-server | see §"TASK-17 reopen reference" |
| BYO-Ollama / LLM bridge | TASK-45 Won't Do | ✅ "Self-Hosted" card + URL/API-key/model picker; bundles llama-server (Vulkan) | see §"TASK-45 reopen reference" |
| Per-scope thinking-mode toggle | ❌ | ✅ "show thinking" switch per scope (default suppressed for dictation) | for o-series/DeepSeek-R1/Qwen-think |
| Inference provider abstraction | none (one engine, one model) | `inferenceProviders/` { openai, anthropic, gemini, groq, lan, local, openwhispr, enterprise } | |
| Audio ducking | ✅ SMTC pause + BT mono-blip mask | ✅ Win GSMTC via Python WinRT sidecar w/ media-key tap fallback | OW pause is more aggressive |
| First-run model download UX | TASK-9 Won't Do | ✅ resume-on-stall + retry+backoff + 3xx redirect follow + proxy-aware + sentinel-idempotent | see §"TASK-9 reopen reference" |
| Pill/overlay | ✅ follow-active-screen | ✅ DictationWidget + meeting-recording floating pill | |
| Tray | ✅ Mac+Win | ✅ + menuManager | |
| Single-instance | TASK-37 | implicit (Electron-default) | |
| CLI | TASK-81 (separate binary) | `cliBridge.js` + `CliIntegrationCard.tsx` (settings UI) | |
| History DB | ❌ | ✅ better-sqlite3 + cloud sync | privacy/scope concern |
| Notes / folders / semantic search | ❌ | ✅ Qdrant + MiniLM embeddings + FTS5 fallback | out of OW scope |
| Meeting transcription / diarization | ❌ | ✅ Zoom/Teams/FaceTime auto-detect + AssemblyAI/Deepgram/OpenAI Realtime + on-device speaker fingerprints | out of OW scope |
| AI agent (named voice assistant) | ❌ | ✅ AgentChat/AgentInput/AgentOverlay + tool registry | out of OW scope |
| Tool registry (notes/calendar/clipboard/web search) | ❌ | ✅ pluggable, gcal-gated, sign-in-gated | out of OW scope |
| Public REST API + MCP server | ❌ | ✅ documented at `docs.openwhispr.com/api/overview` | out of OW scope |
| Cloud sync | ❌ | ✅ Postgres (Neon) | out of OW scope |
| Auth (Google/Apple/MS/email) | ❌ | ✅ Better Auth, bearer tokens in OS keychain | out of OW scope |
| Referral program / usage tier UI | ❌ | ✅ ReferralModal/ReferralDashboard/UsageDisplay/UpgradePrompt | out of OW scope |
| API keys encrypted at rest | partial (config-blob plaintext) | ✅ Electron `safeStorage` (Keychain/DPAPI/libsecret) per-key, one-time silent migration w/ sentinel | direct copy candidate |
| Linux typing-tool | N/A (deferred) | xdotool + wtype + ydotool + native XTest binary, compositor-aware fallbacks | |
| Linux PTT | N/A | ✅ + setup-info card | |
| Auto-update | TASK-67 (Sparkle/tauri-plugin-updater) | `updater.js` + UpdateNotificationOverlay | |
| Crash reporting | TASK-78 in flight | onnx-worker stderr + onnx-worker.log + 5x respawn cap → degrade | partial parity |
| Sidecar lifecycle (clean shutdown + stale-process reaper) | N/A (in-process) | sidecarPidFile + sidecarReaper + sidecarRegistry | useful pattern if TASK-63 ever externalises llama |
| ONNX out-of-process worker | N/A | ✅ `onnxWorkerClient.js` → `workers/onnxWorker.js`, native crash isolation | direct relevance to TASK-62/63 |
| i18n | EN-only | 9 locales + `check-i18n.js` ESLint gate | |
| Tests | Playwright (UI) + Rust unit | helpers tests only (jest-style: localSpeechGate, meetingEchoLeakDetector, transcriptText) | OW has stronger E2E story |
| Theme / typography polish | identity tokens, BT-aware audio | shadcn/ui defaults | OW has more product polish |
| Repo size | small workspace | 26.6 MB, 51 KB main.js, 87 KB database.js | reflects scope delta |
| Stars / forks | (early) | 2895★ / 404 forks | one year head-start, broader product |

## TASK-9 reopen reference (first-run model download UX)

**Files:** `scripts/download-whisper-cpp.js`, `download-llama-server.js`, `download-sherpa-onnx.js`, `download-meeting-aec-helper.js`, `download-qdrant.js`, `download-minilm.js`, `download-diarization-models.js`, `src/helpers/downloadUtils.js`. All MIT.

**Pattern** — common `downloadUtils` (`downloadFile`, `createDownloadSignal`, `cleanupStaleDownloads`, `checkDiskSpace`) shared by every artifact downloader. From the v1.7 changelog (2026-04-30):

- 3xx redirect follow on HuggingFace + GitHub Releases (regression that the manual redirect handler aborted before follow — fixed and shared across all downloaders).
- Resume on stall.
- Retry with exponential backoff.
- Proxy-aware.
- Sentinel files make migration / partial-failure idempotent.
- "Stuck server is fully stopped before any re-download attempt, so partial files don't get clobbered mid-write."
- Disk-space precheck.

**Why TASK-9 was Won't-Do**: model bundling on Mac (artifact in DMG) sidesteps first-run download — the Mac Parakeet artifact is bundled. **Why this still matters**: Windows ships sherpa-onnx separately and we may eventually unbundle Mac artifacts to keep DMG size down, or add a second model (TASK-63 Qwen GGUF). Reference is dual-use: applies to TASK-63's GGUF download path too.

**Re-evaluation trigger**: TASK-63 (GGUF Qwen download) needs exactly this download infra. Worth porting `downloadUtils` shape into Rust (`reqwest` + range requests + sentinel + retry). ~150 LOC equivalent.

## TASK-10 reopen reference (custom vocabulary)

**Files:** `src/components/DictionaryView.tsx` (8.9 KB), `src/hooks/useSettings.ts` (`customDictionary`/`setCustomDictionary`).

**Approach** — flat list-of-strings UX, no fuzzy match. User types comma-separated words → stored in settings → propagated to recognizer pipeline (and prompt-injected to cleanup LLM as bias hint, presumably). The agent name itself is a non-removable entry (`if (word === agentName) return;`).

This is **less sophisticated than Handy's Levenshtein/Soundex/n-gram post-pass** (see doc-34 §TASK-10). OpenWhispr appears to bias by feeding the dictionary into the LLM cleanup prompt as context, not by doing a recognizer-output post-pass. Effective only when LLM cleanup is on.

**Why TASK-10 was Won't-Do**: Parakeet has no native biasing. **Why this is a weaker reference than Handy's**: OpenWhispr leans on its bundled LLM cleanup pass to absorb vocab corrections, which OW doesn't have (TASK-17 also Won't-Do). For a useful OW reopen, Handy's `audio_toolkit/text.rs` is the better source. OpenWhispr's `DictionaryView.tsx` is a useful UX reference for the settings surface only (empty state, comma-paste add, single-word remove with pinned entries).

**Re-evaluation trigger**: combined with TASK-17 reopen — if local LLM cleanup ever lands (TASK-63), then a flat dictionary fed into the cleanup prompt is the cheap path; otherwise Handy's fuzzy post-pass remains the right reference.

## TASK-17 reopen reference (LLM cleanup pass)

**Files:** `src/services/ai/inferenceProviders/{openai,anthropic,gemini,groq,lan,local,openwhispr,enterprise}.ts`, `services/BaseReasoningService.ts`, `LocalReasoningService.ts`, `ReasoningService.ts`, `services/localReasoningBridge.js`, `services/ai/openaiBase.ts`, `services/ai/thinkingSuppression.ts`, `helpers/llamaServer.js`, `helpers/llamaCppInstaller.ts`, `helpers/llamaVulkanManager.js`. Multiple thousand LOC, MIT.

**Provider model** — per-scope provider config: separate provider/model picks for **dictation cleanup**, **agent**, **note formatting**, **chat**. Empty agent setting links to cleanup model with one click (UX dance avoided). UI lives under `src/components/settings/{ChatAgentSettings,DictationAgentSettings,InferenceConfigEditor}.tsx`.

**Cloud providers shipped** — OpenAI / Anthropic / Gemini / Groq / Custom (OpenAI-compatible base URL). Enterprise providers (AWS/Azure/Vertex) gated separately. Custom URL has known-non-OpenAI guardrails (`api.groq.com` / `api.anthropic.com` / `generativelanguage.googleapis.com` get rejected from the OpenAI-base resolver to prevent footgun config).

**Self-hosted card** — URL + API key + model picker (parity with Cloud → Custom). Help text differentiated: Cloud → Custom = OpenRouter/Together; Self-Hosted = OpenAI-compatible local servers (Ollama, LM Studio, vLLM, llama-server).

**Auth + storage** — API keys stored encrypted via Electron `safeStorage` (Keychain/DPAPI/libsecret) — see `secretCrypto.js`, `tokenStore.js`. One-time silent migration with sentinel + round-trip verification, idempotent + retryable on partial failure. Plaintext fallback only on Linux without keyring.

**Bundled local LLM** — `llama-server` binary downloaded at build time (`download:llama-server`, `download:llama-server:all` for cross-platform), Vulkan acceleration on Win (`llamaVulkanManager.js`). Long startup window for slow Vulkan init; stuck server fully stopped before re-download to avoid partial-file clobber. Lifecycle managed via `sidecarPidFile` + `sidecarReaper` + `sidecarRegistry` — survives crashes, reaped on next launch.

**Per-scope thinking-mode** — `thinkingSuppression.ts` strips reasoning blocks per scope. Dictation defaults suppressed (snappy). Toggle per scope to expose for o-series / DeepSeek-R1 / Qwen-think models.

**Re-evaluation trigger** — closer to TASK-63 (in-process Qwen via llama-cpp-2) than TASK-17. OW's TASK-63 is making the right call architecturally (in-process, no sidecar reaper bookkeeping) — but the **inferenceProviders/ abstraction shape** is a clean reference for ever opening up to BYO-cloud later. The **API-key-encryption-at-rest** pattern is a direct copy candidate for TASK-46 / settings.rs hardening regardless of LLM scope.

## TASK-45 reopen reference (Models board + Ollama bridge)

**Files:** `src/components/settings/InferenceConfigEditor.tsx`, `src/services/ai/inferenceProviders/{lan,local}.ts`, `src/components/SelfHostedPanel.tsx`, `src/components/OpenAICompatiblePanel.tsx`.

**Two-card UX** — Cloud → Custom (OpenRouter/Together at hosted base URLs) and Self-Hosted (OpenAI-compatible servers on local network). Distinct help text per card, same underlying provider config shape.

**LAN provider** (`lan.ts`) is specifically for "another machine on my network running llama-server / Ollama" — separate from local-bundled `local.ts`. This three-way split (cloud / lan / local-bundled) is a stronger UX shape than what TASK-45 originally proposed (single Ollama URL field).

**Insecure-endpoint allow-list** — HTTPS required *unless* hostname is local network (private IP / `.local` / `localhost`). Lets users point at LAN llama-server without forcing TLS, while blocking accidental plaintext-key leakage to public hosts.

**Re-evaluation trigger** — same as TASK-17 above: TASK-63 is the in-process answer. If TASK-45 ever revives (BYO-cloud), the cloud/lan/local-bundled three-way split is the right shape. UI and provider abstraction are copyable.

## TASK-81 cross-reference (CLI + IPC integration)

**Files:** `src/helpers/cliBridge.js`, `src/components/CliIntegrationCard.tsx`.

OW's TASK-81 plans a separate `cli/` workspace member (clap parser scaffold, headless transcribe). OpenWhispr instead has the running app expose itself to a CLI bridge — closer in shape to Handy's `arg-pipe` remote control (see doc-34) than to a standalone binary.

Less directly applicable than Handy's `transcription_coordinator.rs`, but the `CliIntegrationCard.tsx` settings UX (shows "your CLI is installed, here's what you can do") is a useful reference for TASK-81.10 (Tauri commands surface for CLI).

## TASK-78 cross-reference (crash reporting)

**Files:** `src/helpers/onnxWorkerClient.js`, `src/workers/onnxWorker.js`, `helpers/sidecarReaper.js`.

OW's TASK-78 plans a Rust panic hook + on-disk dump + opt-in upload. OpenWhispr handles native crashes by **isolating to a worker process** (`onnxWorker.js`) — `bad_alloc` from ONNX runtime confines to the worker, parent rejects in-flight requests and respawns with backoff (capped at 5 attempts → degrades to FTS5 keyword search). stderr + `onnx-worker.log` capture the cause.

This is a different shape (worker isolation vs panic hook), and it only helps for the specific class of native crashes that are confined to the recognizer. For OW's mostly-in-process Rust workspace, TASK-78's panic-hook approach is correct. The reference is useful **only if** TASK-63 or TASK-62 ever wraps a flaky native dep — then worker-process isolation becomes the right pattern.

## Recommendations bucket

### Strongly relevant — fits v1.0 polish/hardening theme

1. **API-key encryption at rest** (`secretCrypto.js` pattern) — Electron `safeStorage` is `keyring` crate equivalent on Rust (Mac Keychain / Win DPAPI / Linux libsecret). Currently OW stores API keys in plaintext config (or has none — verify TASK-45 won't-do scope). One-time silent migration + sentinel + round-trip-verify is a clean pattern. Worth a v1.0 task if any secrets land before v1.0; otherwise queue for whenever TASK-17/45/63-cloud ever land.
2. **Download infrastructure pattern** (`downloadUtils.js`) — TASK-63.2 (GGUF download) directly benefits. 3xx redirect follow + resume + retry+backoff + proxy-aware + sentinel-idempotent is the table-stakes shape OW will need anyway.
3. **TASK-85 confirmed scope** — name collision is real and unavoidable (2.9k★, 1-year head-start, owns domain/protocol/npm/MCP namespaces). TASK-85 should not be deprioritized.

### v1.1+ candidates — would revive de-scoped tasks

4. **Inference provider abstraction shape** (`inferenceProviders/`) — if TASK-17 ever revives for BYO-cloud, copy the cloud/lan/local-bundled three-way split rather than starting from a single Ollama URL field. UI references: `SelfHostedPanel.tsx`, `OpenAICompatiblePanel.tsx`, `InferenceConfigEditor.tsx`.
5. **Per-scope thinking-mode suppression** — if TASK-63 ever exposes user-pickable models (Qwen-think etc.), the per-scope "show thinking" toggle is a clean UX. `thinkingSuppression.ts` is small.
6. **DictionaryView UX** — empty-state + comma-paste add + pinned-entry pattern. Useful settings surface reference if TASK-10 ever revives (combined with Handy's recognizer post-pass for the actual matching).
7. **GNOME-native gsettings hotkey routing** for non-temporary slots — if Linux ever ships, `GNOME_NATIVE_SLOTS` pattern (only `agent`/`meeting`-style persistent slots use gsettings; `cancel`-style temporary slots stay on globalShortcut) is the right architecture. Linux is post-v1.1 anyway.
8. **Right-side modifier hotkey routing** — regex-detect `Right(Control|Alt|...)` and route to native listener instead of globalShortcut. Pairs with Handy's push-to-talk per-binding (doc-34 #7).

### v2.0 candidates — relevant to future hosted-cloud / subscription tier

OW intends to add hosted-cloud functionality and a subscription model on top of the local-first core (BYO-keys stays free; hosted-cloud upgrade is the paid path). OpenWhispr has already built most of this scaffolding — these are reference references for whenever that scope opens up:

9. **Better Auth + bearer-token-in-OS-keychain** — sign-in flow at `auth.openwhispr.com` (Google/Apple/Microsoft/email), bearer tokens stored via `safeStorage` (Keychain/DPAPI/libsecret). Survives renderer crashes + session resets. `VITE_AUTH_URL` lets self-hosters point at their own server. Direct copy candidate for the auth shape if/when OW launches a hosted tier — Better Auth is open source and the `auth.openwhispr.com` pattern is reproducible.
10. **OAuth callback via custom URL scheme** (`openwhispr://`) with disable-with-tooltip when OS hasn't registered the handler — UX guard against stuck OAuth flows.
11. **Cloud sync via Postgres (Neon)** — Better-than-most reference for the *shape* of "local-first SQLite + opt-in cloud sync to Postgres." Nullable columns + optional fields server-side so older clients keep working. Per-record migrations idempotent. Conflict-resolution model worth studying when OW reaches that point.
12. **Per-scope "where does this run" picker** — already covered in #4 above, but doubles as a billing-surface primitive: cleanup-via-cloud (paid) vs cleanup-via-local-bundled (free) is the natural upgrade-prompt point.
13. **Usage / referral / upgrade UI surface** — `UsageDisplay.tsx`, `UpgradePrompt.tsx`, `ReferralModal.tsx`, `ReferralDashboard.tsx`. Self-contained components; useful as visual+UX references for designing OW's subscription surface (don't necessarily copy — but skip the blank-page problem).
14. **Public REST API + MCP server** (`docs.openwhispr.com/api/overview`) — once a hosted backend exists, exposing it as a public API + MCP server is table-stakes for power-user/agent-integration positioning. Worth keeping in mind when designing the cloud sync schema so the API doesn't bolt on awkwardly later.
15. **Streaming-provider abstraction** for cloud transcription (AssemblyAI / Deepgram / OpenAI Realtime) — only relevant if OW ever offers paid hosted-cloud transcription as an upgrade from local Parakeet (e.g. for ultra-low-latency or 100+ language support). Three-way pluggable shape is the same pattern as #4 — apply consistently.
16. **API-key encryption at rest** (already #1) — becomes load-bearing when paid keys + per-user secrets multiply.

### Skip / wrong fit

- **Notes / folders / semantic search** — different product surface; would dilute the "open Superwhisper alternative" positioning even with cloud added. Reconsider only if subscription tier wants a Granola-style notes adjacency.
- **Meeting transcription / live diarization / Zoom-Teams-FaceTime auto-detect** — out of scope even with cloud. Would need entirely different audio capture (system-audio tap, multi-stream merge, AEC). Reconsider only if subscription tier explicitly targets the meeting-bot/Granola/Otter category.
- **AI agent / tool registry / named voice assistant** — out of scope even with cloud; conflicts with the dictation-only positioning.
- **better-sqlite3 history DB on disk by default** — privacy concern. Cloud sync history is fine *if opt-in default-off* and tied to sign-in.
- **Electron migration** — dead-end. Tauri picked deliberately for native footprint + Rust core. Cloud features layer on top of Rust core fine.
- **Bundled llama-server sidecar** — TASK-63 already picked in-process (llama-cpp-2). Don't regress to sidecar bookkeeping unless we hit a wall.
- **9-locale i18n** — premature until product surface stabilises (same as Handy finding).

## Files to study before re-opening tasks

- `scripts/download-*.js` + `src/helpers/downloadUtils.js` — TASK-63.2 download infra reference
- `src/helpers/secretCrypto.js` + `tokenStore.js` — API-key encryption at rest (load-bearing when subscription tier ships)
- `src/services/ai/inferenceProviders/` + `services/ai/openaiBase.ts` — provider abstraction shape if TASK-17 revives
- `src/components/settings/InferenceConfigEditor.tsx` + `SelfHostedPanel.tsx` + `OpenAICompatiblePanel.tsx` — settings UI for BYO-cloud (and for upgrade-prompt placement)
- `src/components/DictionaryView.tsx` — vocab settings UX (combine w/ Handy's recognizer post-pass for the engine)
- `src/services/ai/thinkingSuppression.ts` — reasoning-block strip per scope
- `src/helpers/hotkeyManager.js` — modifier-only + right-side-modifier routing patterns
- `src/helpers/onnxWorkerClient.js` + `workers/onnxWorker.js` — worker-process isolation reference (only if TASK-63 wraps flaky native dep)
- `src/helpers/sidecarPidFile.js` + `sidecarReaper.js` + `sidecarRegistry.js` — sidecar lifecycle (only if we ever externalise an engine)
- **Subscription-tier references** (study before any paid-cloud work):
  - Auth: `useAuth.ts`, `tokenStore.js`, `services/cloudApi.ts`, `AuthenticationStep.tsx`, `EmailVerificationStep.tsx`, `ForgotPasswordView.tsx` — Better Auth client integration + bearer-token-in-keychain
  - Sync: `services/SyncService.ts`, `services/{Notes,Folders,Conversations,ApiKeys,Transcriptions}Service.ts` — local-first SQLite + opt-in Postgres sync shape, nullable-column compat
  - Billing-surface: `UsageDisplay.tsx`, `UpgradePrompt.tsx`, `ReferralModal.tsx`, `ReferralDashboard.tsx`, `useUsage.ts` — upgrade-prompt UX, referral mechanics

## One-line take

OpenWhispr is the product OW *might* grow into the cloud half of (subscription tier on top of local-first core), built on Electron + Postgres + Better Auth. Today it's mostly a foil: notes/agents/meetings are **don't-want** (would erase the dictation-only positioning) — but auth, sync, billing surface, and inference-provider abstraction become directly relevant the day a hosted-cloud upgrade lands. Steal four patterns regardless: download infra (TASK-63.2), API-key encryption at rest (whenever secrets land), inference-provider three-way split (TASK-17/45 if revived), and Better Auth + bearer-in-keychain (whenever subscription tier opens). And ship TASK-85: the name collision is real.
