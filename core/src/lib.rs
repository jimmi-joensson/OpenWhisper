pub mod audio;
pub mod dictation;
mod ffi_c;
#[cfg(feature = "recognizer")]
pub mod recognizer;
pub mod transcript;

#[cfg(feature = "macos-shell")]
#[swift_bridge::bridge]
mod ffi {
    extern "Rust" {
        fn hello_from_rust() -> String;
        fn core_version() -> String;

        fn audio_start_capture() -> Result<(), String>;
        fn audio_stop_capture();
        fn audio_drain_samples() -> Vec<f32>;
        fn audio_is_capturing() -> bool;
        fn audio_current_level() -> f32;

        fn process_transcript(text: &str) -> String;

        type DictationSnapshot;
        fn phase(&self) -> u32;
        fn status_message(&self) -> String;
        fn transcript(&self) -> String;
        fn confidence(&self) -> f32;
        fn sample_count(&self) -> u64;
        fn elapsed_ms(&self) -> u64;
        fn error_message(&self) -> String;
        fn can_toggle(&self) -> bool;
        fn is_recording(&self) -> bool;

        fn dictation_snapshot() -> DictationSnapshot;
        fn dictation_request_toggle() -> u32;
        fn dictation_request_cancel() -> bool;
        fn dictation_mark_loading_model();
        fn dictation_mark_capture_started();
        fn dictation_mark_capture_stopped(sample_count: u64);
        fn dictation_deliver_transcript(text: &str, confidence: f32);
        fn dictation_deliver_error(message: &str);
    }
}

fn process_transcript(text: &str) -> String {
    transcript::process(text)
}

use dictation::DictationSnapshot;

fn dictation_snapshot() -> DictationSnapshot {
    dictation::dictation_snapshot()
}

fn dictation_request_toggle() -> u32 {
    dictation::dictation_request_toggle()
}

fn dictation_request_cancel() -> bool {
    dictation::dictation_request_cancel()
}

fn dictation_mark_loading_model() {
    dictation::dictation_mark_loading_model()
}

fn dictation_mark_capture_started() {
    dictation::dictation_mark_capture_started()
}

fn dictation_mark_capture_stopped(sample_count: u64) {
    dictation::dictation_mark_capture_stopped(sample_count)
}

fn dictation_deliver_transcript(text: &str, confidence: f32) {
    dictation::dictation_deliver_transcript(text, confidence)
}

fn dictation_deliver_error(message: &str) {
    dictation::dictation_deliver_error(message)
}

fn hello_from_rust() -> String {
    "Hello from openwhisper-core (Rust)".to_string()
}

pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn audio_start_capture() -> Result<(), String> {
    audio::audio_start_capture()
}

fn audio_stop_capture() {
    audio::audio_stop_capture()
}

fn audio_drain_samples() -> Vec<f32> {
    audio::audio_drain_samples()
}

fn audio_is_capturing() -> bool {
    audio::audio_is_capturing()
}

fn audio_current_level() -> f32 {
    audio::audio_current_level()
}
