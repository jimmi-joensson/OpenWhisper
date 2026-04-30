use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
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
use tauri::{Emitter, Listener, Manager, WindowEvent};
#[cfg(target_os = "macos")]
use tauri::LogicalPosition;
#[cfg(not(target_os = "macos"))]
use tauri::PhysicalPosition;

mod behavior;
mod focus;
mod fullscreen;
mod hotkey;
mod injection;
mod permissions;
mod settings;
mod tray;

// The pill needs to be a real NSPanel (not NSWindow) so it can render
// over other apps' fullscreen Spaces on macOS Sonoma+. tauri-nspanel
// swizzles the window's class in place; the macro below declares the
// panel config the conversion uses.
#[cfg(target_os = "macos")]
tauri_nspanel::tauri_panel! {
    panel!(PillPanel {
        config: {
            // Pill never takes keyboard focus — the user's typing target
            // is the app behind it. nonactivating style mask is set
            // separately at conversion time.
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

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

#[derive(Serialize, Clone, PartialEq, Eq, Hash)]
pub struct AudioDevice {
    /// Stable cpal device id (Display form). Persisted by the picker; survives
    /// reboots and reconnections, so a saved selection rebinds even if the
    /// device's friendly name changes (driver reinstall, OS rename).
    id: String,
    /// Discord/Windows-Sound-style label. On Windows this is the
    /// `PKEY_Device_FriendlyName` (e.g. "Microphone (SteelSeries Arctis 5
    /// Chat)"); on other platforms it's the cpal description name.
    label: String,
    is_default: bool,
}

// Snapshot the React Audio pane subscribes to. `selected_present` lets the
// UI render the picker as System default when the saved device isn't
// enumerable, and `default_label` powers the Discord-style
// "System default (<device label>)" row so the user can see which device
// the system default currently resolves to.
#[derive(Serialize, Clone, PartialEq, Eq, Hash)]
pub struct AudioDeviceState {
    devices: Vec<AudioDevice>,
    selected_id: Option<String>,
    selected_present: bool,
    default_label: Option<String>,
}

// Latest computed device-state snapshot. The emitter recomputes every 2 s;
// the on-demand `audio_get_device_state` command returns this cache so the
// React Audio pane mount doesn't pay the cpal enumerate cost (which on
// macOS adds up to ~1 s — three CoreAudio property scans per call before
// this cache landed). First-mount-before-first-tick falls through to a
// synchronous compute that seeds the cache.
static CACHED_AUDIO_DEVICE_STATE: Mutex<Option<AudioDeviceState>> = Mutex::new(None);

// One cpal enumerate, derive everything (default label, selected_present)
// from the result. Three sequential enumerations was the thing making the
// pane sluggish on macOS — `default_input_config()` is per-device CoreAudio
// I/O and Bluetooth/Continuity Camera devices are particularly slow to
// answer.
fn compute_audio_device_state() -> AudioDeviceState {
    // Boot-time gate: cpal's macOS backend touches CoreAudio property
    // queries on enumerate. Sequoia's TCC has fired the mic dialog from
    // those reads when Accessibility is still mid-prompt — racing the
    // boot prompt sequence (AX → mic). While not authorized we return a
    // safe placeholder: empty device list, saved id preserved, no
    // disconnected marker. The next emitter tick after authorization
    // pushes the real state and the UI catches up via its subscription.
    if !permissions::is_mic_authorized() {
        return AudioDeviceState {
            devices: Vec::new(),
            selected_id: audio::audio_get_selected_device_id(),
            selected_present: true,
            default_label: None,
        };
    }
    let devices: Vec<AudioDevice> = audio::audio_list_input_devices()
        .into_iter()
        .map(|d| AudioDevice { id: d.id, label: d.label, is_default: d.is_default })
        .collect();
    let selected_id = audio::audio_get_selected_device_id();
    let default_label = devices
        .iter()
        .find(|d| d.is_default)
        .map(|d| d.label.clone());
    let selected_present = match selected_id.as_deref() {
        Some(id) => devices.iter().any(|d| d.id == id),
        // No selection = capture uses host default. Treat as "present" so
        // the UI doesn't render a disconnected marker on the empty option.
        None => true,
    };
    AudioDeviceState { devices, selected_id, selected_present, default_label }
}

// Recompute and write through to the cache. Used by the emitter loop;
// the Tauri command reads the cache without recomputing.
fn refresh_audio_device_state_cache() -> AudioDeviceState {
    let state = compute_audio_device_state();
    if let Ok(mut g) = CACHED_AUDIO_DEVICE_STATE.lock() {
        *g = Some(state.clone());
    }
    state
}

#[tauri::command]
fn audio_get_device_state() -> AudioDeviceState {
    if let Some(cached) = CACHED_AUDIO_DEVICE_STATE.lock().ok().and_then(|g| g.clone()) {
        return cached;
    }
    // First mount before the emitter has had a chance to seed the cache.
    // Pay the cpal enumerate cost once and warm the cache for next time.
    refresh_audio_device_state_cache()
}

#[tauri::command]
fn audio_preview_start() -> Result<(), String> {
    // AC #3: preview is mutually exclusive with an active recording.
    // The hotkey path can't slip in between this check and start_preview
    // because audio_start_capture itself stops the preview, but if a
    // recording IS already in flight we'd otherwise get a confusing
    // "preview rejected" error from the worker — return the precise
    // reason here instead.
    if dictation::is_recording() {
        return Err("recording in progress".into());
    }
    audio::audio_preview_start()
}

#[tauri::command]
fn audio_preview_stop() {
    audio::audio_preview_stop();
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

// One device-state poll every N dictation ticks. 50 ms tick × 40 = 2 s.
// Slow enough that the cpal enumerate isn't a hot path; fast enough that
// an unplugged mic surfaces in the picker before the user clicks Test.
const DEVICE_STATE_TICK_DIVISOR: u64 = 40;

fn hash_device_state(state: &AudioDeviceState) -> u64 {
    let mut h = DefaultHasher::new();
    state.hash(&mut h);
    h.finish()
}

fn spawn_dictation_emitter(app: tauri::AppHandle) {
    thread::Builder::new()
        .name("openwhisper-dictation-emitter".into())
        .spawn(move || {
            // Force an emit on the first device-state tick so React's
            // listener replaces the initial `audio_get_device_state`
            // snapshot with a live one (host-default may have changed
            // between mount and first tick on a slow boot).
            let mut last_device_hash: Option<u64> = None;
            let mut tick_count: u64 = 0;
            loop {
                thread::sleep(Duration::from_millis(TICK_MS));
                tick_count = tick_count.wrapping_add(1);
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
                if tick_count % DEVICE_STATE_TICK_DIVISOR == 0 {
                    let state = refresh_audio_device_state_cache();
                    let hash = hash_device_state(&state);
                    if last_device_hash != Some(hash) {
                        last_device_hash = Some(hash);
                        if app.emit("audio_device_state", state).is_err() {
                            break;
                        }
                    }
                }
            }
        })
        .expect("spawn dictation emitter");
}

/// Center the main window on the monitor that hosts the pill (the
/// screen the user just clicked from). Falls back to the cursor's
/// monitor — and finally the pill's reported `current_monitor` — so
/// non-follow users (no `LAST_MONITOR` recorded) still get centered
/// placement instead of "wherever main was last hidden".
///
/// MUST be called on the main thread — `available_monitors()` and
/// `outer_size()` go through main-thread-only paths on macOS.
fn center_main_on_pill_monitor(app: &tauri::AppHandle) {
    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let pill = app.get_webview_window("pill");
    let monitor = fullscreen::last_pill_monitor()
        .and_then(|origin| fullscreen::find_tauri_monitor(app, origin))
        .or_else(|| {
            fullscreen::cursor_monitor()
                .and_then(|origin| fullscreen::find_tauri_monitor(app, origin))
        })
        .or_else(|| pill.as_ref().and_then(|p| p.current_monitor().ok().flatten()));
    let Some(monitor) = monitor else { return };
    // Math in unified logical pts. See `place_pill` for why we dispatch
    // `set_position` per platform via `set_window_at_logical`.
    let mon_scale = monitor.scale_factor();
    let mon_x = monitor.position().x as f64 / mon_scale;
    let mon_y = monitor.position().y as f64 / mon_scale;
    let mon_w = monitor.size().width as f64 / mon_scale;
    let mon_h = monitor.size().height as f64 / mon_scale;

    let Ok(size) = main.outer_size() else { return };
    // `outer_size()` is physical px on the window's CURRENT monitor.
    // Convert via the window's own scale to get logical-pt size — the
    // value is invariant across monitors (Tauri auto-resizes on cross-DPI
    // moves to keep logical dims constant).
    let win_scale = main.scale_factor().unwrap_or(mon_scale);
    let win_w = size.width as f64 / win_scale;
    let win_h = size.height as f64 / win_scale;

    let x = mon_x + (mon_w - win_w) / 2.0;
    let y = mon_y + (mon_h - win_h) / 2.0;
    let _ = set_window_at_logical(&main, x, y, mon_scale);
}

/// Position a window at logical-pt coords in the unified primary-relative
/// coord space (top-left of primary = (0, 0), Y-down). Platform-aware
/// because Tao's `set_outer_position` scales differently:
///
/// - macOS: takes a `LogicalPosition` verbatim (Cocoa Y conversion uses
///   the primary screen's logical height — `CGDisplay::main().pixels_high`
///   returns logical pts on Mac despite the name).
/// - Windows: scales the position by the **window's current** scale
///   factor, which is the *source* monitor's scale on a cross-DPI move.
///   Pre-multiplying by the *destination* scale and passing
///   `PhysicalPosition` sidesteps that.
fn set_window_at_logical(
    window: &tauri::WebviewWindow,
    x_log: f64,
    y_log: f64,
    dest_scale: f64,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = dest_scale;
        window
            .set_position(LogicalPosition::new(x_log, y_log))
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let x_phys = (x_log * dest_scale).round() as i32;
        let y_phys = (y_log * dest_scale).round() as i32;
        window
            .set_position(PhysicalPosition::new(x_phys, y_phys))
            .map_err(|e| e.to_string())
    }
}

/// Bring the main window forward — invoked when the pill is clicked in
/// idle state. Mirrors the tray's `open_main` behavior so both entry points
/// behave identically.
#[tauri::command]
async fn show_main_window(app: tauri::AppHandle) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let app_clone = app.clone();
    let _ = app
        .run_on_main_thread(move || center_main_on_pill_monitor(&app_clone))
        .map_err(|e| e.to_string());
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
    let app_clone = app.clone();
    let _ = app
        .run_on_main_thread(move || center_main_on_pill_monitor(&app_clone))
        .map_err(|e| e.to_string());
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

/// Last (x, y) we passed to `pill.set_position` — used by `place_pill`
/// to no-op when the periodic refresh task ticks but nothing changed.
/// Logical points, Quartz top-left origin (matches `LogicalPosition`
/// units passed to Tauri).
static LAST_PILL_POSITION: Mutex<Option<(f64, f64)>> = Mutex::new(None);

/// Place the pill bottom-center of a chosen monitor, anchored 24 px
/// above the bottom edge of that monitor's *work area* — the top of
/// the Dock on Mac, the top of the taskbar on Windows, or the screen's
/// bottom edge when neither is on this monitor. Tracks Dock/taskbar
/// resize via `fullscreen::work_area_bottom_y` (NSScreen.visibleFrame
/// on Mac, MONITORINFO.rcWork on Windows).
///
/// `monitor_origin` is opaque to this function: when `Some`, it gets
/// passed straight to `fullscreen::find_tauri_monitor` which knows how
/// to turn it back into a `tauri::Monitor` per platform (mac side
/// converts logical→physical to match Tauri's coordinate space; win
/// compares directly). On no-match — e.g. display unplugged between
/// the watcher tick and this call — falls back to `current_monitor()`,
/// then `primary_monitor()`, so the pill always lands somewhere.
///
/// Self-dedupes via `LAST_PILL_POSITION`: skips the `set_position`
/// call when the computed target equals the last one we set. The
/// periodic refresh task in `setup()` therefore costs nothing while
/// the user isn't moving the cursor or resizing the Dock.
///
/// MUST be called on the main thread: `available_monitors()` and
/// `NSScreen.visibleFrame` are main-thread-only on macOS. Both the
/// Tauri command wrapper and the watcher / refresh callbacks dispatch
/// via `app.run_on_main_thread`.
fn place_pill(app: &tauri::AppHandle, monitor_origin: Option<(i32, i32)>) -> Result<(), String> {
    let pill = app
        .get_webview_window("pill")
        .ok_or_else(|| "pill window not found".to_string())?;

    let monitor = monitor_origin
        .and_then(|origin| fullscreen::find_tauri_monitor(app, origin))
        .or_else(|| pill.current_monitor().ok().flatten())
        .or_else(|| pill.primary_monitor().ok().flatten())
        .ok_or_else(|| "no monitor available".to_string())?;

    // Math in unified LOGICAL pts (primary's top-left = origin, Y-down) —
    // the same space `monitor.position() / monitor.scale_factor()` lands
    // in. We dispatch `set_position` per-platform: Mac via
    // `LogicalPosition` (Tao passes it verbatim and Cocoa Y is converted
    // via primary's logical height), Win via `PhysicalPosition` computed
    // from the *destination* monitor's scale (Tao on Win would otherwise
    // scale a `LogicalPosition` by the **window's current** DPI factor —
    // wrong axis when the window crosses monitors).
    let scale = monitor.scale_factor();
    let mon_x = monitor.position().x as f64 / scale;
    let mon_y = monitor.position().y as f64 / scale;
    let mon_w = monitor.size().width as f64 / scale;
    let mon_h = monitor.size().height as f64 / scale;
    let mon_bottom = mon_y + mon_h;

    // Pill window dimensions in logical points (must match tauri.conf.json).
    // Capsule is centered inside the window via flex so the shadow has room
    // on all four sides — capsule visible bottom is `CAPSULE_BELOW_PAD`
    // from the window's bottom edge.
    const PILL_WIN_W: f64 = 130.0;
    const PILL_WIN_H: f64 = 82.0;
    const CAPSULE_H: f64 = 22.0;
    const CAPSULE_BELOW_PAD: f64 = (PILL_WIN_H - CAPSULE_H) / 2.0;
    /// Logical-pt gap between the capsule's bottom edge and the Dock /
    /// taskbar when one is present on this screen.
    const ABOVE_DOCK_GAP: f64 = 24.0;
    /// Logical-pt gap when no Dock / taskbar — the pill needs visible
    /// margin above the screen's bottom edge so it doesn't sit at the
    /// seam between stacked monitors.
    const ABOVE_BARE_EDGE_GAP: f64 = 80.0;

    let work_area_bottom = fullscreen::work_area_bottom_y(&monitor);
    let gap = if (work_area_bottom - mon_bottom).abs() < 1.0 {
        ABOVE_BARE_EDGE_GAP
    } else {
        ABOVE_DOCK_GAP
    };

    let x = mon_x + (mon_w - PILL_WIN_W) / 2.0;
    // Solve: window_y + (PILL_WIN_H - CAPSULE_BELOW_PAD) = work_area_bottom - gap
    let y = work_area_bottom - gap - PILL_WIN_H + CAPSULE_BELOW_PAD;

    {
        let mut last = LAST_PILL_POSITION.lock().unwrap();
        // Sub-pixel jitter would otherwise cause needless `set_position`
        // churn during periodic refresh. 0.5 px is below user-visible.
        if let Some((lx, ly)) = *last {
            if (lx - x).abs() < 0.5 && (ly - y).abs() < 0.5 {
                return Ok(());
            }
        }
        *last = Some((x, y));
    }

    set_window_at_logical(&pill, x, y, scale)
        .map_err(|e| e.to_string())
}

/// Apply the gating side-effects for the current `(is_fullscreen, behavior::show_in_fullscreen)`
/// pair. Called both from the fullscreen detector callback (on every
/// transition) and from the `behavior_show_in_fullscreen_changed` event
/// listener (when the user toggles the setting while focused on a
/// fullscreen app). Suppression is the conjunction of "fullscreen
/// detected" and "user has not opted out" — when suppressed, the pill
/// hides, the global hotkey detaches, and an in-flight recording is
/// silently aborted (`do_cancel` drops the audio buffer + transitions
/// to IDLE without emitting a transcript, matching the spec's "don't
/// surprise-paste into a fullscreen game" rule).
fn apply_fullscreen_state(app: &tauri::AppHandle, is_fullscreen: bool) {
    let suppress = is_fullscreen && !behavior::show_in_fullscreen();
    hotkey::set_active(app, !suppress);
    let was_recording = suppress && dictation::is_recording();
    if was_recording {
        let _ = do_cancel();
    }
    let Some(pill) = app.get_webview_window("pill") else {
        return;
    };
    if !suppress {
        let _ = pill.show();
        return;
    }
    if !was_recording {
        let _ = pill.hide();
        return;
    }
    // Cancel ran but the pill webview is still rendering the recording
    // frame. NSWindow orderOut caches the last-painted frame, so an
    // immediate hide here means the next show — when the user exits
    // fullscreen — paints those orange bars for one frame before
    // React's IDLE tick lands. Defer the hide a couple of dictation
    // ticks so React renders IDLE *while still visible*; the cached
    // frame is then clean.
    let pill_clone = pill.clone();
    thread::Builder::new()
        .name("openwhisper-pill-deferred-hide".into())
        .spawn(move || {
            thread::sleep(Duration::from_millis(120));
            let _ = pill_clone.hide();
        })
        .expect("spawn pill deferred hide");
}

#[tauri::command]
async fn reposition_pill(
    app: tauri::AppHandle,
    monitor_origin: Option<(i32, i32)>,
) -> Result<(), String> {
    let app_clone = app.clone();
    app.run_on_main_thread(move || {
        if let Err(e) = place_pill(&app_clone, monitor_origin) {
            eprintln!("[reposition_pill] {e}");
        }
    })
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

    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

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
            // Same shape for the audio block: hydrate the in-memory cache
            // and propagate the saved device name into the core's selector
            // so the very first recording opens the user's preferred mic
            // (rather than whatever cpal's default is on this boot).
            let audio_settings = settings::load_audio_settings(app.handle());
            audio::audio_set_selected_device_id(audio_settings.device_id);
            // Convert the pill window to an NSPanel before any
            // collection-behavior call — plain NSWindow can't reliably
            // render on another app's fullscreen Space on Sonoma+
            // regardless of the bits we set, so the swizzle has to
            // happen first. Floating level + nonactivating style mask
            // matches the Cap / Screenpipe / Hyprnote pattern.
            #[cfg(target_os = "macos")]
            {
                use tauri_nspanel::{PanelLevel, StyleMask, WebviewWindowExt};
                if let Some(pill_win) = app.get_webview_window("pill") {
                    if let Ok(panel) = pill_win.to_panel::<PillPanel>() {
                        panel.set_level(PanelLevel::Floating.value());
                        panel.set_style_mask(
                            StyleMask::empty().nonactivating_panel().into(),
                        );
                    }
                }
            }

            // Hydrate the behavior AtomicBool cache before the fullscreen
            // detector thread starts so the very first transition reads
            // the persisted value rather than the default false. Apply
            // the pill's collection-behavior at the same time so users
            // who previously enabled the toggle get the expected
            // over-fullscreen rendering on relaunch without having to
            // flip the Switch again.
            let behavior_settings = settings::load_behavior_settings(app.handle());
            behavior::set_show_in_fullscreen_cache(behavior_settings.show_in_fullscreen);
            behavior::apply_collection_behavior(
                app.handle(),
                behavior_settings.show_in_fullscreen,
            );
            // TASK-48 — clear stale TCC entries on version change before
            // the AX prompt fires, so 0.3.0 → 0.4.0 (and future) upgraders
            // don't have to manually scrub System Settings to re-grant.
            permissions::reset_if_version_changed(app.handle());
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
            //
            // Override: when `behavior.show_in_fullscreen` is true, the
            // detector still observes transitions but the side-effects
            // are skipped — pill stays visible, hotkey stays armed.
            // Mid-recording fullscreen entry with the setting off aborts
            // the recording silently (see `apply_fullscreen_state`).
            let app_for_fullscreen = app.handle().clone();
            fullscreen::install_fullscreen(move |is_fullscreen| {
                apply_fullscreen_state(&app_for_fullscreen, is_fullscreen);
            });

            // Reconcile pill + hotkey state when the user toggles
            // `behavior.show_in_fullscreen` while a fullscreen app is
            // currently focused. The setter has already updated the
            // AtomicBool cache before emitting, so we just re-run the
            // same logic against the latest detector state — flipping
            // on while in fullscreen brings the pill back and re-arms
            // the hotkey without restarting OW; flipping off with a
            // recording in flight aborts it.
            let app_for_behavior_event = app.handle().clone();
            app.handle().listen("behavior_show_in_fullscreen_changed", move |event| {
                let enabled = serde_json::from_str::<bool>(event.payload())
                    .unwrap_or_else(|_| behavior::show_in_fullscreen());
                behavior::apply_collection_behavior(&app_for_behavior_event, enabled);
                apply_fullscreen_state(
                    &app_for_behavior_event,
                    fullscreen::is_active(),
                );
            });

            // Pill-follow: reposition the HUD onto the monitor hosting the
            // focused app whenever it changes. The watcher already gates
            // itself on settings::follow_active_screen() so we don't have
            // to. run_on_main_thread is load-bearing — Tauri's
            // available_monitors() may reach NSScreen.screens internally
            // on macOS, which is main-thread-only.
            let app_for_pill = app.handle().clone();
            fullscreen::install_pill_follow(move |origin| {
                let app = app_for_pill.clone();
                let app_inner = app.clone();
                let _ = app.run_on_main_thread(move || {
                    if let Err(e) = place_pill(&app_inner, origin) {
                        eprintln!("[pill-follow] {e}");
                    }
                });
            });

            // Dock / taskbar resize tracker. The cursor watcher fires
            // on screen-cross only — but the user can grow/shrink the
            // Mac Dock (or Win taskbar in auto-hide states) without
            // ever crossing screens, in which case the pill would
            // drift. Re-running place_pill on a 500 ms cadence picks
            // up work-area changes; place_pill self-dedupes via
            // LAST_PILL_POSITION so this is free when nothing moved.
            let app_for_refresh = app.handle().clone();
            thread::Builder::new()
                .name("openwhisper-pill-refresh".into())
                .spawn(move || loop {
                    thread::sleep(Duration::from_millis(500));
                    if !settings::follow_active_screen() {
                        continue;
                    }
                    let app = app_for_refresh.clone();
                    let app_inner = app.clone();
                    let origin = fullscreen::last_pill_monitor();
                    let _ = app.run_on_main_thread(move || {
                        if let Err(e) = place_pill(&app_inner, origin) {
                            eprintln!("[pill-refresh] {e}");
                        }
                    });
                })
                .expect("spawn pill refresh");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_version,
            dictation_toggle,
            dictation_cancel,
            set_pill_click_through,
            reposition_pill,
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
            settings::audio_set_device,
            settings::settings_get_pill,
            settings::settings_set_pill_follow,
            audio_get_device_state,
            audio_preview_start,
            audio_preview_stop,
            behavior::behavior_get_show_in_fullscreen,
            behavior::behavior_set_show_in_fullscreen,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
