//! Transcribe an existing audio or video file (TASK-91).
//!
//! Two layers:
//!
//! - [`transcribe_file`]: synchronous: decode → chunk → recognize →
//!   filter. The CLI calls this directly.
//! - The file *job* ([`file_job_start`], [`file_job_snapshot`],
//!   [`file_job_cancel`], [`file_job_clear`]): one in-flight job on a
//!   worker thread with a pollable snapshot, mirroring how the dictation
//!   phase machine is exposed. Shells poll the snapshot and render it;
//!   phases, progress and status strings are decided here.
//!
//! File jobs share the recognizer with live dictation. Each chunk is a
//! separate `recognizer_transcribe` call, so a dictation stop that lands
//! mid-file waits at most one chunk for the engine. File results never
//! enter the dictation state machine: no paste, no pill, no stats.

mod chunk;
mod decode;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde::Serialize;

use crate::model_lifecycle::LifecycleState;
use crate::{recognizer, transcript};

pub use chunk::MAX_CHUNK_SECS;
pub use decode::{DecodedAudio, decode_media_file};

/// Sample rate the recognizer consumes.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Lower-case file extensions the decoder understands, for file-picker
/// filters. Detection itself probes the content, so a mislabelled file
/// still works when its real format is supported.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "m4a", "aac", "mp4", "m4v", "mov", "ogg", "oga", "opus", "webm", "mkv", "flac",
    "aif", "aiff", "caf",
];

/// Options for [`transcribe_file`].
#[non_exhaustive]
#[derive(Debug, Clone, Default)]
pub struct FileTranscribeOptions {
    /// Skip the transcript filter pipeline (filler stripping,
    /// substitutions, whitespace normalize).
    pub raw: bool,
}

impl FileTranscribeOptions {
    pub fn raw(mut self, raw: bool) -> Self {
        self.raw = raw;
        self
    }
}

/// Progress callbacks from [`transcribe_file`], in order.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileProgress {
    Decoding,
    /// Decoded; `audio_ms` is the recording length.
    Decoded { audio_ms: u64 },
    /// Recognizer was not resident and is loading.
    LoadingModel,
    /// `done` of `total` chunks recognized.
    Transcribing { done: usize, total: usize },
}

/// Result of [`transcribe_file`].
#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct FileTranscript {
    pub text: String,
    /// Duration-weighted mean of per-chunk recognizer confidence.
    pub confidence: f32,
    pub audio_ms: u64,
    pub decode_ms: u64,
    pub transcribe_ms: u64,
    pub chunks: usize,
}

/// Why [`transcribe_file`] stopped without a transcript.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileTranscribeError {
    /// The cancel flag was raised; stopped at a chunk boundary.
    Cancelled,
    Failed(String),
}

impl std::fmt::Display for FileTranscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Failed(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for FileTranscribeError {}

/// Decode `path`, recognize it chunk by chunk, and return the joined
/// transcript. Blocks; run it on a worker thread. `cancel` is checked
/// between chunks.
pub fn transcribe_file(
    path: &Path,
    opts: &FileTranscribeOptions,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(FileProgress),
) -> Result<FileTranscript, FileTranscribeError> {
    use FileTranscribeError::{Cancelled, Failed};

    on_progress(FileProgress::Decoding);
    let t_decode = Instant::now();
    let audio = decode_media_file(path).map_err(Failed)?;
    let decode_ms = t_decode.elapsed().as_millis() as u64;
    let audio_ms = audio.duration_ms();
    on_progress(FileProgress::Decoded { audio_ms });
    crate::verbose_log!(
        "[ow.file] decoded {} in {decode_ms} ms: {audio_ms} ms audio, src {} Hz x{}",
        path.display(),
        audio.source_rate,
        audio.source_channels,
    );

    if cancel.load(Ordering::Relaxed) {
        return Err(Cancelled);
    }

    let resident = matches!(
        recognizer::engine_state(),
        Some(LifecycleState::Loaded | LifecycleState::Active)
    );
    if !resident {
        on_progress(FileProgress::LoadingModel);
    }
    recognizer::recognizer_ensure_loaded().map_err(|e| Failed(format!("recognizer load: {e}")))?;

    let chunks = chunk::split_points(&audio.samples);
    let total = chunks.len();
    let t_tx = Instant::now();
    let mut texts = Vec::with_capacity(total);
    let mut conf_weighted = 0.0f64;
    let mut conf_weight = 0usize;
    on_progress(FileProgress::Transcribing { done: 0, total });
    for (i, range) in chunks.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(Cancelled);
        }
        let len = range.len();
        if len > 0 {
            let res = recognizer::recognizer_transcribe(&audio.samples[range])
                .map_err(|e| Failed(format!("transcribe: {e}")))?;
            let piece = res.text.trim();
            if !piece.is_empty() {
                texts.push(piece.to_string());
            }
            conf_weighted += res.confidence as f64 * len as f64;
            conf_weight += len;
        }
        on_progress(FileProgress::Transcribing { done: i + 1, total });
    }
    let transcribe_ms = t_tx.elapsed().as_millis() as u64;

    let joined = texts.join(" ");
    let text = if opts.raw {
        joined
    } else {
        // process() appends a trailing space for back-to-back pastes;
        // a file transcript is a standalone block.
        transcript::process(&joined).trim().to_string()
    };
    let confidence = if conf_weight == 0 {
        0.0
    } else {
        (conf_weighted / conf_weight as f64) as f32
    };

    Ok(FileTranscript {
        text,
        confidence,
        audio_ms,
        decode_ms,
        transcribe_ms,
        chunks: total,
    })
}

// ---------------------------------------------------------------------
// File job: single in-flight transcription with a pollable snapshot.

pub const FILE_PHASE_IDLE: u32 = 0;
pub const FILE_PHASE_DECODING: u32 = 1;
pub const FILE_PHASE_LOADING_MODEL: u32 = 2;
pub const FILE_PHASE_TRANSCRIBING: u32 = 3;
pub const FILE_PHASE_DONE: u32 = 4;
pub const FILE_PHASE_ERROR: u32 = 5;
pub const FILE_PHASE_CANCELLED: u32 = 6;

/// Point-in-time view of the file job. Shells render this as-is.
#[non_exhaustive]
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct FileJobSnapshot {
    /// One of the `FILE_PHASE_*` constants.
    pub phase: u32,
    /// True while a worker is running (decoding, loading, transcribing,
    /// or winding down after a cancel).
    pub busy: bool,
    /// User-facing status line ("Reading audio…", "Transcribing… 40%").
    pub status: String,
    pub path: String,
    pub file_name: String,
    /// 0.0..=1.0 across the recognition stage; 0 while decoding/loading.
    pub progress: f32,
    pub chunks_done: usize,
    pub chunks_total: usize,
    /// Recording length, known once decoding finishes.
    pub audio_ms: u64,
    /// Wall time since the job started (frozen once it ends).
    pub elapsed_ms: u64,
    pub text: String,
    pub confidence: f32,
    pub error: String,
    /// Cancel requested; the worker stops at the next chunk boundary.
    pub cancelling: bool,
}

struct Job {
    snap: FileJobSnapshot,
    started: Option<Instant>,
}

fn job() -> &'static Mutex<Job> {
    static JOB: OnceLock<Mutex<Job>> = OnceLock::new();
    JOB.get_or_init(|| {
        Mutex::new(Job {
            snap: FileJobSnapshot::default(),
            started: None,
        })
    })
}

fn cancel_flag() -> &'static AtomicBool {
    static CANCEL: AtomicBool = AtomicBool::new(false);
    &CANCEL
}

fn with_job<R>(f: impl FnOnce(&mut Job) -> R) -> R {
    let mut guard = job().lock().unwrap_or_else(|p| p.into_inner());
    f(&mut guard)
}

/// Start transcribing `path` on a background thread. Fails if a job is
/// already running or the file doesn't exist; the previous job's result
/// is replaced.
pub fn file_job_start(path: PathBuf) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!("not a file: {}", path.display()));
    }
    with_job(|j| {
        if j.snap.busy {
            return Err("a file is already being transcribed".to_string());
        }
        cancel_flag().store(false, Ordering::Relaxed);
        j.started = Some(Instant::now());
        j.snap = FileJobSnapshot {
            phase: FILE_PHASE_DECODING,
            busy: true,
            status: status_for(FILE_PHASE_DECODING, 0.0, false, ""),
            path: path.display().to_string(),
            file_name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            ..Default::default()
        };
        Ok(())
    })?;

    let spawned = std::thread::Builder::new()
        .name("openwhisper-file-transcribe".into())
        .spawn(move || run_job(path));
    if let Err(e) = spawned {
        finish(Err(FileTranscribeError::Failed(format!("spawn worker: {e}"))));
        return Err(format!("spawn worker: {e}"));
    }
    Ok(())
}

/// Ask the running job to stop. Returns `false` when nothing is running.
pub fn file_job_cancel() -> bool {
    with_job(|j| {
        if !j.snap.busy {
            return false;
        }
        cancel_flag().store(true, Ordering::Relaxed);
        j.snap.cancelling = true;
        j.snap.status = "Cancelling…".to_string();
        true
    })
}

/// Current job state. Cheap; safe to poll at UI frame rates.
pub fn file_job_snapshot() -> FileJobSnapshot {
    with_job(|j| {
        let mut s = j.snap.clone();
        if let (true, Some(t)) = (s.busy, j.started) {
            s.elapsed_ms = t.elapsed().as_millis() as u64;
        }
        s
    })
}

/// Drop a finished job's result and return to idle. No-op while busy.
pub fn file_job_clear() {
    with_job(|j| {
        if !j.snap.busy {
            j.snap = FileJobSnapshot::default();
            j.started = None;
        }
    });
}

fn run_job(path: PathBuf) {
    let mut on_progress = |p: FileProgress| {
        with_job(|j| {
            let s = &mut j.snap;
            match p {
                FileProgress::Decoding => s.phase = FILE_PHASE_DECODING,
                FileProgress::Decoded { audio_ms } => s.audio_ms = audio_ms,
                FileProgress::LoadingModel => s.phase = FILE_PHASE_LOADING_MODEL,
                FileProgress::Transcribing { done, total } => {
                    s.phase = FILE_PHASE_TRANSCRIBING;
                    s.chunks_done = done;
                    s.chunks_total = total;
                    s.progress = if total == 0 { 1.0 } else { done as f32 / total as f32 };
                }
            }
            if !s.cancelling {
                s.status = status_for(s.phase, s.progress, false, "");
            }
        });
    };
    let opts = FileTranscribeOptions::default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        transcribe_file(&path, &opts, cancel_flag(), &mut on_progress)
    }))
    .unwrap_or_else(|_| Err(FileTranscribeError::Failed("transcription crashed".to_string())));
    finish(result);
}

fn finish(result: Result<FileTranscript, FileTranscribeError>) {
    with_job(|j| {
        let s = &mut j.snap;
        if let Some(t) = j.started {
            s.elapsed_ms = t.elapsed().as_millis() as u64;
        }
        s.busy = false;
        s.cancelling = false;
        match result {
            Ok(t) => {
                s.phase = FILE_PHASE_DONE;
                s.progress = 1.0;
                s.audio_ms = t.audio_ms;
                s.confidence = t.confidence;
                s.text = t.text;
            }
            Err(FileTranscribeError::Cancelled) => s.phase = FILE_PHASE_CANCELLED,
            Err(FileTranscribeError::Failed(msg)) => {
                s.phase = FILE_PHASE_ERROR;
                s.error = msg;
            }
        }
        s.status = status_for(s.phase, s.progress, s.text.trim().is_empty(), &s.error);
    });
}

/// User-facing status line for a phase. Lives here so every shell says
/// the same thing.
fn status_for(phase: u32, progress: f32, empty_text: bool, error: &str) -> String {
    match phase {
        FILE_PHASE_DECODING => "Reading audio…".to_string(),
        FILE_PHASE_LOADING_MODEL => "Loading Parakeet…".to_string(),
        FILE_PHASE_TRANSCRIBING => format!("Transcribing… {}%", (progress * 100.0).round() as u32),
        FILE_PHASE_DONE if empty_text => "No speech found in this file".to_string(),
        FILE_PHASE_DONE => "Done".to_string(),
        FILE_PHASE_ERROR => format!("Couldn't transcribe this file: {error}"),
        FILE_PHASE_CANCELLED => "Cancelled".to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_strings() {
        assert_eq!(status_for(FILE_PHASE_TRANSCRIBING, 0.404, false, ""), "Transcribing… 40%");
        assert_eq!(status_for(FILE_PHASE_DONE, 1.0, true, ""), "No speech found in this file");
        assert_eq!(
            status_for(FILE_PHASE_ERROR, 0.0, true, "file has no audio track"),
            "Couldn't transcribe this file: file has no audio track"
        );
        assert_eq!(status_for(FILE_PHASE_IDLE, 0.0, true, ""), "");
    }

    /// Real decode + recognizer through the job API. Needs the model, so
    /// opt-in: `OW_FILE_SMOKE=/path/to/file cargo test -p openwhisper-core
    /// --no-default-features --features recognizer --release -- --ignored
    /// file_job_smoke --nocapture`
    #[test]
    #[ignore]
    fn file_job_smoke() {
        let path = std::env::var("OW_FILE_SMOKE").expect("set OW_FILE_SMOKE");
        file_job_start(PathBuf::from(path)).unwrap();
        let mut seen = Vec::new();
        loop {
            let s = file_job_snapshot();
            if seen.last() != Some(&s.status) {
                eprintln!("[{}] {}", s.elapsed_ms, s.status);
                seen.push(s.status.clone());
            }
            if !s.busy {
                assert_eq!(s.phase, FILE_PHASE_DONE, "{}", s.error);
                assert!(!s.text.is_empty());
                eprintln!("{}", s.text);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn start_rejects_missing_file_without_touching_state() {
        let before = file_job_snapshot();
        assert!(file_job_start(PathBuf::from("/definitely/not/here.wav")).is_err());
        assert_eq!(file_job_snapshot().phase, before.phase);
        assert!(!file_job_cancel() || before.busy);
    }
}
