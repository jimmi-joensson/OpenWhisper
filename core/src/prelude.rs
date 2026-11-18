//! `use openwhisper_core::prelude::*;` — one-line entry point for
//! library consumers (the headless CLI, the Tauri shell, future
//! external contributors).
//!
//! Per-module paths stay public so granular use is still possible;
//! the prelude just re-exports the canonical types most consumers
//! need together. Modules covered today:
//!
//! - `ask`: a spoken question, answered by a language model and
//!   shown in a panel (feature `ask`)
//! - `audio` — capture + device enumeration
//! - `dictation` — phase machine + snapshot
//! - `diagnostics` — recognizer info + crash-dump trait surface
//! - `file_transcription`: decode + transcribe audio/video files
//! - `media_gate` — pause/resume gate trait + diagnostic
//! - `refine`: optional language-model cleanup after transcription
//!   (feature `refine`)
//! - `model_lifecycle` — load/unload state machine for resident models
//! - `modes`: the modes of the Refine step, which one runs and why
//!   (no feature needed; applying a mode is under `refine`)
//! - `settings` — schema types + cache accessors
//! - `telemetry` — process memory readout
//! - `transcript` — filter pipeline
//! - `stats` — read-side aggregator
//! - `store` — persistence handle
//!
//! # Feature gating
//!
//! Targets the **default** and **tauri** feature flavors. Both pull in
//! the full library surface (including the recognizer subsystem under
//! `--features tauri = ["recognizer"]`). The two `recognizer`-gated
//! re-exports at the bottom (`Recognizer`, `TranscribeResult`,
//! `recognizer_info`) compile only when the `recognizer` feature is on.
//!
//! The **macos-shell** flavor (shipped SwiftUI app) does NOT consume
//! this prelude — Swift drives core via per-module `swift-bridge` FFI
//! signatures declared in `core/src/lib.rs`'s `#[swift_bridge::bridge]`
//! block, not Rust-side `use` statements. macos-shell builds compile
//! this prelude module for free (no swift-bridge code generation
//! against it) but no shell consumer reaches through it.

pub use crate::audio::{AudioDeviceInfo, AudioEngine, SelectedDeviceStatus};
pub use crate::diagnostics::{
    CrashDump, CrashDumpReader, CrashId, ReadError, RecognizerInfo, default_crash_reader,
};
#[cfg(feature = "recognizer")]
pub use crate::diagnostics::recognizer_info;
#[cfg(feature = "ask")]
pub use crate::ask::{
    AskBackend, AskError, AskEvent, AskImage, AskInput, AskPhase, AskPlan, AskReply, AskSetup, AskSnapshot,
    AskStatus, AskSuggestion, CancelFlag,
};
pub use crate::dictation::{DictationSnapshot, Injector, RefineInfo, SessionKind};
pub use crate::media_gate::{MediaController, MediaGateState, PauseDiagnostic};
pub use crate::model_lifecycle::{
    LifecycleState, ModelHandle, StateChangeCallback, apply_keep_warm, on_state_change,
    registry_snapshot,
};
pub use crate::modes::{ModeApp, ModeChoice, ModeEdit, ModeReason, ModeView};
pub use crate::settings::{
    AppDefault, AskSettings, AudioSettings, BUILTIN_MODE_ID, BUILTIN_MODE_NAME, BehaviorSettings,
    ClaudeCliSettings, HotkeyConfig, HotkeyKind, HotkeySettings, HotkeyTarget, Mode, ModesSettings,
    OpenAiSettings, PerformanceSettings, PillSettings, RefineBackendKind, RefineEffort,
    RefineSettings, StatsSettings, keep_models_warm,
};
#[cfg(feature = "refine")]
pub use crate::refine::{
    BackendState, BackendStatus, ConnectionTest, ModeRun, ModeUse, Prompt, RefineError,
    RefineOutcome, RefineStatus, Refiner, apply_mode, backend_status, refine_text,
    refine_text_in_mode, test_connection,
};
pub use crate::stats::StatsSummary;
pub use crate::store::{Store, StoreError};
pub use crate::model_lifecycle::ModelClaim;
pub use crate::telemetry::{
    collect_memory_stats, query_process_memory, query_system_memory, MemoryStats, ModelMemoryRow,
    ProcessMemory, SystemMemory,
};
pub use crate::transcript::FillerLang;

#[cfg(feature = "recognizer")]
pub use crate::recognizer::{Recognizer, TranscribeResult};
#[cfg(feature = "recognizer")]
pub use crate::file_transcription::{
    FileJobSnapshot, FileProgress, FileTranscribeError, FileTranscribeOptions, FileTranscript,
    transcribe_file,
};
