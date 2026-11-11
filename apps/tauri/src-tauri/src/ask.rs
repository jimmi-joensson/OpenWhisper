//! Ask in the shell (TASK-97.4): the commands, the `ask_tick` emitter
//! and the answer window. Thin wrappers: core owns the settings
//! block, the one ask of the running app and every decision about it
//! (`openwhisper_core::ask`, the ask entry points of
//! `openwhisper_core::dictation`). This module resolves the settings
//! path, moves slow calls off the UI thread, tells the windows what
//! changed and shows or hides the answer window.
//!
//! The same ask is reachable headless through `openwhisper ask …`.
//!
//! # The answer window
//!
//! A third window, label [`ANSWER_LABEL`], declared hidden in both
//! `tauri.conf.json` and `tauri.dev.conf.json` (the dev file replaces
//! the window list). It is shown while there is an ask on core's
//! snapshot (any phase but `idle`) and hidden otherwise, and it is
//! hidden together with the pill while a fullscreen app suppresses
//! the pill ([`set_suppressed`]). [`window_should_show`] is the whole
//! rule.
//!
//! It must never take focus: "insert" pastes into the app the person
//! was in. On macOS it is a non-activating floating NSPanel that
//! cannot become key (converted in `lib.rs::setup`, like the pill)
//! and is shown with `orderFrontRegardless`. On Windows it is created
//! with `focusable: false` (`WS_EX_NOACTIVATE`) and shown with
//! `SW_SHOWNOACTIVATE` through the raw handle: tao's own `show` uses
//! `SW_SHOW` on every show after the first, which activates.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{mpsc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use openwhisper_core::ask::{self, AskOutline, AskPhase, AskSnapshot};
use openwhisper_core::dictation;
use openwhisper_core::refine::BackendStatus;
use openwhisper_core::settings::{
    self as core_settings, AskSettings, ASK_INSTRUCTIONS_MAX_CHARS, ASK_PLACE_MAX_CHARS, ASK_TIMEOUT_DEFAULT_MS,
    ASK_TIMEOUT_MAX_MS, ASK_TIMEOUT_MIN_MS, DEFAULT_CLAUDE_MODEL,
};
use tauri::{AppHandle, Emitter, Manager};

use crate::settings::settings_path;
use crate::{PillSpot, TauriDictationEnv};

/// Label of the answer window.
pub(crate) const ANSWER_LABEL: &str = "answer";

/// Emitted to every window when the ask changes. Payload: core's
/// [`AskSnapshot`], whole.
pub(crate) const ASK_TICK_EVENT: &str = "ask_tick";

/// Emitted to every window after the Ask settings changed. Payload:
/// the stored [`AskSettings`].
pub(crate) const ASK_CHANGED_EVENT: &str = "ask_changed";

/// Size of the answer window in logical points. Must match both
/// config files (a test below reads them).
pub(crate) const ANSWER_WIN_W: f64 = 420.0;
pub(crate) const ANSWER_WIN_H: f64 = 360.0;

/// Logical points between the bottom edge of the answer window and
/// the top of the idle pill capsule. The capsule grows to 1.5× while
/// recording (5.5 points upwards), so this leaves it clear.
const ABOVE_PILL_GAP: f64 = 12.0;

/// How often `ask_tick` is sent while an ask is in flight and nothing
/// else changed, so `elapsed_ms` moves in the panel.
const BUSY_TICK: Duration = Duration::from_millis(250);

// --- settings ------------------------------------------------------

/// What every reader of the Ask settings loads: core's
/// [`AskOutline`] (`settings`, `model`, `effort`, `web_search`,
/// `timeout_ms`, `notice`) side by side with what the controls need
/// to know. Read off the settings alone: the state of the backend is
/// not in it, see [`ask_status`].
#[derive(serde::Serialize)]
pub struct AskState {
    #[serde(flatten)]
    pub outline: AskOutline,
    pub limits: AskLimits,
    /// The Claude model an ask uses when the block names none.
    pub default_model: &'static str,
}

/// The ranges core clamps the Ask block to.
#[derive(serde::Serialize)]
pub struct AskLimits {
    pub timeout_min_ms: u64,
    pub timeout_max_ms: u64,
    pub timeout_default_ms: u64,
    pub instructions_max_chars: usize,
    pub place_max_chars: usize,
}

fn state(outline: AskOutline) -> AskState {
    AskState {
        outline,
        limits: AskLimits {
            timeout_min_ms: ASK_TIMEOUT_MIN_MS,
            timeout_max_ms: ASK_TIMEOUT_MAX_MS,
            timeout_default_ms: ASK_TIMEOUT_DEFAULT_MS,
            instructions_max_chars: ASK_INSTRUCTIONS_MAX_CHARS,
            place_max_chars: ASK_PLACE_MAX_CHARS,
        },
        default_model: DEFAULT_CLAUDE_MODEL,
    }
}

/// Held by everything in this process that puts an Ask block into
/// core's cache: a save and a fresh read. Without it a read that
/// started before a save could finish after it and leave the cache
/// (which decides whether an ask may start) holding the block from
/// before the save. Same reason as the lock in `modes.rs`.
fn ask_lock() -> MutexGuard<'static, ()> {
    static ASK: Mutex<()> = Mutex::new(());
    ASK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Boot-time: hydrate core's Ask cache, so the hotkey install that
/// follows knows whether the ask slot is wanted.
pub fn init(app: &AppHandle) {
    if let Ok(path) = settings_path(app) {
        let _held = ask_lock();
        let _ = core_settings::load_ask_settings(&path);
    }
    // The facts an ask tells the model about this computer: read them
    // once now, on a thread of core's own, so the first ask does not
    // pay for the reads that are slow the first time.
    ask::context::prewarm();
}

/// The Ask block changed from `before` to `now`: re-install the
/// hotkeys when the switch moved (the ask slot is only installed
/// while Ask is on) and tell the windows.
///
/// Not on every save: the pane saves while the person types their
/// instructions, and the keyboard hook is not something to churn
/// (see the Windows hook notes in `hotkey/windows.rs`).
fn announce(app: &AppHandle, before: &AskSettings, now: &AskSettings) {
    if before.enabled != now.enabled {
        crate::hotkey::install(app);
    }
    if let Err(e) = app.emit(ASK_CHANGED_EVENT, now) {
        eprintln!("[ask] {ASK_CHANGED_EVENT} emit failed: {e}");
    }
}

/// Fresh read from disk (the block is also edited by the headless
/// CLI), with what an ask would use right now. Cheap: nothing is
/// asked of the backend, so every window that only wants to know
/// whether Ask is on can call it. Off the UI thread for the file
/// read.
#[tauri::command]
pub async fn ask_get(app: AppHandle) -> Result<AskState, String> {
    let path = settings_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (before, settings) = {
            let _held = ask_lock();
            (core_settings::current_ask_settings(), core_settings::load_ask_settings(&path))
        };
        // The file was changed behind this process's back.
        if before != settings {
            announce(&app, &before, &settings);
        }
        state(ask::outline(&settings, &core_settings::current_refine_settings()))
    })
    .await
    .map_err(|e| format!("ask settings: {e}"))
}

/// Whether the backend an ask uses can run, and where questions are
/// sent (`destination`). Only the Ask pane asks: the check probes
/// the Claude CLI (about 0.2 s, a process start), so it runs on a
/// blocking thread and is not part of [`ask_get`].
#[tauri::command]
pub async fn ask_status() -> Result<BackendStatus, String> {
    tauri::async_runtime::spawn_blocking(|| {
        openwhisper_core::refine::backend_status(&core_settings::current_refine_settings())
    })
    .await
    .map_err(|e| format!("ask status: {e}"))
}

/// The facts about this computer an ask would send along if it
/// started now, as the exact text the model would get
/// (`ask::context_preview`, the text `openwhisper ask context`
/// prints). For the preview in the Ask pane; nothing is sent
/// anywhere. Off the UI thread: the facts are local reads, and the
/// first one of a session takes a moment.
#[tauri::command]
pub async fn ask_context_preview() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| ask::context_preview(&core_settings::current_ask_settings()))
        .await
        .map_err(|e| format!("ask context: {e}"))
}

/// Save the whole block; returns what core stored (trimmed, clamped).
/// Emits `ask_changed`, and re-installs the hotkeys when Ask was
/// switched on or off.
#[tauri::command]
pub fn ask_set(app: AppHandle, settings: AskSettings) -> Result<AskSettings, String> {
    let path = settings_path(&app)?;
    let (before, saved) = {
        let _held = ask_lock();
        let before = core_settings::current_ask_settings();
        (before, core_settings::save_ask_settings(&path, settings)?)
    };
    announce(&app, &before, &saved);
    Ok(saved)
}

// --- the ask -------------------------------------------------------

/// Ask a typed question (`ask::submit`), on a blocking thread.
/// Resolves when the ask is over, with the snapshot it ended on
/// (`done`, `error`, `cancelled`, or `idle` for an empty question or
/// a dismissed ask); the phases in between arrive as `ask_tick`.
/// Rejects at once, in core's words, while Ask is switched off.
#[tauri::command]
pub async fn ask_start(text: String) -> Result<AskSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || ask::submit(&text).map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("ask: {e}"))?
}

/// What the ask hotkey does: start an ask recording, or stop the
/// recording that is running. Nothing while Ask is switched off.
#[tauri::command]
pub fn ask_toggle() -> Result<(), String> {
    crate::do_ask_toggle()
}

/// Stop an ask, whatever it is doing; a finished answer stays.
/// `id` is the ask the caller is showing: when another ask has taken
/// its place by now, nothing happens. Without an id: whatever ask is
/// there. Returns whether there was something to stop. Off the UI
/// thread: stopping kills and reaps the backend process.
#[tauri::command(async)]
pub fn ask_cancel(id: Option<u64>) -> bool {
    dictation::run_cancel_ask(&TauriDictationEnv, id)
}

/// Close an ask: stop it when it is still running, drop it from the
/// snapshot and hide the answer window. `id` as for [`ask_cancel`]:
/// a close meant for an earlier ask leaves the current one alone.
#[tauri::command(async)]
pub fn ask_dismiss(app: AppHandle, id: Option<u64>) {
    dictation::run_dismiss_ask(&TauriDictationEnv, id);
    sync_window(&app);
}

/// Paste the finished answer into the app the person was in, through
/// the injector dictation uses. Only when the answer is complete;
/// returns whether there was one to hand over.
///
/// The ask is closed and the answer window is gone before the paste
/// is sent, so the keystroke meets the other app with nothing of
/// ours on screen above it. With an `id`, only the answer of that
/// ask is inserted.
#[tauri::command(async)]
pub fn ask_insert(app: AppHandle, id: Option<u64>) -> bool {
    let Some((answered, answer)) = ask::finished() else {
        return false;
    };
    if id.is_some_and(|id| id != answered) {
        return false;
    }
    dictation::run_dismiss_ask(&TauriDictationEnv, Some(answered));
    sync_window_now(&app);
    crate::injection::inject_text(&app, &answer);
    true
}

/// Put the answer on the clipboard. What that is, core decides
/// ([`ask::text_to_copy`]): the whole answer once it is complete,
/// what has arrived of it before that. Returns whether anything was
/// copied.
#[tauri::command(async)]
pub fn ask_copy() -> bool {
    let Some(text) = ask::text_to_copy() else {
        return false;
    };
    match crate::injection::copy_text(&text) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("[ask] copy failed: {e}");
            false
        }
    }
}

/// The current ask, for a window that opens after the last tick.
#[tauri::command]
pub fn ask_snapshot() -> AskSnapshot {
    ask::snapshot()
}

// --- the tick ------------------------------------------------------

/// Whether `ask_tick` is due. `sent` is the version last emitted and
/// how long ago: on every change of the snapshot, and at
/// [`BUSY_TICK`] while an ask is in flight, so the elapsed time in
/// the panel moves (core reads it off the clock without a new
/// version).
fn tick_due(sent: Option<(u64, Duration)>, snap: &AskSnapshot) -> bool {
    match sent {
        None => true,
        Some((version, ago)) => {
            version != snap.version || (snap.phase.is_busy() && ago >= BUSY_TICK)
        }
    }
}

/// Start the thread that emits `ask_tick` and keeps the answer window
/// in step with the ask. Polls at the dictation tick rate.
pub fn spawn_emitter(app: AppHandle) {
    thread::Builder::new()
        .name("openwhisper-ask-emitter".into())
        .spawn(move || {
            let mut sent: Option<(u64, Instant)> = None;
            loop {
                thread::sleep(Duration::from_millis(crate::TICK_MS));
                let snap = ask::snapshot();
                if !window_in_step(snap.phase) {
                    sync_window(&app);
                }
                if tick_due(sent.map(|(v, at)| (v, at.elapsed())), &snap) {
                    sent = Some((snap.version, Instant::now()));
                    if app.emit(ASK_TICK_EVENT, snap).is_err() {
                        break;
                    }
                }
            }
        })
        .expect("spawn ask emitter");
}

// --- the answer window ---------------------------------------------

/// The pill is hidden for a fullscreen app, and the answer window
/// with it.
static SUPPRESSED: AtomicBool = AtomicBool::new(false);

/// What the answer window is right now: 1 hidden, 2 shown. It starts
/// hidden (`visible: false` in the config). Written on the main
/// thread only, in the same step that shows or hides the window, so
/// it cannot say one thing while the window is the other.
static APPLIED: AtomicU8 = AtomicU8::new(HIDDEN);

const HIDDEN: u8 = 1;
const SHOWN: u8 = 2;

fn suppressed() -> bool {
    SUPPRESSED.load(Ordering::Relaxed)
}

/// The window is already what `phase` asks for. False for as long as
/// a show or hide is still to be applied (or could not be queued), so
/// the emitter tries again.
fn window_in_step(phase: AskPhase) -> bool {
    window_change(APPLIED.load(Ordering::Relaxed), window_should_show(phase, suppressed())).is_none()
}

/// What to do with the window, given what it is (`applied`) and
/// whether it should be on screen now: `Some(true)` show it,
/// `Some(false)` hide it, `None` leave it.
///
/// The step that runs on the main thread asks this with the state of
/// that moment, not with what was wanted when the step was queued. A
/// show that was queued before a newer hide therefore finds nothing
/// to show any more when its turn comes.
fn window_change(applied: u8, show: bool) -> Option<bool> {
    let want = if show { SHOWN } else { HIDDEN };
    (applied != want).then_some(show)
}

/// The answer window is on screen while there is an ask to show (any
/// phase but `idle`: listening, thinking, answering, and the answer,
/// error or cancel note that stays until it is dismissed), unless
/// the pill is hidden for a fullscreen app.
pub(crate) fn window_should_show(phase: AskPhase, suppressed: bool) -> bool {
    phase != AskPhase::Idle && !suppressed
}

/// Called by the fullscreen path with whether the pill is hidden.
/// A running ask is not stopped by it: its answer is there when the
/// window comes back.
pub(crate) fn set_suppressed(app: &AppHandle, suppressed: bool) {
    SUPPRESSED.store(suppressed, Ordering::Relaxed);
    sync_window(app);
}

/// Show or hide the answer window to match the ask as it is now.
/// Does not wait for the window: the change is queued on the main
/// thread.
pub(crate) fn sync_window(app: &AppHandle) {
    let _ = queue_window_sync(app);
}

/// [`sync_window`], returning once the window has been shown or
/// hidden (or after a short wait, should the main thread be busy).
fn sync_window_now(app: &AppHandle) {
    if let Some(applied) = queue_window_sync(app) {
        let _ = applied.recv_timeout(Duration::from_millis(500));
    }
}

/// Queue a step on the main thread that brings the window in line
/// with the ask. Returns a receiver that hears when the step has
/// run, `None` when the window is in step already or the step could
/// not be queued.
///
/// What the step does is decided when it runs, from the ask and the
/// window as they are then ([`window_change`]): the emitter thread
/// and the commands all come here, their steps can run in any order
/// and after any delay, and each of them leaves the window right for
/// the moment it ran in.
fn queue_window_sync(app: &AppHandle) -> Option<mpsc::Receiver<()>> {
    if window_in_step(ask::snapshot().phase) {
        return None;
    }
    let handle = app.clone();
    let (done, applied) = mpsc::channel::<()>();
    // Window geometry and the panel calls are main-thread-only on
    // macOS. Called from the main thread, this runs in place.
    let queued = app.run_on_main_thread(move || {
        let show = window_should_show(ask::snapshot().phase, suppressed());
        if let Some(show) = window_change(APPLIED.load(Ordering::Relaxed), show) {
            if show {
                // On the pill's monitor, above the pill, before it is
                // seen.
                match crate::pill_spot(&handle, crate::fullscreen::last_pill_monitor()) {
                    Ok(spot) => place_window(&handle, &spot),
                    Err(e) => eprintln!("[ask] answer window not placed: {e}"),
                }
            }
            set_window_visible(&handle, show);
            APPLIED.store(if show { SHOWN } else { HIDDEN }, Ordering::Relaxed);
        }
        let _ = done.send(());
    });
    if let Err(e) = queued {
        eprintln!("[ask] answer window not brought in step: {e}");
        return None;
    }
    Some(applied)
}

/// Show or hide without activating anything. Main thread only.
fn set_window_visible(app: &AppHandle, show: bool) {
    #[cfg(target_os = "macos")]
    {
        use tauri_nspanel::ManagerExt;
        // `show` is orderFrontRegardless and `hide` is orderOut:
        // neither activates the app nor makes the panel key.
        if let Ok(panel) = app.get_webview_panel(ANSWER_LABEL) {
            if show {
                panel.show();
            } else {
                panel.hide();
            }
            return;
        }
        // The conversion in setup failed (it logged why). A plain
        // window still shows the answer; it cannot go over another
        // app's fullscreen Space.
        if let Some(window) = app.get_webview_window(ANSWER_LABEL) {
            let _ = if show { window.show() } else { window.hide() };
        }
    }
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE, SW_SHOWNOACTIVATE};
        let Some(window) = app.get_webview_window(ANSWER_LABEL) else {
            return;
        };
        match window.hwnd() {
            // Both directions through the raw handle, so tao's own
            // idea of "visible" is never half right: it skips a hide
            // for a window it did not show itself.
            Ok(hwnd) => unsafe {
                let _ = ShowWindow(hwnd, if show { SW_SHOWNOACTIVATE } else { SW_HIDE });
            },
            Err(e) => eprintln!("[ask] answer window handle: {e}"),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Some(window) = app.get_webview_window(ANSWER_LABEL) {
            let _ = if show { window.show() } else { window.hide() };
        }
    }
}

/// Top-left of the answer window, in the logical points of
/// [`PillSpot`]: centered on the pill's monitor, its bottom edge
/// [`ABOVE_PILL_GAP`] above the pill capsule, and never above the top
/// of that monitor.
fn answer_origin(spot: &PillSpot) -> (f64, f64) {
    let x = spot.mon_x + (spot.mon_w - ANSWER_WIN_W) / 2.0;
    let y = (spot.capsule_top - ABOVE_PILL_GAP - ANSWER_WIN_H).max(spot.mon_y);
    (x, y)
}

/// Last position set on the answer window, to skip the native call
/// when the pill's periodic refresh finds nothing moved.
static LAST_POSITION: Mutex<Option<(f64, f64)>> = Mutex::new(None);

/// Put the answer window above the pill at `spot`. Called whenever
/// the pill is placed and whenever the window is about to be shown.
/// Main thread only, like `place_pill`.
pub(crate) fn place_window(app: &AppHandle, spot: &PillSpot) {
    let Some(window) = app.get_webview_window(ANSWER_LABEL) else {
        return;
    };
    let (x, y) = answer_origin(spot);
    {
        let mut last = LAST_POSITION.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((lx, ly)) = *last {
            if (lx - x).abs() < 0.5 && (ly - y).abs() < 0.5 {
                return;
            }
        }
        *last = Some((x, y));
    }
    if let Err(e) = crate::set_window_at_logical(&window, x, y, spot.scale) {
        eprintln!("[ask] answer window position: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_PHASES: [AskPhase; 7] = [
        AskPhase::Idle,
        AskPhase::Listening,
        AskPhase::Thinking,
        AskPhase::Answering,
        AskPhase::Done,
        AskPhase::Error,
        AskPhase::Cancelled,
    ];

    #[test]
    fn the_window_shows_for_every_phase_but_idle() {
        let shown: Vec<&str> = ALL_PHASES
            .into_iter()
            .filter(|p| window_should_show(*p, false))
            .map(AskPhase::as_str)
            .collect();
        assert_eq!(shown, ["listening", "thinking", "answering", "done", "error", "cancelled"]);
    }

    #[test]
    fn the_window_is_hidden_with_the_pill_whatever_the_phase() {
        for phase in ALL_PHASES {
            assert!(!window_should_show(phase, true), "{phase:?}");
        }
    }

    fn snap(phase: AskPhase, version: u64) -> AskSnapshot {
        AskSnapshot { phase, version, ..AskSnapshot::default() }
    }

    #[test]
    fn a_window_step_acts_on_the_state_of_the_moment_it_runs_in() {
        // In step: nothing to do, either way.
        assert_eq!(window_change(HIDDEN, false), None);
        assert_eq!(window_change(SHOWN, true), None);
        // Out of step: show, or hide.
        assert_eq!(window_change(HIDDEN, true), Some(true));
        assert_eq!(window_change(SHOWN, false), Some(false));

        // Steps run in the order they were queued, each with what is
        // wanted when it runs. A show is queued (an ask starts), then
        // a hide (it is dismissed) before either has run.
        let run = |applied: &mut u8, show_now: bool| {
            if let Some(show) = window_change(*applied, show_now) {
                *applied = if show { SHOWN } else { HIDDEN };
            }
        };
        let mut applied = HIDDEN;
        // Both steps run after the dismiss: neither shows anything.
        run(&mut applied, false);
        run(&mut applied, false);
        assert_eq!(applied, HIDDEN);
        // The show ran before the dismiss, the hide after: hidden.
        run(&mut applied, true);
        assert_eq!(applied, SHOWN);
        run(&mut applied, false);
        assert_eq!(applied, HIDDEN);
        // A step queued for a hide that runs after a new ask began
        // shows the window instead of hiding it.
        run(&mut applied, true);
        assert_eq!(applied, SHOWN);
    }

    #[test]
    fn the_window_counts_as_out_of_step_until_the_step_has_run() {
        // The applied state is only written by the step itself, so
        // while nothing has run the emitter keeps asking for one.
        assert_eq!(APPLIED.load(Ordering::Relaxed), HIDDEN);
        assert!(window_in_step(AskPhase::Idle));
        for phase in [AskPhase::Listening, AskPhase::Answering, AskPhase::Done] {
            assert!(!window_in_step(phase), "{phase:?}");
        }
    }

    #[test]
    fn a_tick_is_sent_first_and_on_every_change() {
        let ms = Duration::from_millis;
        // The first tick, so a window that just opened has the state.
        assert!(tick_due(None, &snap(AskPhase::Idle, 0)));
        // Nothing changed, nothing running: silence, however long.
        assert!(!tick_due(Some((0, ms(60_000))), &snap(AskPhase::Idle, 0)));
        assert!(!tick_due(Some((7, ms(60_000))), &snap(AskPhase::Done, 7)));
        // A change goes out at once, busy or not.
        assert!(tick_due(Some((7, ms(0))), &snap(AskPhase::Done, 8)));
        assert!(tick_due(Some((7, ms(0))), &snap(AskPhase::Idle, 8)));
    }

    #[test]
    fn a_running_ask_ticks_about_four_times_a_second() {
        let ms = Duration::from_millis;
        for phase in [AskPhase::Listening, AskPhase::Thinking, AskPhase::Answering] {
            assert!(!tick_due(Some((3, ms(200))), &snap(phase, 3)), "{phase:?}");
            assert!(tick_due(Some((3, ms(250))), &snap(phase, 3)), "{phase:?}");
        }
        // Over: the elapsed time stands still, so does the tick.
        for phase in [AskPhase::Done, AskPhase::Error, AskPhase::Cancelled, AskPhase::Idle] {
            assert!(!tick_due(Some((3, ms(5_000))), &snap(phase, 3)), "{phase:?}");
        }
    }

    fn spot(mon_x: f64, mon_y: f64, mon_w: f64, capsule_top: f64) -> PillSpot {
        PillSpot { x: 0.0, y: 0.0, scale: 2.0, mon_x, mon_y, mon_w, capsule_top }
    }

    #[test]
    fn the_window_is_centered_above_the_pill_on_the_pills_monitor() {
        // A 1440 pt wide primary display, pill capsule top at 830.
        let (x, y) = answer_origin(&spot(0.0, 0.0, 1440.0, 830.0));
        assert_eq!(x, (1440.0 - ANSWER_WIN_W) / 2.0);
        assert_eq!(y + ANSWER_WIN_H, 830.0 - ABOVE_PILL_GAP);

        // A second display to the left of and above the primary one.
        let (x, y) = answer_origin(&spot(-1920.0, -200.0, 1920.0, 800.0));
        assert_eq!(x, -1920.0 + (1920.0 - ANSWER_WIN_W) / 2.0);
        assert_eq!(y, 800.0 - ABOVE_PILL_GAP - ANSWER_WIN_H);
    }

    #[test]
    fn the_window_stays_on_a_monitor_too_short_for_it() {
        let (_, y) = answer_origin(&spot(0.0, 100.0, 800.0, 300.0));
        assert_eq!(y, 100.0);
    }

    #[test]
    fn the_tick_payload_is_cores_snapshot_with_the_phase_as_a_word() {
        let value = serde_json::to_value(AskSnapshot {
            id: 3,
            phase: AskPhase::Answering,
            question: "What is a tap?".into(),
            answer: "A tap is".into(),
            version: 12,
            ..AskSnapshot::default()
        })
        .unwrap();
        let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        // The fields `AskSnapshot` in src/lib/ask.ts mirrors.
        assert_eq!(
            keys,
            [
                "answer",
                "backend",
                "effort",
                "elapsed_ms",
                "error",
                "first_text_ms",
                "id",
                "model",
                "notice",
                "phase",
                "question",
                "search_query",
                "searching",
                "started_at_ms",
                "suggestion",
                "usage",
                "version",
                "web_search",
            ]
        );
        assert_eq!(value["phase"], "answering");
        assert_eq!(value["answer"], "A tap is");
        assert_eq!(value["first_text_ms"], serde_json::Value::Null);
        // Nothing to suggest is null; a suggestion is a word.
        assert_eq!(value["suggestion"], serde_json::Value::Null);
        let marked = AskSnapshot { suggestion: Some(ask::AskSuggestion::WebSearch), ..AskSnapshot::default() };
        assert_eq!(serde_json::to_value(marked).unwrap()["suggestion"], "web-search");
        assert_eq!(serde_json::to_value(AskPhase::Idle).unwrap(), "idle");
    }

    #[test]
    fn the_settings_payload_has_no_backend_state_in_it() {
        // What `ask_get` answers with: read off the two settings
        // blocks alone, so nothing in it can have cost a probe.
        let refine = openwhisper_core::settings::RefineSettings::default();
        let value = serde_json::to_value(state(ask::outline(&AskSettings::default(), &refine))).unwrap();
        let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        // The fields `AskState` in src/lib/ask.ts mirrors.
        assert_eq!(
            keys,
            [
                "backend_kind",
                "default_model",
                "destination",
                "effort",
                "limits",
                "model",
                "notice",
                "settings",
                "timeout_ms",
                "web_search",
            ]
        );
        assert_eq!(value["backend_kind"], "claude-cli");
        assert_eq!(value["settings"]["enabled"], false);
        assert_eq!(value["limits"]["timeout_default_ms"], 60_000);
    }

    #[test]
    fn the_changed_payload_is_the_stored_block() {
        let value = serde_json::to_value(AskSettings { enabled: true, ..AskSettings::default() }).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "enabled": true,
                "web_search": false,
                "instructions": "",
                "model": null,
                "effort": null,
                "timeout_ms": 60_000,
                "context": {
                    "date_time": true,
                    "computer": true,
                    "front_app": true,
                    "locale": true,
                    "place": "",
                },
            })
        );
    }

    #[test]
    fn the_settings_payload_carries_the_context_switches_and_the_place_limit() {
        let refine = openwhisper_core::settings::RefineSettings::default();
        let ask = AskSettings {
            context: openwhisper_core::settings::ContextSettings {
                computer: false,
                place: "Aarhus, Denmark".into(),
                ..Default::default()
            },
            ..AskSettings::default()
        };
        let value = serde_json::to_value(state(ask::outline(&ask, &refine))).unwrap();
        assert_eq!(value["settings"]["context"]["computer"], false);
        assert_eq!(value["settings"]["context"]["date_time"], true);
        assert_eq!(value["settings"]["context"]["place"], "Aarhus, Denmark");
        assert_eq!(value["limits"]["place_max_chars"], 120);
        // The text of the preview is core's, read off the same block.
        assert!(ask::context_preview(&ask).starts_with("Where the person is: Aarhus, Denmark\nDate: "));
    }

    /// Every window of a config file, by label.
    fn windows(config: &str) -> Vec<serde_json::Value> {
        let config: serde_json::Value = serde_json::from_str(config).unwrap();
        config["app"]["windows"].as_array().unwrap().clone()
    }

    #[test]
    fn the_answer_window_is_declared_the_same_in_both_config_files() {
        // The dev file's window list replaces the main one, so a
        // window missing there does not exist in a dev build.
        let main = windows(include_str!("../tauri.conf.json"));
        let dev = windows(include_str!("../tauri.dev.conf.json"));
        let find = |list: &[serde_json::Value]| {
            list.iter().find(|w| w["label"] == ANSWER_LABEL).cloned().expect("answer window")
        };
        let answer = find(&main);
        assert_eq!(answer, find(&dev));
        for list in [&main, &dev] {
            let labels: Vec<&str> = list.iter().map(|w| w["label"].as_str().unwrap()).collect();
            assert_eq!(labels, ["main", "pill", ANSWER_LABEL]);
        }

        assert_eq!(answer["width"].as_f64(), Some(ANSWER_WIN_W));
        assert_eq!(answer["height"].as_f64(), Some(ANSWER_WIN_H));
        // Hidden until an ask starts, and never a focus target.
        for off in ["visible", "focus", "focusable", "decorations", "resizable", "shadow"] {
            assert_eq!(answer[off], false, "{off}");
        }
        for on in ["transparent", "alwaysOnTop", "skipTaskbar", "acceptFirstMouse"] {
            assert_eq!(answer[on], true, "{on}");
        }
    }

    #[test]
    fn the_answer_window_may_use_the_commands() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let labels: Vec<&str> =
            capability["windows"].as_array().unwrap().iter().map(|w| w.as_str().unwrap()).collect();
        assert!(labels.contains(&ANSWER_LABEL), "{labels:?}");
    }
}
