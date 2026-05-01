# LLM-based disfluency cleanup — implementation plan

**Backlog parent:** TASK-63
**Spec:** `backlog/docs/specs/2026-05-01-llm-disfluency-cleanup.md`
**Date:** 2026-05-01
**Depends on:** TASK-62 (model lifecycle foundation)

Each `### Task N:` heading maps 1:1 to a Backlog subtask `TASK-63.N`. Ordering: Tasks 1–4 build the engine bottom-up. Task 5 wraps it in the lifecycle abstraction (requires TASK-62 done). Tasks 6 + 7 wire it into the dictation flow. Tasks 8 + 9 are the surfacing layer. Task 10 is test coverage. Task 11 is end-to-end smoke.

---

### Task 1: `llama-cpp-2` dependency + GGUF infrastructure

**Goal.** Add `llama-cpp-2` to `core/Cargo.toml` with platform-conditional features (Metal on Mac, Vulkan on Windows). Add a GGUF-path resolver that returns the canonical model location per platform.

**Files.** `core/Cargo.toml`, `core/src/cleanup/mod.rs` (new), `core/src/cleanup/paths.rs` (new), `core/src/lib.rs`.

**Steps.**

1. Add to `core/Cargo.toml`:
   ```toml
   [target.'cfg(target_os = "macos")'.dependencies]
   llama-cpp-2 = { version = "...", features = ["metal"] }

   [target.'cfg(not(target_os = "macos"))'.dependencies]
   llama-cpp-2 = { version = "...", features = ["vulkan"] }
   ```
   Pick the latest stable version; document in a comment why per-platform features are split.
2. Create `core/src/cleanup/mod.rs` exposing `pub mod paths;` and a public `CleanupEngine` trait (signatures only — implementation in Task 3).
3. Create `core/src/cleanup/paths.rs`:
   ```rust
   pub fn cleanup_models_dir() -> PathBuf { ... }  // ~/Library/Application Support/com.openwhisper.app/models on Mac, %LOCALAPPDATA%\com.openwhisper.app\models on Windows
   pub fn gguf_path(variant: &str) -> PathBuf { ... }
   ```
4. Wire `pub mod cleanup;` into `core/src/lib.rs`.
5. `cargo check -p openwhisper-core` clean on both targets. Verify the Vulkan feature builds on Windows host (or note in plan that this is verified at execution time on a Windows box).

**Outcome ACs (Backlog).**

- `llama-cpp-2` is a `core` dependency with Metal feature on Mac, Vulkan on Windows.
- `cleanup::paths::cleanup_models_dir()` returns the platform-correct directory.
- `cleanup::paths::gguf_path(variant)` returns the right path for `qwen3.5-0.8b-q4` and `qwen3.5-2b-q4`.
- `cargo check` clean on both `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`.

---

### Task 2: GGUF model download

**Goal.** Mirror the Parakeet download pattern (`core/src/recognizer/download.rs`) for the cleanup GGUF. Includes progress reporting via the existing `download_bytes_done` / `download_bytes_total` snapshot fields in `core/src/dictation.rs`.

**Files.** `core/src/cleanup/download.rs` (new), `core/src/cleanup/mod.rs`.

**Steps.**

1. Read `core/src/recognizer/download.rs` for the existing pattern: HTTPS URL, `reqwest` streaming, SHA-256 integrity check, atomic rename on completion.
2. In `core/src/cleanup/download.rs`, add:
   ```rust
   pub async fn ensure_gguf_present(variant: &str) -> Result<PathBuf, String>
   ```
   - Returns the on-disk path if file exists and SHA-256 matches expected.
   - Otherwise downloads from the variant's URL (HuggingFace mirror — Unsloth or official Qwen GGUF), with progress callbacks.
3. Hardcode known-good URLs and SHA-256s for `qwen3.5-0.8b-q4_k_m.gguf` and `qwen3.5-2b-q4_k_m.gguf` in a `MODEL_CATALOG` table. Document the source.
4. Wire progress into the dictation snapshot: on `Content-Length` received, call into `dictation::set_download_total`; on each chunk, call `dictation::add_download_bytes`. (Use existing helpers in `core/src/dictation.rs`; if they don't exist as standalone setters, add them.)
5. Unit test the catalog lookup; integration smoke is manual (real download).

**Outcome ACs (Backlog).**

- `ensure_gguf_present(variant)` is async and returns the on-disk path.
- Download progress flows into the existing dictation snapshot's `download_bytes_done` / `download_bytes_total` fields.
- Existing-file fast path skips download when SHA-256 matches.
- Hardcoded catalog covers `qwen3.5-0.8b-q4` and `qwen3.5-2b-q4` with documented source URLs.
- Manual: deleting the cached file and triggering a cleanup dictation produces a download with progress visible in the UI.

---

### Task 3: CleanupEngine trait + bare LlamaCpp implementation

**Goal.** Define the cleanup engine API; implement a bare `LlamaCppCleanup` that loads a GGUF and runs unconstrained generation against a prompt. No constraints yet — that's Task 4.

**Files.** `core/src/cleanup/mod.rs`, `core/src/cleanup/engine.rs` (new), `core/src/cleanup/prompt.rs` (new).

**Steps.**

1. In `core/src/cleanup/mod.rs`, define:
   ```rust
   #[derive(Debug, Clone)]
   pub struct CleanupHint {
       pub primary_languages: Vec<String>,
       pub aggressive: bool,
       pub detected_language: Option<String>,
   }

   pub trait CleanupEngine: Send {
       fn ensure_loaded(&mut self) -> Result<(), String>;
       fn cleanup(&mut self, text: &str, hint: &CleanupHint) -> Result<String, String>;
   }
   ```
2. In `cleanup/prompt.rs`, build the prompt from a template:
   - System: "You are a transcript cleanup assistant. Output a JSON list of delete spans only — never insert or replace."
   - User: input text + hint values.
   - Include language hint, aggressive flag's effect on soft fillers.
3. In `cleanup/engine.rs`, implement `LlamaCppCleanup`:
   - `pub fn new(gguf_path: PathBuf) -> Self`
   - `ensure_loaded()` constructs `LlamaModel` + `LlamaContext` from `llama-cpp-2`. Call `model.warmup()` (mmap + an empty forward pass).
   - `cleanup()` runs unconstrained generation with the prompt and returns the raw output. We'll add the constraint in Task 4.
4. Document explicitly in the file header: this is a **scaffolding step**; the real cleanup needs the constraint from Task 4 to be safe. Until Task 4 lands, do not call `cleanup()` from anywhere except a unit test.
5. Unit test: load a small GGUF (could mock the engine via a trait stub if a real GGUF isn't checked in), assert `cleanup()` returns a String.

**Outcome ACs (Backlog).**

- `CleanupEngine` trait with `ensure_loaded` and `cleanup` defined.
- `LlamaCppCleanup` loads a GGUF via `llama-cpp-2` and runs `mmap` + warmup forward pass.
- Bare `cleanup()` returns model output (unconstrained — flagged unsafe for production until Task 4).
- File header explicitly warns about safety until Task 4.

---

### Task 4: LLGuidance constraint + edit-list schema + applier + validator

**Goal.** Constrain the model output to a JSON edit-list schema. Validate spans. Apply edits in reverse order. Fail-closed to unchanged input on any validator error.

**Files.** `core/src/cleanup/engine.rs`, `core/src/cleanup/edits.rs` (new), `core/src/cleanup/grammar.rs` (new).

**Steps.**

1. In `cleanup/grammar.rs`, define the LLGuidance grammar (or GBNF equivalent if `llama-cpp-2` doesn't support LLGuidance directly in its current version — fall back to GBNF and document the choice). Schema:
   ```
   root := "[" edit-list "]"
   edit-list := edit ("," edit)*
   edit := '{"op":"delete","span":[' span-start "," span-end "]}'
   span-start := number
   span-end := number
   ```
2. Wire the grammar into `LlamaCppCleanup::cleanup()` — pass the grammar handle to the inference call. Verify by inspecting raw output: model emits only `[...]` JSON, no prose.
3. In `cleanup/edits.rs`, define:
   ```rust
   pub struct Edit { pub op: EditOp, pub span: (usize, usize) }
   pub enum EditOp { Delete }
   pub fn parse_edits(json: &str) -> Result<Vec<Edit>, String>
   pub fn validate_edits(edits: &[Edit], input_len: usize) -> Result<(), ValidatorError>
   pub fn apply_edits(input: &str, edits: &[Edit]) -> String
   ```
4. Validator rules:
   - All spans `0 ≤ start < end ≤ input_len`.
   - Spans don't overlap (sort by start, ensure each `start >= previous_end`).
   - Total deleted bytes ≤ 50 % of `input_len`. (Sanity guard.)
   - **Span boundaries align to UTF-8 char boundaries.** Use `str::is_char_boundary`. Reject spans that bisect a multi-byte codepoint — common for the 25 EU langs.
5. `apply_edits` sorts spans descending by start, slices and rejoins.
6. In `LlamaCppCleanup::cleanup()`, the full flow becomes:
   ```rust
   let raw = generate_with_grammar(prompt, grammar)?;
   let edits = parse_edits(&raw).map_err(|e| log::warn!("...") + return Ok(text.to_string()))?;
   validate_edits(&edits, text.len()).map_err(|e| log::warn!("...") + return Ok(text.to_string()))?;
   Ok(apply_edits(text, &edits))
   ```
   Failures return the **unchanged input** (fail-closed). Log every failure with structured context for debugging.
7. Unit tests:
   - Valid edit list → applied correctly
   - Overlapping spans → rejected
   - Out-of-bounds span → rejected
   - >50 % deletion → rejected
   - UTF-8 boundary violation (e.g. mid-codepoint in `"å"`) → rejected
   - Empty edit list → returns input unchanged
   - Malformed JSON → returns input unchanged + warn logged

**Outcome ACs (Backlog).**

- LLGuidance grammar (or GBNF fallback) constrains output to JSON delete-span schema.
- `parse_edits`, `validate_edits`, `apply_edits` exist in `cleanup/edits.rs`.
- Validator rejects overlap, out-of-bounds, >50 % deletion, UTF-8-boundary violations.
- `cleanup()` fails closed: any validator error returns the unchanged input + warn log.
- Unit tests cover all validator paths plus the happy path.

---

### Task 5: Wrap CleanupEngine in ModelHandle (uses TASK-62 abstraction)

**Goal.** Replace direct `LlamaCppCleanup` ownership with a `ModelHandle<Box<dyn CleanupEngine>>`. 60-second idle timeout. Registers itself in the lifecycle registry so the keep-warm setting reaches it.

**Files.** `core/src/cleanup/mod.rs`.

**Steps.**

1. Add `static CLEANUP_ENGINE: OnceLock<ModelHandle<Box<dyn CleanupEngine>>>` to `cleanup/mod.rs`.
2. Initializer constructs the handle with:
   - `label = "cleanup-llm"`
   - `idle_timeout = Duration::from_secs(60)`
   - Loader closure: read settings for current `model_variant`, call `ensure_gguf_present(variant).await`, then construct `LlamaCppCleanup::new(path)` and call `ensure_loaded()`.
3. Public API:
   ```rust
   pub fn cleanup_ensure_loaded() -> impl Future<Output = Result<(), String>>
   pub fn cleanup_process(text: &str, hint: &CleanupHint) -> Result<String, String>
   ```
4. The model-variant change path: when settings flip the variant, call `cleanup::reload_for_variant(new_variant)` which `unload()`s the current handle and replaces the loader closure (or stores variant in a global the loader reads).
5. Unit-test the lifecycle integration with a mock `CleanupEngine` (no real GGUF needed at this layer).

**Outcome ACs (Backlog).**

- Cleanup model loaded via `ModelHandle` with 60-s idle timeout.
- `cleanup_process(text, hint)` auto-loads if Unloaded, runs the engine, returns to Loaded with idle re-armed.
- Variant switch triggers unload + new loader closure (no stale model).
- Handle registered with lifecycle registry — "keep models warm" setting affects it.

---

### Task 6: Pre-warm trigger on PHASE_RECORDING

**Goal.** When dictation enters `PHASE_RECORDING`, fire-and-forget cleanup model load. By the time STT completes, cleanup is in Loaded.

**Files.** `core/src/dictation.rs`, `core/src/cleanup/mod.rs`.

**Steps.**

1. In `core/src/dictation.rs`, in the function that transitions to `PHASE_RECORDING` (find via `state.phase = PHASE_RECORDING` assignment), add:
   ```rust
   if settings::cleanup_enabled() {
       tokio::spawn(async move {
           let _ = openwhisper_core::cleanup::cleanup_ensure_loaded().await;
       });
   }
   ```
2. Add `pub fn cleanup_enabled() -> bool` accessor to settings (atomic flag set in Task 8).
3. The pre-warm is fire-and-forget; failures only log. Do not block recording start on cleanup load.
4. Edge case (very short dictation): when the user stops before pre-warm completes, `cleanup_process()` will await the in-flight load. This is the "<1 s dictation pays the load cost" trade-off documented in the spec.
5. Manual smoke: enable cleanup, start a long recording (>2 s), watch logs — cleanup model load completes during recording. Stop recording — cleanup runs immediately with no extra wait.

**Outcome ACs (Backlog).**

- Entering `PHASE_RECORDING` with cleanup enabled spawns a load task.
- Recording start is not blocked by cleanup load.
- Logs show cleanup model reaching Loaded state during a long recording.
- Cleanup gated on `settings::cleanup_enabled()` — pre-warm is a no-op when disabled.

---

### Task 7: Wire cleanup into transcript pipeline

**Goal.** After `transcript::process` runs the rule pass, run `cleanup::cleanup_process` if enabled. Hook into both the Mac SwiftUI shell and Tauri shell call sites — matches the TASK-43 pattern.

**Files.** `core/src/transcript.rs`, `apps/tauri/src-tauri/src/lib.rs`, `apps/macos/App/DictationService.swift`.

**Steps.**

1. Audit `core/src/transcript.rs::process` — confirm signature and call sites. Both shells call this; we want the LLM cleanup to also run for both.
2. **Decision: where does the LLM call live?** Two options:
   - (a) Add an async wrapper `pub async fn process_full(text: &str, lang_hint: &str) -> String` in `core/src/transcript.rs` that runs rules + LLM. Requires both shells to use the async path.
   - (b) Keep `transcript::process` synchronous; add a second function `cleanup::cleanup_process` and call it from each shell after the rule pass.
   Choose **(b)** — preserves the existing call sites in Mac shell and only adds the LLM step where we want it. Tauri side gets the async runtime; Mac shell already uses async via Tokio in DictationService.
3. In Tauri's `apps/tauri/src-tauri/src/lib.rs::spawn_recognizer`, after the existing `transcript::process` call, add:
   ```rust
   let cleaned = if settings::cleanup_enabled() {
       cleanup::cleanup_process(&rule_output, &hint_from_settings()).unwrap_or(rule_output)
   } else {
       rule_output
   };
   dictation_deliver_transcript(&cleaned);
   ```
4. In `apps/macos/App/DictationService.swift`, add the equivalent call via the FFI surface (a new `cleanup_process_text(text, hint)` C export needed in `core/src/ffi_c.rs`).
5. Surface `cleanup::cleanup_process` as a C-export in `core/src/ffi_c.rs` for Mac. Tauri can call the Rust API directly.
6. `cargo check` clean on both targets.
7. Manual smoke (Mac + Win): "um, like, hello, like, world" → with cleanup enabled + aggressive ON, cleanup removes both "like"s. With aggressive OFF, leaves them. Without cleanup enabled, output matches today's rule-only output.

**Outcome ACs (Backlog).**

- Cleanup runs after `transcript::process` in both Mac and Tauri shells.
- Cleanup is gated on `settings::cleanup_enabled()` and falls back to rule-pass output on any error.
- Same source of truth: `cleanup::cleanup_process` is called from both shells; no duplicated logic.
- Manual smoke confirms aggressive-on vs aggressive-off behavior.

---

### Task 8: Cleanup settings (core)

**Goal.** Add `cleanup` block to settings JSON. Atomic flags + accessor functions for hot-read paths. Tauri commands for get/set.

**Files.** `apps/tauri/src-tauri/src/settings/mod.rs`, `apps/tauri/src-tauri/src/lib.rs`.

**Steps.**

1. Add to `settings/mod.rs`:
   ```rust
   #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
   pub struct CleanupSettings {
       pub enabled: bool,                          // default false
       pub model_variant: String,                  // default "qwen3.5-0.8b-q4"
       pub aggressive: bool,                       // default false
       pub primary_languages: Vec<String>,         // default ["en"]
   }
   impl Default for CleanupSettings { ... }
   ```
   Extend `SettingsFile` with `#[serde(default)] cleanup: Option<CleanupSettings>`.
2. Add atomic flags + accessors:
   - `pub fn cleanup_enabled() -> bool`
   - `pub fn cleanup_aggressive() -> bool`
   - `pub fn cleanup_model_variant() -> String` (RwLock over String)
   - `pub fn cleanup_primary_languages() -> Vec<String>` (RwLock over Vec)
3. Tauri commands:
   - `settings_get_cleanup(app) -> CleanupSettings`
   - `settings_set_cleanup(app, value: CleanupSettings) -> Result<(), String>`
   The setter writes JSON, updates atomics. If `model_variant` changed, call `cleanup::reload_for_variant`.
4. Register both commands in `invoke_handler!`.
5. `cargo check` clean.

**Outcome ACs (Backlog).**

- `CleanupSettings` schema added with all four fields and defaults.
- Atomic accessors return current values; reads are non-blocking.
- `settings_set_cleanup` persists JSON, updates all atomics atomically, and triggers variant reload when applicable.
- Both commands registered in `invoke_handler!`.

---

### Task 9: Cleanup settings UI (Dictation pane) + pill loading placeholder

**Goal.** Add a "Cleanup" section to Settings → Dictation with the four controls. Add a placeholder loading indicator to the pill for the cold-load case (text or simple dot — real animation is TASK-64).

**Files.** `apps/tauri/src/components/dictation-pane.tsx` (or wherever the dictation settings live), `apps/tauri/src/PillOverlay.tsx`.

**Steps.**

1. Confirm the existing settings sidebar entry for dictation. If none exists, add one (the user has multiple panes already — General, Audio, Shortcuts; add Dictation).
2. Section "Cleanup" with four rows:
   - **Enable cleanup** (Switch) — gates everything below.
   - **Model size** (RadioGroup) — "Smaller, faster (0.8B)" default vs "Larger, better (2B)" — disabled when cleanup off.
   - **Aggressive cleanup** (Switch) — helper text "Removes 'like', 'you know', 'I mean' when used as filler. Off keeps your speaking style intact." — disabled when cleanup off.
   - **Primary languages** (multi-select) — chips for the 25 EU langs Parakeet v3 supports — disabled when cleanup off.
3. On mount: `invoke<CleanupSettings>('settings_get_cleanup')` → set local state.
4. On any change: optimistic update + `invoke('settings_set_cleanup', { value: newSettings })`. Revert on rejection.
5. Pill placeholder: subscribe to `model-state-changed` (from TASK-62 Task 7). When `label === "cleanup-llm"` and `state === "Loading"`, show a placeholder text "Preparing cleanup…" or a static dot in the pill. **Must be replaceable by TASK-64**'s real animation — keep the surface narrow.
6. `pnpm exec tsc` clean.

**Outcome ACs (Backlog).**

- Dictation pane shows the four cleanup controls; disabled state propagates from the master toggle.
- Settings persist on every change; UI hydrates from stored values on mount.
- Pill shows a placeholder loading indicator (text or static dot — not the final animation) when cleanup model is in Loading state.
- Placeholder lives in a single component / location so TASK-64 can swap it without touching pill internals.

---

### Task 10: Playwright spec — cleanup settings UI

**Goal.** UI half coverage for the new Dictation → Cleanup section.

**Files.** Extend `apps/tauri/tests/settings-window.spec.ts` (or new `apps/tauri/tests/cleanup-settings.spec.ts` if too long).

**Steps.**

1. Test: Dictation pane sidebar entry visible; clicking opens the pane with the Cleanup section.
2. Test: master toggle defaults OFF; flipping it enables the dependent controls.
3. Test: model variant radio renders with 0.8B selected by default; flipping to 2B records `invoke('settings_set_cleanup', { value: { ..., model_variant: 'qwen3.5-2b-q4' } })`.
4. Test: aggressive toggle defaults OFF.
5. Test: primary-languages multiselect; selecting "Danish" records the new languages array.
6. Test: hydrate-from-stored — shim returns `{ enabled: true, aggressive: true }`, UI renders correct state.
7. `pnpm test:ui` green.

**Outcome ACs (Backlog).**

- 6 Playwright assertions covering: sidebar entry, master toggle gating, variant change, aggressive toggle default, languages multiselect, hydrate-from-stored.
- `pnpm test:ui` green locally and on CI.

---

### Task 11: End-to-end smoke (Mac + Windows)

**Goal.** Manual verification on both platforms. Document results in the spec or this plan as a comment block. Latency expectations come from the conversation's research (≤500 ms target on Mac high / Win GPU; ≤1 s upper bound on Mac low / Win CPU).

**Files.** Document results in `backlog/docs/specs/2026-05-01-llm-disfluency-cleanup.md` (append a "Verified" section at the bottom).

**Steps.**

1. **Mac (M-series dev box).**
   - Fresh install + cleanup enabled → first dictation triggers GGUF download (progress visible in pill / status).
   - Dictation 1: "um, like, hello, like, world" with aggressive ON → cleanup removes the fillers.
   - Dictation 2 immediately after: same input, aggressive OFF → "like" preserved.
   - Wait 90 s after dictation 2 → Diagnostics pane shows cleanup-llm = Unloaded.
   - Dictation 3: triggers cold reload → `PHASE_LOADING_MODEL` visible briefly, cleanup completes.
   - Measure end-to-end latency for ~100-token cleanup: target ≤500 ms p95.
2. **Windows mid-tier (i7 13th gen + RTX 3060 if available).**
   - Same flow.
   - Confirm Vulkan path engages (or document fallback to CPU if no Vulkan device).
   - Latency target: ≤500 ms with GPU, ≤1.5 s on CPU-only.
3. **Disable cleanup, dictate** → output matches existing rule-pass (regression check).
4. Append observed latency numbers + any platform quirks to the spec's "Verified" section.

**Outcome ACs (Backlog).**

- Mac end-to-end smoke passes (download, two dictations with aggressive on/off, cold reload after idle, latency in range).
- Windows end-to-end smoke passes (same flow + acceleration backend confirmed).
- Disabled-cleanup regression check passes.
- Spec's "Verified" section appended with date, platforms, and observed latency numbers.

---

## Reviewer loop

Once all 11 plan tasks have matching Backlog subtasks, dispatch the plan-document-reviewer agent with the standard plan-review criteria PLUS the verbatim Backlog-enforcement fragment from `.claude/skills/writing-backlog-plans/references/plan-reviewer-addendum.md`. Address findings before handing the plan to an executor.

## Execution handoff

Order: 1 → 2 in parallel with 3 (download path is independent of engine scaffold). Then 4 (constraints + applier). Then 5 (lifecycle wrap; depends on TASK-62 done). Then 6 + 7 in parallel. Then 8. Then 9 + 10 in parallel. Then 11.

**Hard prerequisite:** TASK-62 must be Done before TASK-63.5 starts. TASK-63.1–4 can run before TASK-62 is fully done as long as TASK-62.2 (the `ModelHandle` skeleton) has landed.
