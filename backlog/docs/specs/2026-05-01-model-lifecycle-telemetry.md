---
id: doc-19
title: 'Model memory telemetry + lifecycle foundation — design'
type: spec
created_date: '2026-05-01 00:00'
---

# Model memory telemetry + lifecycle foundation — design

**Backlog parent:** TASK-62
**Date:** 2026-05-01
**Status:** Spec → Plan

## Problem

The recognizer loads once and stays resident for the lifetime of the app. There is no observability layer that can answer "how much RAM does Parakeet cost while idle vs while transcribing?" Without numbers we cannot make informed decisions about adding a second model (the LLM cleanup pass in TASK-63), pick a hardware floor, or tell power users why the app's RAM footprint is what it is.

A second-order problem: when the LLM cleanup pass lands, "always-resident" stops scaling. Two models that each cost ~500 MB–1.5 GB and only run during dictation bursts should not occupy that RAM during the 23 hours of the day the user is doing anything else.

## Goal

Two complementary capabilities, both living in `core/`:

1. **Memory telemetry** — a cross-platform query that returns the current process RSS and a per-model attribution (what each loaded model is costing right now), exposed to the Tauri shell via commands and rendered in a Diagnostics UI.
2. **Explicit model lifecycle** — a `ModelHandle<T>` abstraction that owns load/unload with an idle timer, replacing the implicit "load once, stay forever" pattern. Wraps the existing Parakeet recognizer to validate the abstraction without adding new model dependencies.

A "Keep models warm" power-user setting overrides the idle release for users who'd rather pay RAM than first-use latency.

## Non-goals (this spec)

- **LLM cleanup integration.** That's TASK-63. This spec stops at the trait + Parakeet wrapping; the LLM model is a future consumer of the same abstraction.
- **Pre-warm on recording start.** Same — TASK-63 wires the recording-start signal into the cleanup model. The lifecycle abstraction must support it; this spec does not.
- **System-RAM auto-detect** ("if user has lots of RAM, never release"). Confirmed out of scope with user — too much state to track, marginal value.
- **Real animated loading indicator in pill.** Placeholder text/dot is enough for this parent; TASK-64 ships the animation.
- **Per-model unload during active dictation.** Models are released only between dictations. We do not unload mid-recording or mid-cleanup.

## Behavior model

### State machine (lives in `core/src/model_lifecycle.rs`)

```
            load()                     use()
Unloaded ─────────→ Loading ──────→ Loaded ──────→ Active
                                       ↑              │
                                       │              ↓
                                       └──────────────┘
                                       ↑
                                  idle timer
                                       ↓
                                   Releasing ─────→ Unloaded
```

States:

- **Unloaded** — no model in memory, no resources held.
- **Loading** — load in progress (file I/O + CoreML compile / GGUF mmap warmup). Transient. Phase visible to UI.
- **Loaded** — model resident, idle, ready. Idle timer is counting down.
- **Active** — currently servicing a call (transcribe / cleanup). Idle timer paused.
- **Releasing** — unload in progress. Transient. Could fail (rare) — falls back to Unloaded with the resources actually freed.

Transitions:

- `load()` from `Unloaded` → `Loading` → `Loaded`. Idempotent: calling on `Loaded` is a no-op; calling during `Loading` is awaited.
- Any access (`use()`) from `Loaded` → `Active` → `Loaded` (single-flight; concurrent users serialize).
- `idle_timer expires` (configurable per handle, default 5 min recognizer / 60 s LLM) from `Loaded` → `Releasing` → `Unloaded`.
- `force_unload()` for the "keep warm = OFF and user closed Settings" case — same as idle timer firing immediately.

### Idle timer

Background task per `ModelHandle`. On every `Loaded` transition, the timer is (re)armed for the configured idle window. On any `use()`, the timer is cancelled (state goes Active) and re-armed after the call completes (state goes back to Loaded). When the timer fires and the state is still Loaded, it triggers `Releasing`.

The "Keep models warm" setting flips behavior globally:

- ON → idle timers are armed at `Duration::MAX` (effectively never fire). Models loaded once stay until app quit.
- OFF (default) → idle timers honor their configured timeout.

The setting is a hot-reload — flipping it cancels and re-arms all live handles' timers immediately.

### Memory telemetry (`core/src/telemetry/memory.rs`)

| Field | Source | Notes |
|---|---|---|
| Process RSS | macOS `task_info(MACH_TASK_BASIC_INFO).resident_size`; Windows `GetProcessMemoryInfo().WorkingSetSize` | Cross-platform via `sysinfo` crate as a baseline; native APIs for tighter numbers later if needed |
| Process peak RSS | Same source, peak field | macOS `resident_size_max`; Windows `PeakWorkingSetSize` |
| Per-model attribution | Captured at `Loaded` transition via RSS delta (RSS at end of Loading minus RSS at start of Loading) | Imperfect — concurrent allocations elsewhere skew it. We label this clearly in the UI as "estimated" |
| Lifecycle state | `ModelHandle::state()` | Authoritative |

The "estimated" caveat matters: ANE-resident models on macOS occupy memory that doesn't always show up in process RSS the way userspace allocations do. We surface what we can measure and label the rest.

### Surfaces

- **Tauri commands** (registered in `apps/tauri/src-tauri/src/lib.rs`):
  - `telemetry_get_memory() -> MemoryStats` — returns process RSS/peak + per-model rows
  - `telemetry_subscribe_state()` — emits `model-state-changed` events when any handle transitions
- **Diagnostics panel** in Settings → Diagnostics: live RAM table, refresh ~1 Hz. Shows process row + one row per registered handle with state, last-loaded duration, current RSS attribution.
- **Settings → General**: "Keep models warm" toggle, default OFF. Helper text: "Keep speech-recognition and cleanup models in memory between sessions. Uses more RAM, eliminates first-use load delay."

### Existing recognizer integration

`core/src/recognizer/mod.rs` currently exposes `recognizer_ensure_loaded()` and `recognizer_transcribe()`. The wrapping does not change the public API:

- `ENGINE: OnceLock<Mutex<Box<dyn Recognizer>>>` becomes `ENGINE: OnceLock<ModelHandle<Box<dyn Recognizer>>>`.
- `recognizer_ensure_loaded()` calls into `ModelHandle::load()`.
- `recognizer_transcribe()` calls into `ModelHandle::use_with(|r| r.transcribe(...))`.
- `Recognizer::ensure_loaded` becomes the loader closure passed to the handle factory.

The dictation state machine in `core/src/dictation.rs` already has `PHASE_LOADING_MODEL` (line 27) — we reuse it; cold reload after idle release surfaces as the same phase the user already understands.

## Cross-platform implementation

| Concern | macOS | Windows |
|---|---|---|
| RSS + peak query | `sysinfo` crate (default) → `task_info` for tighter numbers later | `sysinfo` crate (default) → `GetProcessMemoryInfo` later |
| Idle-timer task host | Tokio runtime already used in core (`#[tokio::main]` in dictation paths) | Same |
| Recognizer unload | `FluidAudioBridge::drop()` releases the Swift bridge handle; CoreML model unloads when last reference drops | `OrtParakeet::drop()` releases the `ort::Session` |
| Cold-reload latency | ~200–500 ms first time (CoreML compile cache helps subsequent loads) | ~100–300 ms (ONNX session creation) |
| Telemetry per model | Same delta-on-load attribution; ANE-resident memory caveat called out in UI | RSS reflects ONNX runtime + model weights cleanly |

The watcher / idle timer runs on the Tokio runtime hosting the cleanup async path that lands in TASK-63. For TASK-62 alone we'll use a `tokio::spawn` per handle with a `JoinHandle` stored in the handle for cancellation; if Tokio isn't already required by a Parakeet-only build, a `std::thread::spawn` + condvar fallback is acceptable for the recognizer's 5-min cadence (revisit when LLM lands).

## Settings shape

```jsonc
{
  "performance": {
    "keep_models_warm": false
  }
}
```

- Field is optional. Absent = treat as `false` (zero-config default: release when idle).
- Setter writes the JSON and flips a process-global `AtomicBool`. The lifecycle module subscribes to changes (or polls the atomic on every state transition) and reconfigures live handles immediately — no restart required.

## Trade-offs / open decisions

- **Telemetry granularity**: per-model attribution by RSS-delta is imperfect (concurrent allocs skew it). Acceptable for v1; if it causes confusion in Diagnostics, switch to a more structured allocator hook (heavy lift) later.
- **Idle timeouts hardcoded vs configurable**: hardcoded for v1 (5 min recognizer / 60 s LLM). Settings exposes only the binary "warm or not" toggle. We can grow per-model timeout knobs if real telemetry shows the defaults are wrong.
- **Async runtime in core**: the LLM cleanup will need Tokio anyway (TASK-63). For this parent, we adopt Tokio in core if not already pervasive, or use `std::thread` for the recognizer-only timer. Decision deferred to plan execution.
- **Diagnostics panel placement**: Settings → Diagnostics (new sub-pane) vs a "Performance" sub-pane in General. Plan defaults to its own pane — telemetry is a power-user surface, deserves separation.
- **`force_unload()` exposure**: kept internal to core; not surfaced as a Tauri command. The "Keep warm OFF" path drives unload via the idle timer firing immediately. We can add a manual "Unload now" button later if power users ask.

## Risks

- **CoreML compile cache invalidation on Mac.** First load after model conversion is slow (~seconds). The lifecycle abstraction does not change this; we surface it via the existing `PHASE_LOADING_MODEL` indicator. If the cache is invalidated mid-session (rare), users see a longer reload.
- **`ort::Session::drop()` on Windows.** Has historically been finicky in some versions when the runtime is unloaded mid-process. We test explicit drop with onnxruntime.dll vendoring path (per `openwhisper-dev-workflow` skill) before shipping.
- **Tokio runtime startup cost in core if not already a runtime dep.** ~10 ms the first time. Acceptable for a singleton.
- **Setting hot-reload race.** If a user flips "Keep warm" OFF while a model is in `Active`, the timer starts after the use completes, not retroactively. Acceptable.

## References

- Existing recognizer trait + global engine: `core/src/recognizer/mod.rs:55–99`.
- Dictation phases (already exposes `PHASE_LOADING_MODEL`): `core/src/dictation.rs:26–31`.
- Settings file conventions and atomic-flag pattern: see `apps/tauri/src-tauri/src/settings/mod.rs` and the pill-follow plan (TASK-55, Task 1) for the canonical "JSON + AtomicBool" recipe.
- Project principles applied: orchestration-in-rust (state machine in `core/`), zero-config-over-toggles (auto-release default, "Keep warm" is the override).
