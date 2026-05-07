//! `use openwhisper_core::prelude::*;` — one-line entry point for
//! library consumers (the headless CLI, the Tauri shell, future
//! external contributors).
//!
//! Per-module paths stay public so granular use is still possible;
//! the prelude just re-exports the canonical types most consumers
//! need together. Modules covered today:
//!
//! - `audio` — capture + device enumeration
//! - `dictation` — phase machine + snapshot
//! - `diagnostics` — recognizer info + crash-dump trait surface
//! - `media_gate` — pause/resume gate trait + diagnostic
//! - `model_lifecycle` — load/unload state machine for resident models
//! - `settings` — schema types + cache accessors
//! - `telemetry` — process memory readout
//! - `transcript` — filter pipeline
//! - `stats` — read-side aggregator
//! - `store` — persistence handle
//!
//! Recognizer types are gated behind the `recognizer` feature
//! because consumers without that feature can't construct one.

pub use crate::audio::{AudioDeviceInfo, AudioEngine, SelectedDeviceStatus};
pub use crate::diagnostics::{
    CrashDump, CrashDumpReader, CrashId, ReadError, RecognizerInfo, default_crash_reader,
};
#[cfg(feature = "recognizer")]
pub use crate::diagnostics::recognizer_info;
pub use crate::dictation::{DictationSnapshot, Injector};
pub use crate::media_gate::{MediaController, MediaGateState, PauseDiagnostic};
pub use crate::model_lifecycle::{
    LifecycleState, ModelHandle, StateChangeCallback, apply_keep_warm, on_state_change,
    registry_snapshot,
};
pub use crate::settings::{
    AudioSettings, BehaviorSettings, HotkeyConfig, HotkeyKind, HotkeySettings, HotkeyTarget,
    PerformanceSettings, PillSettings, StatsSettings, keep_models_warm,
};
pub use crate::stats::StatsSummary;
pub use crate::store::{Store, StoreError};
pub use crate::telemetry::{
    collect_memory_stats, query_process_memory, MemoryStats, ModelMemoryRow, ProcessMemory,
};
pub use crate::transcript::FillerLang;

#[cfg(feature = "recognizer")]
pub use crate::recognizer::{Recognizer, TranscribeResult};
