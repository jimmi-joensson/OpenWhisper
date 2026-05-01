# Model memory telemetry + lifecycle foundation — implementation plan

**Backlog parent:** TASK-62
**Spec:** `backlog/docs/specs/2026-05-01-model-lifecycle-telemetry.md`
**Date:** 2026-05-01

Each `### Task N:` heading maps 1:1 to a Backlog subtask `TASK-62.N`. Ordering: Tasks 1–3 are pure-core foundations and can run sequentially. Task 4 needs Task 3 (settings hot-reload integrates with the timer). Tasks 5 and 6 (Mac and Win wrapping) can run in parallel after 1–4 land. Tasks 7–9 are surfacing layers — 7 depends on 4–6; 8 and 9 depend on 7; 10 depends on 8 and 9.

---

### Task 1: Memory query primitive

**Goal.** Cross-platform process memory query in `core/`. Returns RSS, peak RSS, and a structured `ProcessMemory` snapshot.

**Files.** `core/src/telemetry/mod.rs` (new), `core/src/telemetry/memory.rs` (new), `core/Cargo.toml`, `core/src/lib.rs`.

**Steps.**

1. Add `sysinfo` to `core/Cargo.toml` with default features minus the ones we don't need (`disable-default-features = true`, enable `system` only).
2. Create `core/src/telemetry/` module exposing:
   ```rust
   #[derive(Debug, Clone)]
   pub struct ProcessMemory {
       pub rss_bytes: u64,
       pub peak_rss_bytes: u64,
       pub timestamp: std::time::SystemTime,
   }

   pub fn query_process_memory() -> ProcessMemory { ... }
   ```
3. Use `sysinfo::System` with `RefreshKind::new().with_memory(...)` and `Pid::from_u32(std::process::id())` to read the current process's memory.
4. Unit test in `core/src/telemetry/memory.rs`: assert `rss_bytes > 0` and `peak_rss_bytes >= rss_bytes` after a deliberate large allocation.
5. Wire `pub mod telemetry;` into `core/src/lib.rs`.
6. `cargo check -p openwhisper-core` clean. `cargo test -p openwhisper-core --lib telemetry` green.

**Outcome ACs (Backlog).**

- `ProcessMemory` type defined with `rss_bytes`, `peak_rss_bytes`, `timestamp`.
- `query_process_memory()` returns non-zero RSS on the current process.
- Unit test covers "RSS grows after allocation; peak ≥ current".
- `cargo check` and `cargo test` clean for `openwhisper-core`.

---

### Task 2: ModelHandle<T> state machine (no timer)

**Goal.** Pure state machine for load / use / unload, no async, no timer. `Unloaded → Loading → Loaded → Active`. Tested standalone with a mock loader.

**Files.** `core/src/model_lifecycle/mod.rs` (new), `core/src/lib.rs`.

**Steps.**

1. Define the state enum:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum LifecycleState { Unloaded, Loading, Loaded, Active, Releasing }
   ```
2. Define `ModelHandle<T>`:
   ```rust
   pub struct ModelHandle<T: Send + 'static> {
       state: Arc<Mutex<LifecycleState>>,
       inner: Arc<Mutex<Option<T>>>,
       loader: Arc<dyn Fn() -> Result<T, String> + Send + Sync>,
       last_load_rss_delta: Arc<Mutex<u64>>,
       label: String,  // e.g. "recognizer", "cleanup-llm"
   }
   ```
3. Implement:
   - `pub fn new<F>(label: &str, loader: F) -> Self` where `F: Fn() -> Result<T, String> + Send + Sync + 'static`
   - `pub fn load(&self) -> Result<(), String>` — single-flight, idempotent.
   - `pub fn unload(&self) -> Result<(), String>` — drops `inner`, transitions to Unloaded.
   - `pub fn use_with<R>(&self, f: impl FnOnce(&mut T) -> R) -> Result<R, String>` — auto-load if Unloaded, transitions to Active for the call, back to Loaded on return.
   - `pub fn state(&self) -> LifecycleState`
   - `pub fn current_memory_estimate(&self) -> u64` — returns the RSS delta cached at load time, or 0 if unloaded.
4. RSS delta: in `load()`, call `query_process_memory()` before invoking the loader, again after. Store `after - before` (saturating sub) in `last_load_rss_delta`. Document in module docs that this is "estimated; concurrent allocations skew the number".
5. Unit tests with a mock loader (counts calls, fails on demand):
   - load → use → unload cycle transitions states correctly
   - load is idempotent
   - use auto-loads from Unloaded
   - unload from Active is rejected (returns error; caller waits for use to finish)
   - failed loader leaves state at Unloaded with error returned
6. `cargo test -p openwhisper-core --lib model_lifecycle` green.

**Outcome ACs (Backlog).**

- `LifecycleState` and `ModelHandle<T>` defined in `core/src/model_lifecycle/mod.rs`.
- `load()`, `unload()`, `use_with()`, `state()`, `current_memory_estimate()` public on the handle.
- `current_memory_estimate()` returns RSS delta captured at the most recent `Loading → Loaded` transition.
- Unit tests cover load idempotency, auto-load on use, unload-while-active rejection, failed-loader cleanup.
- `cargo check` and `cargo test` clean.

---

### Task 3: Idle timer + auto-release

**Goal.** Background task per handle that calls `unload()` after configurable idle. Exposed via `ModelHandle::with_idle_timeout(Duration)`. Uses Tokio (already a transitive dep via `tauri`; for `core/` we adopt it directly).

**Files.** `core/src/model_lifecycle/mod.rs`, `core/Cargo.toml`.

**Steps.**

1. Add `tokio = { version = "1", features = ["rt-multi-thread", "time", "sync", "macros"] }` to `core/Cargo.toml` if not already present (verify against existing `recognizer/download.rs` async usage — likely uses reqwest's async runtime).
2. Extend `ModelHandle<T>`:
   ```rust
   idle_timeout: Arc<RwLock<Duration>>,
   timer_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
   last_used: Arc<Mutex<Instant>>,
   ```
3. New constructor `pub fn with_idle_timeout(label: &str, loader: F, idle: Duration) -> Self`.
4. After every transition into `Loaded` (post-load or post-use), spawn a Tokio task on `tokio::runtime::Handle::current()`:
   ```rust
   tokio::spawn(async move {
       loop {
           let elapsed = ...;
           let remaining = idle_timeout.read() - elapsed;
           if remaining <= Duration::ZERO { unload(); break; }
           tokio::time::sleep(remaining).await;
       }
   })
   ```
   Cancel previous timer (`abort()`) before spawning a new one.
5. New `pub fn set_idle_timeout(&self, new: Duration)` — write to `idle_timeout`, re-arms the timer immediately. (`Duration::MAX` ≈ "never" for the keep-warm setting.)
6. Test with a mock + tokio test runtime:
   - load + idle 50 ms timer → state goes to Unloaded after 50 ms
   - use during the wait extends idle window correctly
   - `set_idle_timeout(Duration::MAX)` cancels release; model stays Loaded
7. `cargo test -p openwhisper-core --lib model_lifecycle::idle` green.

**Outcome ACs (Backlog).**

- `with_idle_timeout` constructor + `set_idle_timeout` setter exist on `ModelHandle`.
- After idle expires, handle transitions Loaded → Unloaded automatically.
- Active calls cancel the timer; it re-arms on return to Loaded.
- `Duration::MAX` (the "keep warm" path) keeps the model resident indefinitely.
- Tokio runtime requirement documented in module docs.

---

### Task 4: Performance settings (keep_models_warm)

**Goal.** Add `performance.keep_models_warm` to settings. Atomic flag + hot-reload pushes the change into all live `ModelHandle`s.

**Files.** `apps/tauri/src-tauri/src/settings/mod.rs`, `apps/tauri/src-tauri/src/lib.rs`, `core/src/model_lifecycle/mod.rs`.

**Steps.**

1. In `settings/mod.rs`, add:
   ```rust
   #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
   pub struct PerformanceSettings { pub keep_models_warm: bool }
   impl Default for PerformanceSettings { fn default() -> Self { Self { keep_models_warm: false } } }
   ```
   Extend `SettingsFile` with `#[serde(default)] performance: Option<PerformanceSettings>`.
2. Add process-global `static KEEP_MODELS_WARM: AtomicBool` and `pub fn keep_models_warm() -> bool` accessor (mirror the pill-follow pattern from TASK-55.1).
3. Add Tauri commands:
   - `settings_get_performance(app) -> PerformanceSettings`
   - `settings_set_keep_models_warm(app, value: bool) -> Result<(), String>`
   The setter writes the JSON, flips the atomic, then **broadcasts** to all registered handles.
4. In `core/src/model_lifecycle/mod.rs`, add a process-global registry:
   ```rust
   static REGISTRY: OnceLock<Mutex<Vec<Weak<HandleInner>>>> = OnceLock::new();
   pub fn registered_handles() -> Vec<Arc<HandleInner>> { ... }  // upgrades weaks
   pub fn apply_keep_warm(keep_warm: bool) { ... }  // sets idle_timeout to MAX or default per handle
   ```
   Each new `ModelHandle::new` registers itself.
5. The Tauri setter calls `openwhisper_core::model_lifecycle::apply_keep_warm(value)` after writing settings.
6. Register both Tauri commands in `lib.rs::invoke_handler!`.
7. `cargo check` clean. Manual smoke: launch dev build, set `keep_models_warm=true`, verify atomic flag returns true and a registered mock handle's `idle_timeout` reads `Duration::MAX`.

**Outcome ACs (Backlog).**

- `PerformanceSettings` schema added; `keep_models_warm` defaults `false`.
- `settings_set_keep_models_warm(true)` persists JSON, flips atomic, and reconfigures every registered ModelHandle's timer in the same call.
- Registered handles correctly update on flip without app restart.
- Both Tauri commands wired in `invoke_handler!`.
- `cargo check` clean.

---

### Task 5: Wrap Parakeet recognizer in ModelHandle (macOS path)

**Goal.** Convert `core/src/recognizer/mod.rs::ENGINE` from `OnceLock<Mutex<Box<dyn Recognizer>>>` to a `ModelHandle`-wrapped form. Mac path uses `FluidAudioBridge`.

**Files.** `core/src/recognizer/mod.rs`.

**Steps.**

1. Change `ENGINE: OnceLock<Mutex<Box<dyn Recognizer>>>` to `ENGINE: OnceLock<ModelHandle<Box<dyn Recognizer>>>` initialized via `ModelHandle::with_idle_timeout("recognizer", default_backend_loader, Duration::from_secs(300))`.
2. The loader closure constructs and `ensure_loaded`s the backend in one shot:
   ```rust
   let loader = || {
       let mut backend = default_backend();
       backend.ensure_loaded()?;
       Ok(backend)
   };
   ```
3. Rewrite the public functions:
   - `recognizer_ensure_loaded()` calls `engine().load()` (no-op if Loaded).
   - `recognizer_transcribe(samples)` calls `engine().use_with(|r| r.transcribe(samples))`.
4. Verify the Swift bridge (`FluidAudioBridge`) cleans up on `Drop` correctly. Read `core/src/recognizer/fluidaudio.rs::Drop` impl; if missing, add one that calls into Swift to release the model handle.
5. Add a manual smoke note (top of file): "first transcription after 5 min idle on Mac shows `PHASE_LOADING_MODEL` for ~200–500 ms then proceeds".
6. `cargo check --target aarch64-apple-darwin -p openwhisper-tauri` clean.
7. Manual: launch dev build, dictate, wait 6 min, dictate again. Verify the second dictation goes through `PHASE_LOADING_MODEL` again briefly.

**Outcome ACs (Backlog).**

- `ENGINE` is a `ModelHandle<Box<dyn Recognizer>>` with 5-min idle timeout.
- `recognizer_ensure_loaded` and `recognizer_transcribe` retain their existing public signatures.
- `FluidAudioBridge` releases its Swift handle on `Drop` (verified or added).
- After 5+ min idle, next dictation re-enters `PHASE_LOADING_MODEL` and succeeds.
- `cargo check` clean on aarch64-apple-darwin.

---

### Task 6: Wrap Parakeet recognizer in ModelHandle (Windows path)

**Goal.** Same as Task 5 for Windows — wrap `OrtParakeet`. Specific concern: verify `ort::Session::drop()` actually frees on unload.

**Files.** `core/src/recognizer/ort_parakeet.rs`, `core/src/recognizer/mod.rs` (shared logic from Task 5 already covers it).

**Steps.**

1. Audit `OrtParakeet::Drop` — confirm `ort::Session` is dropped explicitly. If not, add a `Drop` impl.
2. Run a manual memory test on Windows: log RSS via `query_process_memory()` before and after a `engine().unload()` call. Verify RSS decreases by ~the model size (a few hundred MB for Parakeet-TDT v3). Document the observed delta in a comment.
3. If `ort::Session` does not release on drop in our version of `ort` crate, add a workaround note + open follow-up task (do not block this task).
4. `cargo check --target x86_64-pc-windows-msvc -p openwhisper-tauri` clean (or run on a Windows host).
5. Manual smoke (Windows): same as Task 5 — dictate, wait 6 min, dictate again; observe `PHASE_LOADING_MODEL` re-entry and RSS drop in between via the Diagnostics panel (lands in Task 8).

**Outcome ACs (Backlog).**

- `OrtParakeet` releases its `ort::Session` on `Drop` (verified or added).
- After idle release, RSS drops by an observable amount on Windows.
- `cargo check` clean on x86_64-pc-windows-msvc.

---

### Task 7: Tauri telemetry commands + state-change events

**Goal.** Surface memory + state to the Tauri shell. Two commands and one event channel.

**Files.** `apps/tauri/src-tauri/src/lib.rs`, `core/src/telemetry/mod.rs`, `core/src/model_lifecycle/mod.rs`.

**Steps.**

1. In `core/src/telemetry/mod.rs`, add aggregation:
   ```rust
   pub struct ModelMemoryRow { pub label: String, pub state: LifecycleState, pub estimated_rss_bytes: u64 }
   pub struct MemoryStats { pub process: ProcessMemory, pub models: Vec<ModelMemoryRow> }
   pub fn collect_memory_stats() -> MemoryStats { ... }
   ```
   Walks the `model_lifecycle::registered_handles()` list and reads each handle's state + memory estimate.
2. In `apps/tauri/src-tauri/src/lib.rs`, add `#[tauri::command] fn telemetry_get_memory() -> MemoryStats` that calls into the core function.
3. In `core/src/model_lifecycle/mod.rs`, add a callback registry:
   ```rust
   pub fn on_state_change(cb: Arc<dyn Fn(&str, LifecycleState) + Send + Sync>) { ... }
   ```
   Every transition fires registered callbacks with `(label, new_state)`.
4. In `apps/tauri/src-tauri/src/lib.rs::setup`, register a callback that calls `app.emit("model-state-changed", payload)` with `{ label, state }` on every fire. Use a small JSON payload type.
5. Register `telemetry_get_memory` in `invoke_handler!`.
6. `cargo check` clean.
7. Manual: trigger a recognizer load, observe `model-state-changed` event in `tauri --inspector` console.

**Outcome ACs (Backlog).**

- `telemetry_get_memory` Tauri command returns `MemoryStats` with per-model rows.
- `model-state-changed` event fires on every Lifecycle transition with `{ label, state }`.
- Event includes both recognizer transitions and any future cleanup transitions.
- `cargo check` clean.

---

### Task 8: Diagnostics panel UI

**Goal.** New Settings → Diagnostics sub-pane rendering live RAM table. Refresh ~1 Hz via polling `telemetry_get_memory`.

**Files.** `apps/tauri/src/components/diagnostics-pane.tsx` (new), `apps/tauri/src/components/settings-window.tsx` (sidebar entry).

**Steps.**

1. Add a "Diagnostics" entry to the Settings sidebar (lives wherever the Audio / General entries are wired). Place it last (power-user surface).
2. Create `DiagnosticsPane`:
   - On mount, poll `invoke('telemetry_get_memory')` every 1000 ms.
   - Subscribe to `model-state-changed` for instant updates between polls.
   - Render a table:
     | Label | State | Estimated RAM | Last loaded |
     |---|---|---|---|
     | (process) | — | <RSS> / peak <peak> | — |
     | recognizer | Loaded | ~500 MB (est.) | 4 min ago |
     | cleanup | Unloaded | — | — |
   - Footer note: "Per-model RAM is an estimate; concurrent allocations and ANE-resident memory on macOS may not be reflected."
3. Use shadcn primitives consistent with other panes (Field, Card, Table).
4. `pnpm exec tsc -b` clean. `pnpm test:ui` green (with new pane unit-tested in Task 10).

**Outcome ACs (Backlog).**

- Diagnostics sidebar entry visible in Settings window.
- Pane renders process RSS + per-model rows, refreshes every ~1 s.
- State updates propagate immediately on `model-state-changed`.
- Estimate caveat surfaced in pane footer.

---

### Task 9: General pane — "Keep models warm" toggle

**Goal.** Add toggle row to Settings → General, persisting via `settings_set_keep_models_warm`.

**Files.** `apps/tauri/src/components/general-pane.tsx`.

**Steps.**

1. Add a "Performance" section in GeneralPane (or merge into existing Appearance — executor's call). Single Field row with shadcn Switch.
2. On pane mount: `invoke<PerformanceSettings>('settings_get_performance')` → set local state. Default `false` if call rejects.
3. On toggle flip: optimistic update, then `invoke('settings_set_keep_models_warm', { value: nextValue })`. On rejection, revert + console warn.
4. Label: "Keep models warm". Helper: "Keep speech-recognition and cleanup models in memory between sessions. Uses more RAM, eliminates first-use load delay."
5. `pnpm exec tsc` clean.

**Outcome ACs (Backlog).**

- Toggle renders in General pane and reflects persisted state on open.
- Flip persists to settings.json AND flips the atomic in the same call.
- Default OFF for new users.
- Visual treatment matches existing pane toggles.

---

### Task 10: Playwright spec — diagnostics pane + keep-warm toggle

**Goal.** UI half coverage. Memory measurements themselves are not CI-testable; document manual smoke at the top.

**Files.** Extend `apps/tauri/tests/settings-window.spec.ts`.

**Steps.**

1. Test 1 — Diagnostics sidebar entry visible, clicking opens the pane, table renders with at least the process row (mock `telemetry_get_memory` to return a stable shape).
2. Test 2 — Pane updates RSS column when shim re-emits `telemetry_get_memory` with a higher value.
3. Test 3 — Pane updates state column when shim emits `model-state-changed`.
4. Test 4 — General pane: "Keep models warm" toggle defaults OFF; click flips to ON; shim records `invoke('settings_set_keep_models_warm', { value: true })`.
5. Test 5 — Hydrate-from-stored-value: with shim returning `{ keep_models_warm: true }` from `settings_get_performance`, toggle renders ON.
6. Document manual smoke at top of file: "Real cold-load-after-idle behavior is not CI-testable; verify on Mac and Windows by leaving the app open 6+ min between dictations and watching the Diagnostics pane."
7. `pnpm test:ui` green.

**Outcome ACs (Backlog).**

- 5 Playwright assertions covering: sidebar entry, RSS update, state update, toggle flip, hydrate-from-stored.
- Manual smoke steps documented in the spec.
- `pnpm test:ui` green locally and on CI.

---

## Reviewer loop

Once all 10 plan tasks have matching Backlog subtasks, dispatch the plan-document-reviewer agent with the standard plan-review criteria PLUS the verbatim Backlog-enforcement fragment from `.claude/skills/writing-backlog-plans/references/plan-reviewer-addendum.md`. Address findings before handing the plan to an executor.

## Execution handoff

Order: 1 → 2 → 3 → 4. Then 5 and 6 in parallel. Then 7. Then 8 and 9 in parallel. Then 10. Status updates flow through `backlog task edit` per the cheatsheet.
