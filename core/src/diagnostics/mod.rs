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

// --- crash dumps (TASK-78) -----------------------------------------

/// Opaque crash identifier. Wraps the unix-ms filename stem stored
/// on disk; consumers should treat the value as opaque and
/// round-trip via `Display` / `as_str()`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrashId(String);

impl CrashId {
    /// Construct a `CrashId` from a string token. Public so the
    /// CLI can build ids from a `--id` flag without going through
    /// the trait. Validation that the token corresponds to an
    /// actual file on disk is the reader's job.
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
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

/// A read crash dump. Re-exports the on-disk schema from
/// `crate::crashes` so the diagnostics trait surface and the
/// concrete file format are the same type — no double-marshal.
pub type CrashDump = crate::crashes::CrashFile;

/// Failure reasons surfaced from `CrashDumpReader::read`.
#[non_exhaustive]
#[derive(Debug)]
pub enum ReadError {
    UnsafeId(String),
    NotFound,
    Io(std::io::Error),
    Parse(serde_json::Error),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsafeId(id) => write!(f, "invalid crash id: {id}"),
            Self::NotFound => f.write_str("crash dump not found"),
            Self::Io(e) => write!(f, "crash dump io: {e}"),
            Self::Parse(e) => write!(f, "crash dump parse: {e}"),
        }
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Parse(e) => Some(e),
            _ => None,
        }
    }
}

impl From<crate::crashes::ReadCrashError> for ReadError {
    fn from(err: crate::crashes::ReadCrashError) -> Self {
        match err {
            crate::crashes::ReadCrashError::UnsafeId(id) => Self::UnsafeId(id),
            crate::crashes::ReadCrashError::NotFound => Self::NotFound,
            crate::crashes::ReadCrashError::Io(e) => Self::Io(e),
            crate::crashes::ReadCrashError::Parse(e) => Self::Parse(e),
        }
    }
}

/// Read-side surface for crash dumps. The CLI (`cli crash-dump`)
/// and the Tauri shell's `crashes_list` / `crashes_read` commands
/// both consume this trait via [`default_crash_reader`] so the
/// "where do crashes live + how do you read them" question has one
/// answer in core.
pub trait CrashDumpReader: Send + Sync {
    fn list(&self) -> Vec<CrashDump>;
    fn read(&self, id: &CrashId) -> Result<CrashDump, ReadError>;
}

/// File-backed reader over `<dir>/<unix-ms>.json` files. The
/// canonical implementation behind [`default_crash_reader`].
/// Construct with an explicit dir to inspect crashes from the dev
/// bundle (`com.openwhisper.dev`) or a Playwright fixture — the
/// `default_crash_reader` resolver only knows the release path.
pub struct FileBackedCrashDumpReader {
    dir: PathBuf,
}

impl FileBackedCrashDumpReader {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }
}

impl CrashDumpReader for FileBackedCrashDumpReader {
    fn list(&self) -> Vec<CrashDump> {
        match crate::crashes::list_crashes(&self.dir) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "[diagnostics] list_crashes {}: {e}",
                    self.dir.display(),
                );
                Vec::new()
            }
        }
    }

    fn read(&self, id: &CrashId) -> Result<CrashDump, ReadError> {
        crate::crashes::read_crash(&self.dir, id.as_str()).map_err(Into::into)
    }
}

/// Hook the active crash-dump reader. Returns `Some` when the
/// OS-default crash dir resolves (macOS / Windows with the right
/// env vars set), `None` on platforms without an established crash
/// path. CLI surfaces print a clean "no crash dir" notice on `None`
/// rather than blowing up.
pub fn default_crash_reader() -> Option<Box<dyn CrashDumpReader>> {
    let dir = crate::crashes::default_crash_dir()?;
    Some(Box::new(FileBackedCrashDumpReader::new(dir)))
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
    fn crash_id_round_trips_through_display() {
        let id = CrashId::new("1717503600123");
        assert_eq!(id.to_string(), "1717503600123");
        assert_eq!(id.as_str(), "1717503600123");
    }

    #[test]
    fn file_backed_reader_lists_and_reads() {
        use crate::crashes::{
            CrashFile, RustPanic, SCHEMA_VERSION, write_crash_file,
        };
        let tmp = tempfile::tempdir().unwrap();
        let mk = |id: &str, ts: i64, msg: &str| CrashFile {
            schema_version: SCHEMA_VERSION,
            id: id.into(),
            ts_unix_ms: ts,
            app_version: "0.6.0".into(),
            os: "macos (arm64)".into(),
            rust_panic: RustPanic {
                thread_name: "main".into(),
                message: msg.into(),
                location: "x.rs:1:1".into(),
                backtrace: "<stub>".into(),
            },
            recording_state: None,
            events: vec![],
        };
        let older = mk("100", 100, "older crash");
        let newer = mk("200", 200, "newer crash");
        write_crash_file(&older, tmp.path()).unwrap();
        write_crash_file(&newer, tmp.path()).unwrap();

        let reader =
            FileBackedCrashDumpReader::new(tmp.path().to_path_buf());
        let listed = reader.list();
        assert_eq!(listed.len(), 2);
        // Newest first.
        assert_eq!(listed[0].id, "200");
        assert_eq!(listed[1].id, "100");

        let dump = reader.read(&CrashId::new("100")).unwrap();
        assert_eq!(dump.rust_panic.message, "older crash");

        // Missing → NotFound, not Io.
        assert!(matches!(
            reader.read(&CrashId::new("999")),
            Err(ReadError::NotFound)
        ));
        // Path-traversal-shaped → UnsafeId, never reaches the disk.
        assert!(matches!(
            reader.read(&CrashId::new("../etc/passwd")),
            Err(ReadError::UnsafeId(_))
        ));
    }
}
