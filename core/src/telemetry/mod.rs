//! Process telemetry primitives — read-side facts about the running
//! OpenWhisper process for the in-app Diagnostics pane and the
//! headless `cli` smoke harness.
//!
//! Two layers:
//!
//! - `memory::query_process_memory` — current process RSS + peak via
//!   the `sysinfo` crate (cross-platform). Used for the "is our
//!   process leaking?" Diagnostics readout.
//! - [`collect_memory_stats`] — aggregates the process snapshot with
//!   per-model rows from the [`crate::model_lifecycle`] registry.
//!   Surface the Tauri shell's `telemetry_get_memory` command
//!   returns (TASK-62.7) and the headless `cli memory --models`
//!   subcommand consumes (per `openwhisper-headless-first`).

use serde::{Deserialize, Serialize};

use crate::model_lifecycle::LifecycleState;

pub mod memory;

pub use memory::{query_process_memory, query_system_memory, ProcessMemory, SystemMemory};

/// Per-model snapshot for the Diagnostics → Memory pane. One row per
/// registered `ModelHandle` with an idle timer; handles built via
/// `ModelHandle::new` (no timer) are intentionally not in the
/// registry and don't appear here.
///
/// `serde` derives so the Tauri command can ship this straight to the
/// React side without a hand-rolled bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMemoryRow {
    /// `recognizer`, `cleanup-llm`, … (the `label` passed to
    /// `ModelHandle::with_idle_timeout`).
    pub label: String,
    pub state: LifecycleState,
    /// RSS-delta estimate captured at the most recent
    /// `Loading → Loaded` transition. `0` when the handle has never
    /// loaded successfully OR was unloaded since (delta is per-load
    /// snapshot, not a live measurement). Documented as estimated in
    /// the UI; concurrent allocations during load skew the number.
    pub estimated_rss_bytes: u64,
    /// Static claim the model holds while resident — see
    /// [`crate::model_lifecycle::ModelClaim`]. The Diagnostics
    /// readout uses this (alongside `in_process`) to compute a
    /// system-wide-honest "OpenWhisper Memory" total that includes
    /// ANE-resident weights on Mac. `0` when the handle is Unloaded
    /// or no claim was registered.
    pub claimed_bytes: u64,
    /// `true` when the claim is already counted inside the calling
    /// process's RSS (CPU-resident ONNX). `false` when the claim
    /// lives outside RSS (Mac ANE / GPU VRAM) and should be added on
    /// top of `ProcessMemory.rss_bytes` for a true total.
    pub in_process: bool,
}

/// Combined readout: host-wide system memory + this process's RSS +
/// per-model rows. Returned by the Tauri command `telemetry_get_memory`
/// that the Diagnostics pane polls at ~1 Hz.
///
/// `system` answers "how is my whole machine holding up?" so the user
/// doesn't have to alt-tab to Activity Monitor / Task Manager.
/// `process` answers "is OpenWhisper itself heavy?". The pane shows
/// both, side by side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub system: SystemMemory,
    pub process: ProcessMemory,
    pub models: Vec<ModelMemoryRow>,
}

/// One-shot snapshot of memory state for the Diagnostics readout.
/// Walks the live `ModelHandle` registry and reads each handle's
/// label / state / RSS-delta estimate without locking the handle's
/// inner mutex — telemetry must not contend with active transcription.
pub fn collect_memory_stats() -> MemoryStats {
    MemoryStats {
        system: query_system_memory(),
        process: query_process_memory(),
        models: crate::model_lifecycle::registry_snapshot(),
    }
}
