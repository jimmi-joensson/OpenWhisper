use std::thread;
use std::time::Duration;

use openwhisper_core::audio;
use openwhisper_core::dictation::{
    self, PHASE_RECORDING, PHASE_TRANSCRIBING, TOGGLE_BEGIN_RECORDING, TOGGLE_STOP_RECORDING,
};
use openwhisper_core::recognizer;
use serde::Serialize;
use tauri::{Emitter, LogicalPosition, Manager};

mod hotkey;
mod tray;

pub(crate) const TICK_MS: u64 = 50;
const SAMPLE_RATE_HZ: u64 = 16_000;

#[derive(Serialize, Clone)]
struct DictationTick {
    phase: u32,
    status: &'static str,
    status_message: String,
    transcript: String,
    confidence: f32,
    sample_count: u64,
    elapsed_ms: u64,
    error_message: String,
    can_toggle: bool,
    is_recording: bool,
    level: f32,
}

fn phase_to_status(phase: u32) -> &'static str {
    match phase {
        PHASE_RECORDING => "recording",
        PHASE_TRANSCRIBING => "transcribing",
        _ => "idle",
    }
}

#[tauri::command]
fn core_version() -> String {
    openwhisper_core::core_version()
}

/// Shared toggle path. Used by the `dictation_toggle` Tauri command AND the
/// per-platform hotkey threads (Mac CGEventTap, Win Ctrl+Space chord). Phase
/// machine in the core decides whether the toggle starts or stops recording.
pub(crate) fn do_toggle() -> Result<(), String> {
    let action = dictation::dictation_request_toggle();
    match action {
        TOGGLE_BEGIN_RECORDING => {
            // Kick off model load lazily on first record so the UI's
            // "loading model" phase reflects real work. ensure_loaded is
            // idempotent — subsequent toggles short-circuit.
            dictation::dictation_mark_loading_model();
            thread::Builder::new()
                .name("openwhisper-recognizer-load".into())
                .spawn(|| {
                    if let Err(e) = recognizer::recognizer_ensure_loaded() {
                        dictation::dictation_deliver_error(&format!(
                            "recognizer load failed: {e}"
                        ));
                    }
                })
                .expect("spawn recognizer loader");
            audio::audio_start_capture()?;
            dictation::dictation_mark_capture_started();
        }
        TOGGLE_STOP_RECORDING => {
            audio::audio_stop_capture();
            let samples = audio::audio_drain_samples();
            let count = samples.len() as u64;
            dictation::dictation_mark_capture_stopped(count);
            if count > 0 {
                spawn_recognizer(samples);
            }
        }
        _ => {}
    }
    Ok(())
}

/// Shared cancel path. See [`do_toggle`].
pub(crate) fn do_cancel() -> bool {
    audio::audio_stop_capture();
    let _ = audio::audio_drain_samples();
    dictation::dictation_request_cancel()
}

#[tauri::command]
fn dictation_toggle() -> Result<(), String> {
    do_toggle()
}

#[tauri::command]
fn dictation_cancel() -> bool {
    do_cancel()
}

// Decode samples on a worker thread (recognizer call blocks until done).
// Mac path = FluidAudio + ANE; Win path = sherpa-onnx + CPU. See
// core/src/recognizer/mod.rs for the OS-conditional impl.
fn spawn_recognizer(samples: Vec<f32>) {
    thread::Builder::new()
        .name("openwhisper-recognizer-decode".into())
        .spawn(move || {
            // Defensive: recognizer_transcribe requires the engine to be
            // initialized. Loader was kicked off at recording start, but
            // a slow first-load might still be in flight — re-call
            // ensure_loaded so we block until it's ready.
            if let Err(e) = recognizer::recognizer_ensure_loaded() {
                dictation::dictation_deliver_error(&format!("recognizer load: {e}"));
                return;
            }
            match recognizer::recognizer_transcribe(&samples) {
                Ok(res) => dictation::dictation_deliver_transcript(&res.text, res.confidence),
                Err(e) => dictation::dictation_deliver_error(&format!("transcribe: {e}")),
            }
        })
        .expect("spawn recognizer decoder");
}

fn spawn_dictation_emitter(app: tauri::AppHandle) {
    thread::Builder::new()
        .name("openwhisper-dictation-emitter".into())
        .spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(TICK_MS));
                let snap = dictation::dictation_snapshot();
                let level = audio::audio_current_level();
                // While recording, snapshot.sample_count is 0 until stop. Show a
                // running count so the UI counter doesn't sit at 0 throughout.
                let live_samples = if snap.phase() == PHASE_RECORDING {
                    snap.elapsed_ms() * SAMPLE_RATE_HZ / 1000
                } else {
                    snap.sample_count()
                };
                let payload = DictationTick {
                    phase: snap.phase(),
                    status: phase_to_status(snap.phase()),
                    status_message: snap.status_message(),
                    transcript: snap.transcript(),
                    confidence: snap.confidence(),
                    sample_count: live_samples,
                    elapsed_ms: snap.elapsed_ms(),
                    error_message: snap.error_message(),
                    can_toggle: snap.can_toggle(),
                    is_recording: snap.is_recording(),
                    level,
                };
                if app.emit("dictation_tick", payload).is_err() {
                    break;
                }
            }
        })
        .expect("spawn dictation emitter");
}

#[tauri::command]
async fn set_pill_click_through(
    app: tauri::AppHandle,
    passthrough: bool,
) -> Result<(), String> {
    let pill = app
        .get_webview_window("pill")
        .ok_or_else(|| "pill window not found".to_string())?;
    pill.set_ignore_cursor_events(passthrough)
        .map_err(|e| e.to_string())
}

/// Place the pill bottom-center of its current monitor. The 80 px bottom
/// margin clears the Dock / taskbar in the default-layout case; Phase 7
/// will replace this with true work-area math (NSScreen.visibleFrame on
/// Mac, GetMonitorInfo rcWork on Windows).
#[tauri::command]
async fn position_pill_bottom_center(app: tauri::AppHandle) -> Result<(), String> {
    let pill = app
        .get_webview_window("pill")
        .ok_or_else(|| "pill window not found".to_string())?;

    let monitor = match pill.current_monitor().map_err(|e| e.to_string())? {
        Some(m) => m,
        None => pill
            .primary_monitor()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no monitor available".to_string())?,
    };

    let scale = monitor.scale_factor();
    let mon_w = monitor.size().width as f64 / scale;
    let mon_h = monitor.size().height as f64 / scale;
    let mon_x = monitor.position().x as f64 / scale;
    let mon_y = monitor.position().y as f64 / scale;

    const PILL_W: f64 = 70.0;
    const PILL_H: f64 = 22.0;
    const BOTTOM_MARGIN: f64 = 80.0;

    let x = mon_x + (mon_w - PILL_W) / 2.0;
    let y = mon_y + mon_h - PILL_H - BOTTOM_MARGIN;

    pill.set_position(LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    #[cfg(target_os = "windows")]
    let builder = {
        use tauri_plugin_global_shortcut::ShortcutState;
        builder.plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        if let Err(e) = do_toggle() {
                            eprintln!("global shortcut toggle failed: {e}");
                        }
                    }
                })
                .build(),
        )
    };

    builder
        .setup(|app| {
            spawn_dictation_emitter(app.handle().clone());
            tray::install(app.handle())?;
            hotkey::install(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_version,
            dictation_toggle,
            dictation_cancel,
            set_pill_click_through,
            position_pill_bottom_center,
            hotkey::hotkey_retry,
            hotkey::hotkey_status_current,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
