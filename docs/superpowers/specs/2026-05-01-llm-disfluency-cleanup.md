# LLM-based disfluency cleanup — design

**Backlog parent:** TASK-63
**Date:** 2026-05-01
**Status:** Spec → Plan
**Depends on:** TASK-62 (model lifecycle foundation)

## Problem

The existing transcript pipeline (`core/src/transcript.rs`, shipped via TASK-16/43/44) handles English + Danish hard fillers, comma-run collapse, and adjacent duplicate-word dedupe. It does this with deterministic rules and no model. It works well for the cases it covers, but the rule set has known ceilings:

- **Soft fillers** ("like", "you know", "I mean") are polyfunctional. A rule that strips them either over-deletes (nukes the user's actual style) or under-deletes (leaves them all). Discrimination requires context.
- **Partial-word stutter / false starts** ("repet- repetitive", "I went to- I drove to the store") are reparandum-repair patterns. Regex catches the trivial cases; the rest need understanding of which words belong to the surrendered start vs the corrected version.
- **Multilingual coverage** ends at EN + DA today. Parakeet v3 produces text in 25 European languages. Hand-rolling per-language filler lists scales but doesn't solve the soft-filler / false-start problems for any of them.
- **Light grammar** — capitalization restoration, double-punctuation, re-agreement after fragment removal — is out of reach for the rule set entirely.

A small local LLM addresses the ceiling. Modern small models (Qwen 3.5 0.8B / 2B) handle 119+ languages, run in sub-500ms on Mac and sub-1s on Windows for short edits, and — critically — can be constrained at decode time to emit only a delete-span list rather than free-text rewrites, eliminating the "model rewrote my words" hallucination class by construction.

## Goal

Add a local LLM cleanup pass as an opt-in post-processing step **after** the existing rule-based filter. Pipeline becomes:

```
Parakeet → core/transcript::process (rules: fillers, dedupe, comma-runs)
        → cleanup::process_llm    (NEW: soft fillers, false starts, light grammar)
        → text injection
```

Specifics:

- **Engine**: `llama-cpp-2` Rust crate + GGUF format. Same engine on Mac (Metal) and Windows (Vulkan / CPU). Single binary path.
- **Default model**: Qwen 3.5 0.8B Q4_K_M (~500 MB). Hits ≤500 ms target across all four hardware tiers we care about (Mac high/low, Win CPU/GPU).
- **Opt-in upgrade**: Qwen 3.5 2B Q4_K_M (~1.3 GB) for users on M-series 2023+ or Windows-with-GPU.
- **Constrained decoding**: LLGuidance-driven JSON edit-list output (delete spans only, indices into the input). Model cannot insert or rewrite. Catastrophic failure (validator rejects) → drop cleanup, ship rule-pass output as today.
- **Pre-warm on recording start**: cleanup `ModelHandle::load()` fires when dictation enters `PHASE_RECORDING`. By the time the user stops dictating and STT completes, cleanup is already in `Loaded` state. No load cost on the user's wait path in the common case.
- **Lifecycle**: Cleanup model uses the `ModelHandle` abstraction from TASK-62. Default 60 s idle timeout. "Keep models warm" setting overrides.

## Non-goals (this spec)

- **Replacing the rule-based pre-pass.** Rules are fast, deterministic, and already work. LLM is additive — it gets text the rules already cleaned.
- **Cloud LLM fallback.** Per `openwhisper-project-principles`, cleanup must be local. Adding a cloud option for "better quality" is an explicit out-of-scope decision the user can re-open later if they want.
- **Model fine-tuning.** Off-the-shelf Qwen 3.5 instruct, prompted at runtime. No per-language LoRAs, no custom training.
- **Real loading-state pill animation.** Placeholder text/dot only; TASK-64 ships the animation.
- **Streaming cleanup output.** Cleanup runs on the full transcript at once (short input, short output). No partial / streaming UX.
- **Cleanup of rule-pass output we disagree with.** If the user has set custom filler lists or aggressive rules upstream, we don't try to "undo" them at the LLM layer.

## Behavior model

### Cleanup pipeline (lives in `core/src/cleanup/`)

```rust
// Conceptual; final API in plan
pub trait CleanupEngine: Send {
    fn ensure_loaded(&mut self) -> Result<(), String>;
    fn cleanup(&mut self, text: &str, hint: CleanupHint) -> Result<String, String>;
}

pub struct CleanupHint {
    pub primary_languages: Vec<String>,  // user setting, e.g. ["en", "da"]
    pub aggressive: bool,                 // user toggle for soft-filler removal
}
```

The `cleanup()` call:

1. Builds prompt with input text + hint (primary languages, aggressive flag).
2. Calls `llama-cpp-2` with LLGuidance-constrained schema for `[{"op":"delete","span":[start,end]},...]`.
3. Validates the edit list:
   - All spans are `(start, end)` with `0 ≤ start < end ≤ input_len`.
   - Spans don't overlap.
   - Total deleted length ≤ 50 % of input (sanity guard against hallucination — cleanup should never delete most of what the user said).
4. Applies edits in reverse order to produce output text.
5. On any validator failure → returns the unchanged input + logs a warning. Cleanup is a polish layer; failing closed = ship the rule-pass output.

### Constrained-decoding schema

LLGuidance grammar restricts the model to emit JSON matching:

```json
[
  { "op": "delete", "span": [12, 18] },
  { "op": "delete", "span": [42, 55] }
]
```

No `insert`, no `replace`, no free text. The grammar pins the output structure so validation is mostly about span sanity (above), not model conformance.

### Pre-warm trigger

`core/src/dictation.rs` already drives the phase transitions and exposes `PHASE_RECORDING`. We add a hook in the recording-start path that calls into `cleanup::ensure_loaded_async()` — fire-and-forget, returns immediately.

`ensure_loaded_async()` dispatches a Tokio task that calls `ModelHandle::load()`. The model's idle timer is reset on this load. When the user stops dictating (typically 3–60 s later), the cleanup model is in `Loaded` state and `cleanup()` runs immediately.

Edge cases:

- **Very short dictation (<1 s)**: load may not be done. We `await` the load before running cleanup — adds the latency we wanted to hide. Acceptable: users who dictate 1-second snippets probably don't need cleanup polish.
- **User cancels recording**: pre-warm has fired but no transcript will arrive. The cleanup model sits in `Loaded`, idle timer runs, releases on schedule. No wasted work beyond a bit of disk I/O.
- **Cleanup disabled in settings**: pre-warm hook is a no-op. We don't load a model the user doesn't want.

### Multilingual handling

Parakeet v3 outputs the language as part of recognition (per existing `transcript::process` heuristic — Danish-character detect, future native language tag). We pass the detected language plus the user's primary-languages setting into the cleanup prompt:

> Input is in {detected_lang}. The user primarily speaks: {primary_langs}. Code-switching is expected.

Qwen 3.5's 119+ language coverage handles all 25 Parakeet v3 languages. The hint helps disambiguate ambiguous cases (e.g., "er" is Danish copula vs English filler — already solved at rules layer for those two, but the principle generalizes).

### "Aggressive cleanup" toggle

Default OFF — preserves user voice. When OFF, prompt instructs the model to leave discourse markers ("like", "you know", "I mean") in place. When ON, prompt allows soft-filler removal.

This is the primary lever for users who want a smoother output. We ship default-OFF because the user explicitly flagged sensitivity to over-deletion of their own discourse markers.

## Cross-platform implementation

| Concern | macOS | Windows |
|---|---|---|
| Engine | `llama-cpp-2` with Metal feature | `llama-cpp-2` with Vulkan feature, CPU fallback |
| GGUF location | `~/Library/Application Support/com.openwhisper.app/models/qwen3.5-0.8b-q4_k_m.gguf` | `%LOCALAPPDATA%\com.openwhisper.app\models\qwen3.5-0.8b-q4_k_m.gguf` |
| First-run download | Triggered on first cleanup-enabled dictation. Progress events use the existing `download_bytes_done`/`download_bytes_total` fields in the dictation snapshot (`core/src/dictation.rs:51`). | Same |
| Acceleration backend | Metal (sub-500 ms decode for 0.8B Q4 on M-series) | Vulkan if present, CPU fallback (sub-1 s on i7 13th gen, sub-500 ms with discrete GPU) |
| Bundle impact (no model) | +~3–5 MB for `llama-cpp-2` + Metal kernels | +~3–5 MB for `llama-cpp-2` + Vulkan path |
| Bundle impact (with bundled model) | We do **not** bundle the model. Download on first cleanup use, mirroring the Parakeet download pattern. | Same |
| `mmap` warmup | Fired at first `Loaded` transition; first inference pays cold-cache cost (~150–400 ms on top of compute) | Same. Vulkan/CUDA may need an additional GPU warmup; tested empirically before shipping. |

## Settings shape

```jsonc
{
  "cleanup": {
    "enabled": false,
    "model_variant": "qwen3.5-0.8b-q4",
    "aggressive": false,
    "primary_languages": ["en"]
  }
}
```

- `enabled` default `false`. Cleanup is opt-in for v1 (we don't change a user's transcripts without consent).
- `model_variant`: `"qwen3.5-0.8b-q4"` (default) or `"qwen3.5-2b-q4"` (opt-in upgrade). Switching variants is a model swap → unloads the current handle, loads the new one on next use.
- `aggressive`: default `false`. Controls soft-filler removal in the prompt.
- `primary_languages`: defaults to `["en"]`; UI multiselect from a list of the 25 Parakeet v3 supported languages.

UI surface lives in **Settings → Dictation** (new section "Cleanup" — distinct from the future Diagnostics panel from TASK-62).

## Trade-offs / open decisions

- **Default OFF for `enabled`**. We ship cleanup as opt-in because (a) it's a meaningful behavior change to a user's transcripts, (b) it adds a 500 MB download, and (c) per project principles we lead with auto-detect / zero-config but only when the auto-detect is unambiguous; here it isn't.
- **Default 0.8B over 2B**. 0.8B is the only variant that hits ≤500 ms across all four hardware tiers. 2B is faster + better but doesn't hit the floor. Power users on capable hardware can opt up.
- **Pre-warm always on (when cleanup enabled)**. We don't add a setting for "delay load until cleanup is needed" because that's the case where we actively want pre-warm. If telemetry later shows pre-warm is wasteful for a class of users, revisit.
- **JSON edit list vs free text**. Edit-list-only constraint is non-negotiable for v1. The "model could rewrite better than delete-only" temptation is real but the hallucination risk is too high without a much stronger eval harness.
- **Engine: `llama-cpp-2` only**, not MLX path on Mac. MLX is 1.3–1.8× faster on Apple Silicon but adds a second engine. Ship `llama-cpp-2` only; revisit MLX if Mac-low p95 telemetry shows latency pain after a few weeks of real use.
- **No bundled model**. Download on first cleanup-enabled dictation, mirroring the Parakeet download path. Avoids ballooning the installer.

## Risks

- **Hallucination via constraint-leak**. LLGuidance is robust but not bulletproof — if the grammar is buggy or the model emits unparseable JSON, we fall back to the rule-pass output. The validator (span sanity + 50 % deletion cap) is the second layer.
- **Model download interrupted**. First-run failure mode: user enables cleanup, loses connection mid-download, dictation works (rules pass) but cleanup is silently broken. We surface the download state via the existing `download_bytes_*` snapshot fields and show "Download cleanup model" UX in Settings → Dictation.
- **Vulkan availability on Windows**. Older or virtualized Windows boxes may not have Vulkan. CPU fallback works but may push p95 above 1 s. We measure and clearly label "your hardware: cleanup ~Xs / cleanup not recommended" in the diagnostics panel from TASK-62.
- **Pre-warm wasted work for users who cancel often**. If a user starts and cancels recordings frequently, we'll load the cleanup model unnecessarily. The 60-s idle timer reclaims after the canceled session ends, so the waste is bounded. Acceptable.
- **Existing rule-pass interactions**. The LLM sees text that's already had hard fillers stripped + duplicates collapsed. It must not "second-guess" those — prompt and aggressive=OFF default make this unlikely but worth watching in real use.
- **App Store / sandboxing implications.** Bundling `llama-cpp-2` with Metal on Mac is fine for direct distribution (Developer ID + notarized); App Store / sandbox compliance is not a near-term concern.

## References

- Existing transcript pipeline: `core/src/transcript.rs` (called from both shells via `transcript::process`).
- Existing recognizer model-download pattern: `core/src/recognizer/download.rs` — mirror this for the GGUF.
- Dictation state machine + download progress fields: `core/src/dictation.rs:26–53`.
- Lifecycle abstraction (this spec depends on it): TASK-62 spec at `docs/superpowers/specs/2026-05-01-model-lifecycle-telemetry.md`.
- Project principles applied: local-first-for-cost-saving (LLM is local), zero-config-over-toggles (sane defaults, minimal user choices), Mac-as-source-of-truth (settings UI mirrors Mac SwiftUI patterns).
- Engine choice rationale: discussed in conversation context — `llama-cpp-2` over MLX for portability + Vulkan for Windows GPU coverage.
