use std::thread;
use std::time::Duration;

use openwhisper_core::audio;
use openwhisper_core::dictation::{
    self, PHASE_RECORDING, PHASE_TRANSCRIBING, TOGGLE_BEGIN_RECORDING, TOGGLE_STOP_RECORDING,
};
use openwhisper_core::recognizer;
use openwhisper_core::transcript;
use openwhisper_core::verbose_log;
use serde::Serialize;
use tauri::{Emitter, LogicalPosition, Manager, WindowEvent};

mod focus;
mod fullscreen;
mod hotkey;
mod injection;
mod permissions;
mod settings;
mod tray;

pub(crate) const TICK_MS: u64 = 50;
const SAMPLE_RATE_HZ: u64 = 16_000;

// Resolve the running bundle's productName at runtime so user-visible copy
// (window title, tray menus, error banners) reflects whether this is the
// release ("OpenWhisper") or the dev overlay ("OpenWhisper Dev"). Single
// source of truth = tauri.conf.json `productName`, optionally overridden
// by tauri.dev.conf.json. Falls back to "OpenWhisper" if the field is
// absent (shouldn't happen in practice).
pub(crate) fn product_name(app: &tauri::AppHandle) -> String {
    app.config()
        .product_name
        .clone()
        .unwrap_or_else(|| "OpenWhisper".to_string())
}

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
    download_bytes_done: u64,
    download_bytes_total: u64,
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
            // Stop capture is cheap (cpal stream teardown). The expensive
            // bit — sinc resampling the buffer to 16 kHz — used to run
            // here on the hotkey thread, blocking the phase transition
            // and freezing the UI on "recording" for ~1–2 s on Windows.
            // Now we flip phase optimistically and let the worker thread
            // drain + resample as the first step of transcription.
            audio::audio_stop_capture();
            dictation::dictation_mark_transcribing_pending();
            spawn_stop_pipeline();
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

// Drain the captured buffer (downmix + sinc resample to 16 kHz) and run
// the recognizer, both on a worker thread. The hotkey thread has already
// flipped phase to TRANSCRIBING via dictation_mark_transcribing_pending,
// so the UI redraws *before* this work starts.
//
// Mac path = FluidAudio + ANE; Win path = sherpa-onnx + CPU. See
// core/src/recognizer/mod.rs for the OS-conditional impl.
fn spawn_stop_pipeline() {
    thread::Builder::new()
        .name("openwhisper-stop-pipeline".into())
        .spawn(move || {
            let t_drain = std::time::Instant::now();
            let samples = audio::audio_drain_samples();
            let count = samples.len() as u64;
            let drain_ms = t_drain.elapsed().as_millis();
            // Empty mic → mark_capture_stopped flips phase back to DONE
            // with "no audio captured". Populated → updates sample_count
            // and reaffirms TRANSCRIBING (no-op vs the optimistic flip).
            dictation::dictation_mark_capture_stopped(count);
            if count == 0 {
                verbose_log!("[ow.dictation] stop empty drain_ms={drain_ms}");
                return;
            }
            // Defensive: recognizer_transcribe requires the engine to be
            // initialized. Loader was kicked off at recording start, but
            // a slow first-load might still be in flight — re-call
            // ensure_loaded so we block until it's ready.
            let t_load = std::time::Instant::now();
            if let Err(e) = recognizer::recognizer_ensure_loaded() {
                dictation::dictation_deliver_error(&format!("recognizer load: {e}"));
                return;
            }
            let load_ms = t_load.elapsed().as_millis();
            let t_tx = std::time::Instant::now();
            match recognizer::recognizer_transcribe(&samples) {
                Ok(res) => {
                    let transcribe_ms = t_tx.elapsed().as_millis();
                    let cleaned = transcript::process(&res.text);
                    verbose_log!(
                        "[ow.dictation] stop drain_ms={drain_ms} ensure_loaded_ms={load_ms} \
                         transcribe_ms={transcribe_ms} samples={count} chars={} confidence={:.2}",
                        cleaned.len(),
                        res.confidence
                    );
                    dictation::dictation_deliver_transcript(&cleaned, res.confidence);
                }
                Err(e) => dictation::dictation_deliver_error(&format!("transcribe: {e}")),
            }
        })
        .expect("spawn stop pipeline");
}

// Cold-loading the recognizer takes ~2.5s on Windows (sherpa-onnx + Parakeet
// int8). Doing it at boot on a background thread means the in-line load
// inside dictation_toggle becomes a no-op once this completes, so the first
// Record click decodes at steady-state latency instead of paying the wait.
// recognizer_ensure_loaded is idempotent, so a slow warmup overlapping a
// fast first Record still yields the same correct result — spawn_recognizer
// blocks on it.
//
// Phase ownership during warmup: we flip dictation phase to LOADING_MODEL
// on entry so the UI surfaces the boot-time download (~487 MB on first
// run). On success we hand control back to IDLE via dictation_mark_loaded
// — that helper only flips IDLE if phase is still LOADING_MODEL, so a
// user-driven recording start that overlaps with the warmup completion
// isn't clobbered. On failure we route through deliver_error so the
// recognizer banner picks it up.
fn spawn_recognizer_warmup() {
    thread::Builder::new()
        .name("openwhisper-recognizer-warmup".into())
        .spawn(|| {
            dictation::dictation_mark_loading_model();
            match recognizer::recognizer_ensure_loaded() {
                Ok(()) => dictation::dictation_mark_loaded(),
                Err(e) => {
                    eprintln!("[warmup] recognizer load failed: {e}");
                    dictation::dictation_deliver_error(&format!("recognizer load: {e}"));
                }
            }
        })
        .expect("spawn recognizer warmup");
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
                    download_bytes_done: snap.download_bytes_done(),
                    download_bytes_total: snap.download_bytes_total(),
                };
                if app.emit("dictation_tick", payload).is_err() {
                    break;
                }
            }
        })
        .expect("spawn dictation emitter");
}

/// Bring the main window forward — invoked when the pill is clicked in
/// idle state. Mirrors the tray's `open_main` behavior so both entry points
/// behave identically.
#[tauri::command]
async fn show_main_window(app: tauri::AppHandle) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    main.show().map_err(|e| e.to_string())?;
    main.unminimize().map_err(|e| e.to_string())?;
    main.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

/// Navigate to the Settings view. Settings is now an in-window route
/// rather than a separate window — bring the main window forward and
/// emit `ow_navigate` so the React tree swaps to the Settings shell.
#[tauri::command]
async fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    main.show().map_err(|e| e.to_string())?;
    main.unminimize().map_err(|e| e.to_string())?;
    main.set_focus().map_err(|e| e.to_string())?;
    app.emit("ow_navigate", "settings")
        .map_err(|e| e.to_string())?;
    Ok(())
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
/// margin is measured from the visible capsule edge, not the window edge —
/// the window includes transparent padding so the CSS drop-shadow has room
/// to render. Phase 7 will replace this with true work-area math
/// (NSScreen.visibleFrame on Mac, GetMonitorInfo rcWork on Windows).
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

    // Window dimensions (must match tauri.conf.json pill window). Capsule
    // is centered inside the window via flex so the shadow has room on all
    // four sides — capsule visible bottom is `CAPSULE_BELOW_PAD` from the
    // window's bottom edge.
    const PILL_WIN_W: f64 = 130.0;
    const PILL_WIN_H: f64 = 82.0;
    const CAPSULE_H: f64 = 22.0;
    const CAPSULE_BELOW_PAD: f64 = (PILL_WIN_H - CAPSULE_H) / 2.0;
    const VISUAL_BOTTOM_MARGIN: f64 = 80.0;

    let x = mon_x + (mon_w - PILL_WIN_W) / 2.0;
    // Solve: window_y + (PILL_WIN_H - CAPSULE_BELOW_PAD) = mon_h - VISUAL_BOTTOM_MARGIN
    let y = mon_y + mon_h - VISUAL_BOTTOM_MARGIN - PILL_WIN_H + CAPSULE_BELOW_PAD;

    pill.set_position(LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Single-instance MUST register on the Builder before .setup() runs:
    // the plugin's callback fires in the *original* process when a second
    // launch is attempted, so it has to be wired before that process is
    // ready to accept callbacks. Mirrors Mac SwiftUI's terminatePriorInstances
    // (project_swiftui_window_lsuielement memory) but inverts the verb —
    // existing process wins, new launch is dropped + focuses the existing
    // window instead of killing the new one.
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init());

    builder
        .setup(|app| {
            // Menu-bar-only — no Dock icon. Matches Superwhisper / Dropbox /
            // the shipped SwiftUI app (which calls
            // NSApp.setActivationPolicy(.accessory) in
            // applicationWillFinishLaunching to avoid the icon flash).
            // Tauri's setup() runs after applicationDidFinishLaunching, so
            // a brief Dock-icon flash on cold boot is the trade-off vs.
            // baking LSUIElement = true into Info.plist; the latter would
            // also remove the app from Force Quit / Activity Monitor's
            // application list, which we want to keep so users can recover
            // from a wedged process.
            #[cfg(target_os = "macos")]
            let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            spawn_dictation_emitter(app.handle().clone());
            spawn_recognizer_warmup();

            // Close-to-tray: intercept the main window's close button and
            // hide the window instead of letting AppKit / Win32 propagate
            // the close. Tray Quit (`app.exit(0)` in tray::install) is the
            // sole true-exit path. Mirrors the SwiftUI shell's
            // `applicationShouldTerminateAfterLastWindowClosed = false`
            // (project_swiftui_window_lsuielement). Pill window isn't
            // user-closeable so it's untouched here.
            if let Some(main) = app.get_webview_window("main") {
                // Window title bar reflects productName so dev overlays as
                // "OpenWhisper Dev" while release stays "OpenWhisper".
                // tauri.conf.json `app.windows[0].title` is the static
                // fallback; we overwrite at runtime so a single config
                // override in tauri.dev.conf.json's productName drives
                // both the bundle name AND the visible chrome.
                let _ = main.set_title(&product_name(app.handle()));

                let main_clone = main.clone();
                main.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = main_clone.hide();
                    }
                });
            }

            // Register the Tauri-side paste flow so the core can call into
            // it from `dictation_deliver_transcript`. Single-set; the core
            // ignores subsequent calls.
            openwhisper_core::dictation::set_injector(Box::new(
                injection::TauriInjector::new(app.handle().clone()),
            ));
            tray::install(app.handle())?;
            // AX-grant watcher: polls `AXIsProcessTrusted()` and brings
            // main forward when the user finishes granting AX in System
            // Settings. No-op on non-Mac platforms.
            focus::install_ax_watcher(app.handle().clone());
            // Load saved hotkeys before install so the backends pick up the
            // user's bindings instead of the platform defaults. First-run
            // returns the default and persists nothing — the file is only
            // written on explicit save.
            let _ = settings::load_settings(app.handle());
            hotkey::install(app.handle());
            // Proactively prompt for Mic on macOS once AX is operationally
            // trusted — mirrors PermissionsCoordinator.swift's "AX before
            // mic" sequencing. Gate on hotkey install having succeeded
            // (CGEventTap created), NOT on AXIsProcessTrusted: the TCC
            // flag false-negatives in dev because ad-hoc cdhash drift
            // invalidates TCC's trusted record on every rebuild
            // (project_tcc_dev_pain), but a working tap is proof that AX
            // is real. Deferred to the next run-loop tick via
            // run_on_main_thread because AVFoundation's
            // requestAccessForMediaType: relies on the Cocoa run loop —
            // setup() runs before NSApp.run() spins it up, so a sync call
            // here goes nowhere.
            let app_for_perm = app.handle().clone();
            let _ = app.handle().run_on_main_thread(move || {
                permissions::request_microphone(&app_for_perm);
            });

            // Fullscreen-aware: when the user enters a fullscreen app, drop
            // the global hotkey so the fullscreen app receives Right Cmd /
            // Ctrl+Space normally, AND hide the pill so we don't paint over
            // games / videos / presentations. Re-arm on exit.
            let app_for_fullscreen = app.handle().clone();
            fullscreen::install(move |is_fullscreen| {
                hotkey::set_active(&app_for_fullscreen, !is_fullscreen);
                if let Some(pill) = app_for_fullscreen.get_webview_window("pill") {
                    let _ = if is_fullscreen { pill.hide() } else { pill.show() };
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_version,
            dictation_toggle,
            dictation_cancel,
            set_pill_click_through,
            position_pill_bottom_center,
            show_main_window,
            open_settings_window,
            hotkey::hotkey_retry,
            hotkey::hotkey_status_current,
            permissions::permissions_status_current,
            settings::settings_get_hotkeys,
            settings::settings_set_hotkey,
            settings::settings_reset_hotkey,
            settings::settings_capture_hotkey_start,
            settings::settings_capture_hotkey_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
