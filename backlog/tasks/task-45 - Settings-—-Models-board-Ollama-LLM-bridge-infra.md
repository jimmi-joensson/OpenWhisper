---
id: TASK-45
title: Settings — Models board + Ollama LLM bridge infra
status: Won't Do
assignee: []
created_date: '2026-04-26 20:58'
updated_date: '2026-04-30 16:35'
labels:
  - tauri
  - settings
  - llm
  - ollama
  - cleanup
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Prepare BYO-LLM infrastructure (Ollama bridge) and ship the Settings → Models board UI per the design handoff (`OpenWhisper Design System.html` → SettingsModelsBoard, screens.jsx:480-637). This is **infra prep + UI shell only** — no model gets downloaded and no cleanup runs unless the user manually points at their own running Ollama. Keeps disk/RAM cost at zero by default. TASK-17 then becomes the follow-up that ships a default model + first-run UX + cloud providers.

## Scope

### Tauri UI (new Settings surface)
- First Settings window for the Tauri shell. New components dir `apps/tauri/src/components/settings/`.
- Port `SettingsModelsBoard` pixel-perfect from the design handoff:
  - Sidebar layout, Models pane active.
  - Column header (Model / Type / Speed / Accuracy / Size).
  - Rows: Parakeet (installed, NVIDIA green tile), Parakeet Multilingual (placeholder "downloading" demo or hidden until TASK-15 lands — confirm), Ollama bridge, LM Studio bridge.
  - Right-edge controls per state: trash (installed) / download (available) / ProgressRing (downloading) / BridgeGlyph (bridge → "Connect…").
  - 22px provider tiles, two 5-segment Speed/Accuracy bars, lock/star glyphs, NVIDIA `n` mark, Ollama llama silhouette.
  - Storage card: Disk + Memory stats, models-folder path code, "Show in Finder" button.
- Reuse existing `tokens.css` palette; do not introduce new design tokens.
- Entry point: tray menu `Settings…` item (and `⌘,` accelerator on macOS). New Tauri `WebviewWindow` (design uses `ScreenFrame width=840` — separate window, not a route in main shell).

### Settings persistence
- New Tauri commands `settings_get` / `settings_set` exposing a typed JSON blob.
- Persisted at `~/Library/Application Support/OpenWhisper/settings.json` (mirrors models-folder convention surfaced in the design).
- Initial schema (extensible):
  - `cleanup.enabled: bool` (default false)
  - `cleanup.backend: "ollama" | "lmstudio"` (default "ollama")
  - `cleanup.model: string` (default empty — user picks)
  - `cleanup.endpoint: string` (default "http://localhost:11434")
  - `cleanup.language: string | null` (default null — falls back to TranscriptProcessor.detectLang)
- No surface for sampling params (temp/top_p/top_k); hardcode `temp 0.2`, `top_p 0.9` in core.

### "Connect…" sheet (Ollama row)
- Sheet/dialog opened by clicking BridgeGlyph on the Ollama row.
- Inputs: endpoint URL (default `http://localhost:11434`).
- On "Test connection" → `GET {endpoint}/api/tags` → populate model dropdown from response.
- "Save" → persist `cleanup.{enabled:true, backend:"ollama", model, endpoint}`.
- "Disconnect" → `cleanup.enabled=false`, keep model/endpoint for re-enable.
- LM Studio row: same `BridgeGlyph` action but the sheet shows "coming soon" — visual placeholder per design intent.

### Rust core — new `cleanup` module
- `core/src/cleanup/mod.rs` — trait `Cleanup { fn run(&self, raw: &str, lang: Option<&str>) -> Result<String, CleanupError>; }`.
- `core/src/cleanup/ollama.rs` — HTTP client (no SDK; small `reqwest`/`ureq` call against `POST {endpoint}/v1/chat/completions` — OpenAI-compat path, simpler than `/api/chat`). Timeout: 2.5s.
- `core/src/cleanup/prompt.rs` — system prompt template per brief ("ALLOWED edits / FORBIDDEN edits / Output only the cleaned transcript"). Language hint substituted into `{language_or_unknown}`.
- Always compiled; no-ops when settings flag is off (avoids cargo feature complexity).

### Dictation pipeline hook
- `apps/tauri/src-tauri/src/lib.rs:122` — after `transcript::process(&res.text)`, before `dictation::dictation_deliver_transcript`:
  - If `cleanup.enabled` → run `Cleanup::run(rule_cleaned, lang)` with 2.5s timeout.
  - On Ok → deliver cleaned, stash raw alongside.
  - On timeout / err → deliver rule-cleaned (current behavior), surface inline warning in pill ("Cleanup unavailable — falling back").
- Extend `core::dictation::DictationSnapshot` with `raw_transcript: String` so both raw + cleaned are accessible to the shell.
- `core/src/dictation.rs` keeps the orchestration (per `feedback_rust_core_orchestration.md`); shells just plumb the IPC.

## Out of scope (deferred)
- Qwen3-4B-Instruct-2507 as bundled default (no shipping weights, no `ollama pull` UX, no first-run prompt) → folded into TASK-17.
- Cloud providers (OpenAI/Anthropic) + Keychain key storage → TASK-17.
- Multi-language cleanup fixtures (Danish, Latvian, German) → TASK-17.
- Streaming cleanup → future.
- LM Studio actual integration → future.
- CLI flags (Tauri shell has no CLI surface).

## Open questions to confirm in PR
1. Settings = separate `WebviewWindow` (matches design `ScreenFrame width=840`) or route in main shell? Recommend: separate window.
2. Persist `raw_transcript` to disk for audit, or memory-only in `DictationSnapshot`? Recommend: memory-only for v1; disk persistence is its own task.
3. Parakeet Multilingual row — show as `downloading` demo (per design) or hide until TASK-15 actually wires the download? Recommend: hide until real, to avoid lying UI.

## References
- Design handoff: `/tmp/ow_design/openwhisper/project/screens.jsx:480-637` (SettingsModelsBoard + ModelRow + ModelTile + glyphs).
- Existing dictation hook site: `apps/tauri/src-tauri/src/lib.rs:122`.
- Rule-cleaning module that this composes after: `core/src/transcript.rs`.
- Related task (broader cleanup feature, depends on this): TASK-17.
- Project memory on Rust orchestration: `feedback_rust_core_orchestration.md`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Settings window opens via tray menu and ⌘, on macOS; Models board renders pixel-perfect vs SettingsModelsBoard in the design handoff
- [ ] #2 settings_get / settings_set Tauri commands persist cleanup.{enabled,backend,model,endpoint,language} to ~/Library/Application Support/OpenWhisper/settings.json
- [ ] #3 Ollama Connect… sheet probes GET {endpoint}/api/tags and lists real installed models from the user's Ollama instance
- [ ] #4 With cleanup.enabled=false (default): zero behavior change vs current main, zero network traffic to localhost:11434
- [ ] #5 With cleanup.enabled=true and a valid Ollama endpoint+model: dictate via pnpm dev:tauri yields a cleaned transcript injected, raw transcript preserved on DictationSnapshot.raw_transcript
- [ ] #6 Timeout (>2.5s) or connection refused falls back to rule-cleaned text and surfaces an inline warning in the pill — never blocks paste
- [ ] #7 core/src/cleanup module compiles and unit-tests cover prompt rendering + Ollama HTTP request shape (mocked transport)
- [ ] #8 cargo check clean from apps/tauri/src-tauri and core
- [ ] #9 pnpm test:ui green from apps/tauri; new Playwright spec covers (a) Models board renders Parakeet + Ollama bridge rows, (b) Connect… sheet probes endpoint and populates model dropdown
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review as Won't Do. Post-v0.4.0 priorities reset; Models board + Ollama bridge will be re-planned from current state if/when revisited.
<!-- SECTION:FINAL_SUMMARY:END -->
