//! Tauri commands for file transcription (TASK-91).
//!
//! Decoding, chunking, the job phase machine and status strings all
//! live in [`openwhisper_core::file_transcription`]. These are thin
//! wrappers React invokes; the file path itself comes from the dialog
//! plugin's picker or the webview's native drag-drop event.

use std::path::PathBuf;

use openwhisper_core::file_transcription::{self, FileJobSnapshot};

#[tauri::command]
pub fn file_transcribe_start(path: String) -> Result<(), String> {
    file_transcription::file_job_start(PathBuf::from(path))
}

#[tauri::command]
pub fn file_transcribe_cancel() -> bool {
    file_transcription::file_job_cancel()
}

#[tauri::command]
pub fn file_transcribe_snapshot() -> FileJobSnapshot {
    file_transcription::file_job_snapshot()
}

#[tauri::command]
pub fn file_transcribe_clear() {
    file_transcription::file_job_clear()
}

/// Extensions for the picker's filter. Sourced from core so the filter
/// never drifts from what the decoder accepts.
#[tauri::command]
pub fn file_transcribe_extensions() -> Vec<&'static str> {
    file_transcription::SUPPORTED_EXTENSIONS.to_vec()
}
