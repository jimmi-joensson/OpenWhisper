//! Diagnostics readout for the headless CLI and the in-app
//! diagnostics pane.
//!
//! Two kinds of facts:
//!
//! 1. **Recognizer state** ([`RecognizerInfo`], [`recognizer_info`]) —
//!    which engine is wired in, model version, active execution
//!    provider after `ensure_loaded`. Drives `cli recognizer-info`.
//! 2. **Crash dumps** ([`CrashDumpReader`], [`default_crash_reader`])
//!    — placeholder surface for TASK-78. Today's
//!    [`default_crash_reader`] returns `None` so consumers compile
//!    against the contract; once TASK-78 lands a file-backed reader
//!    the consumer code switches over without redesign.

use std::path::PathBuf;

/// Static engine label baked at compile time per platform. Mac uses
/// FluidInference's ANE-tuned `.mlmodelc` via the Swift bridge;
/// Windows / Linux drive Parakeet ONNX directly through `ort`.
#[cfg(target_os = "macos")]
pub const ENGINE: &str = "FluidAudio";
#[cfg(not(target_os = "macos"))]
pub const ENGINE: &str = "ort+sherpa-onnx";

/// Parakeet model the active build is tuned for. Both Mac and
/// Windows ship the same TDT-v3 0.6B weights; bumping the model
/// requires a coordinated change across `core::recognizer` and
/// (Mac) the FluidAudio Swift bridge.
pub const MODEL_VERSION: &str = "parakeet-tdt-0.6b-v3";

/// Snapshot of the recognizer's current state. Static fields
/// (`engine`, `model_version`) are populated unconditionally; live
/// fields (`model_path`, `ep`) are best-effort and may be `None`
/// when the engine hasn't been initialized yet — call
/// `core::recognizer::recognizer_ensure_loaded` first if you want a
/// fully-populated readout.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct RecognizerInfo {
    pub engine: &'static str,
    pub model_version: &'static str,
    pub model_path: Option<PathBuf>,
    pub ep: Option<String>,
}

impl RecognizerInfo {
    /// Construct manually. `#[non_exhaustive]` blocks struct
    /// expressions from outside the crate; this is the supported
    /// constructor.
    pub fn new(
        engine: &'static str,
        model_version: &'static str,
        model_path: Option<PathBuf>,
        ep: Option<String>,
    ) -> Self {
        Self {
            engine,
            model_version,
            model_path,
            ep,
        }
    }
}

/// Snapshot of the recognizer's current state. `ep` is populated
/// only after the engine has been initialized via
/// `recognizer_ensure_loaded`; before that it returns `None`.
///
/// Lives behind the `recognizer` feature because the active-EP
/// accessor lives on the `Recognizer` trait.
#[cfg(feature = "recognizer")]
pub fn recognizer_info() -> RecognizerInfo {
    RecognizerInfo {
        engine: ENGINE,
        model_version: MODEL_VERSION,
        // Model-on-disk paths aren't surfaced uniformly across
        // backends yet (FluidAudio's `.mlmodelc` lives inside the app
        // bundle and is owned by the Swift side; ort's three .onnx
        // files come from `download::ensure_model()` but the cache
        // doesn't expose them post-build). Both backends know the
        // path internally; future work plumbs it out — for now the
        // CLI prints "<unknown>".
        model_path: None,
        ep: crate::recognizer::active_ep(),
    }
}

// --- crash dumps (placeholder surface for TASK-78) ----------------

/// Opaque crash identifier. The concrete representation lands in
/// TASK-78 (`<timestamp>.json` filename or similar); consumers
/// should treat the value as opaque and round-trip it through
/// `Display` for serialization.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrashId(String);

impl CrashId {
    /// Construct a `CrashId` from a string token. Visibility is
    /// `pub(crate)` until TASK-78 lands the concrete file-backed
    /// reader and decides on the canonical representation.
    pub(crate) fn _new(token: String) -> Self {
        Self(token)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CrashId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Placeholder crash dump. Concrete fields land in TASK-78 once the
/// schema is locked in — backtrace, log tail, app version, OS,
/// recording-state-at-crash. Today the struct is intentionally
/// empty so consumer code can compile against the trait surface.
#[non_exhaustive]
#[derive(Debug, Clone, Default)]
pub struct CrashDump {}

impl CrashDump {
    /// Construct a `CrashDump`. Visibility mirrors `CrashId::_new`
    /// — internal until TASK-78 fleshes the schema out.
    pub(crate) fn _new() -> Self {
        Self {}
    }
}

/// Failure reasons surfaced from `CrashDumpReader::read`. TASK-78
/// may add variants (e.g. `Locked`, `SchemaMismatch`); the
/// `#[non_exhaustive]` keeps that future-compatible.
#[non_exhaustive]
#[derive(Debug)]
pub enum ReadError {
    NotFound,
    Io(std::io::Error),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("crash dump not found"),
            Self::Io(e) => write!(f, "crash dump io: {e}"),
        }
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NotFound => None,
            Self::Io(e) => Some(e),
        }
    }
}

/// Read-side surface for crash dumps. TASK-78 lands the concrete
/// `FileBackedCrashDumpReader` that lists `<crash_dir>/*.json`; this
/// trait shape is the contract CLI Task 8 (`cli crash-dump`) and
/// TASK-78's in-app inspector compile against today.
pub trait CrashDumpReader: Send + Sync {
    fn list(&self) -> Vec<CrashId>;
    fn read(&self, id: &CrashId) -> Result<CrashDump, ReadError>;
}

/// Hook the active crash-dump reader. Today returns `None` so the
/// CLI's `crash-dump` subcommand can register its surface and exit
/// cleanly with a "deferred feature" notice. TASK-78 swaps in a
/// `Some(FileBackedCrashDumpReader)` with no caller-side change.
pub fn default_crash_reader() -> Option<Box<dyn CrashDumpReader>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_is_platform_specific() {
        // Sanity: the const compiled into the binary matches the
        // platform we built for. Catches a future cfg-flip mistake
        // that swaps the strings.
        if cfg!(target_os = "macos") {
            assert_eq!(ENGINE, "FluidAudio");
        } else {
            assert_eq!(ENGINE, "ort+sherpa-onnx");
        }
    }

    #[test]
    fn model_version_is_parakeet_v3() {
        assert_eq!(MODEL_VERSION, "parakeet-tdt-0.6b-v3");
    }

    #[test]
    fn default_crash_reader_returns_none_until_task_78() {
        assert!(default_crash_reader().is_none());
    }

    #[test]
    fn crash_id_round_trips_through_display() {
        let id = CrashId::_new("2026-05-06T12-00-00".into());
        assert_eq!(id.to_string(), "2026-05-06T12-00-00");
        assert_eq!(id.as_str(), "2026-05-06T12-00-00");
    }
}
