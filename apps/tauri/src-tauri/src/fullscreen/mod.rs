//! Fullscreen + pill-follow detection — fires callbacks when the
//! foreground app's fullscreen state or its hosting monitor changes.
//!
//! Two consumers, one poll thread:
//! - Fullscreen callback (`install_fullscreen`) — registered by `lib.rs`
//!   to drop the global hotkey and hide the pill while the user is in
//!   a fullscreen app.
//! - Pill-follow callback (`install_pill_follow`) — registered by
//!   `lib.rs` to reposition the pill HUD onto the monitor hosting the
//!   focused app. Gated by `settings::follow_active_screen()` so the
//!   user's opt-out toggle is honored without restart.
//!
//! Implementation is poll-based (500 ms tick on a dedicated thread)
//! rather than NSWorkspace / SetWinEventHook observer-driven. Reasons:
//!
//! - **Mac**: `NSWorkspaceActiveSpaceDidChangeNotification` requires a
//!   block-based observer (block2 + objc2-app-kit deps + observer
//!   lifetime tracking). The detection itself goes through the
//!   Accessibility framework (any-thread-safe), so the only thing the
//!   observer would buy is "react instantly instead of within 500 ms" —
//!   not user-visible for activation gating.
//! - **Windows**: `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` does need a
//!   dedicated thread + msg pump regardless. C5 wires it; until then,
//!   poll covers Win as well.
//!
//! If profiling later shows poll cost, swap to observers without
//! changing the public API of this module.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "windows")]
mod windows;

const POLL_INTERVAL: Duration = Duration::from_millis(500);

static ACTIVE: AtomicBool = AtomicBool::new(false);

type Callback = Box<dyn Fn(bool) + Send + Sync + 'static>;
static CALLBACK: OnceLock<Callback> = OnceLock::new();
static INSTALLED: AtomicBool = AtomicBool::new(false);

type MonitorCallback = Box<dyn Fn(Option<(i32, i32)>) + Send + Sync + 'static>;
static MONITOR_CB: OnceLock<MonitorCallback> = OnceLock::new();
/// Last-seen monitor origin so the callback only fires on actual
/// changes. Deliberately NOT reset when `follow_active_screen` flips
/// off — flipping back on should only trigger when the user genuinely
/// changes monitor afterwards, not replay the current one.
static LAST_MONITOR: Mutex<Option<(i32, i32)>> = Mutex::new(None);

/// Last-detected fullscreen state. The poll thread updates this on every
/// tick whether or not the value changed. Consumers that need to
/// re-evaluate gating outside of a transition (e.g. when the user toggles
/// `behavior.show_in_fullscreen` while a fullscreen app is currently
/// focused) read this without restarting the poller.
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Register the fullscreen-state change callback and start the poll
/// thread. First caller wins; subsequent calls re-register the
/// callback but no-op the spawn.
pub fn install_fullscreen<F>(on_change: F)
where
    F: Fn(bool) + Send + Sync + 'static,
{
    let _ = CALLBACK.set(Box::new(on_change));
    ensure_poller_started();
}

/// Temporary alias — keeps `lib.rs`'s existing `fullscreen::install`
/// call site compiling until TASK-55.5 swaps it for the new
/// two-callback wiring.
pub use install_fullscreen as install;

/// Register the pill-follow callback. Fires `cb(Some(origin))` only
/// when the focused window's hosting monitor's origin tuple changes
/// AND the user has not opted out via `settings::follow_active_screen`.
/// First-call-wins for the spawn; subsequent calls re-register.
#[allow(dead_code)] // wired from lib.rs in TASK-55.5
pub fn install_pill_follow<F>(on_change: F)
where
    F: Fn(Option<(i32, i32)>) + Send + Sync + 'static,
{
    let _ = MONITOR_CB.set(Box::new(on_change));
    ensure_poller_started();
}

fn ensure_poller_started() {
    if INSTALLED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        spawn_poller();
    }
}

fn spawn_poller() {
    thread::Builder::new()
        .name("openwhisper-fullscreen-poll".into())
        .spawn(|| loop {
            thread::sleep(POLL_INTERVAL);

            let cur = detect_now();
            let prev = ACTIVE.swap(cur, Ordering::Relaxed);
            if prev != cur {
                if let Some(cb) = CALLBACK.get() {
                    cb(cur);
                }
            }

            // Pill-follow signal: gated on the user setting so a flip
            // to OFF stops firing immediately, no restart needed. We
            // do NOT touch LAST_MONITOR while gated off — flipping
            // back on therefore only fires when the user genuinely
            // changes monitor afterwards.
            if crate::settings::follow_active_screen() {
                if let Some(origin) = focused_window_monitor() {
                    let mut last = LAST_MONITOR.lock().unwrap();
                    if *last != Some(origin) {
                        *last = Some(origin);
                        if let Some(cb) = MONITOR_CB.get() {
                            cb(Some(origin));
                        }
                    }
                }
            }
        })
        .expect("spawn fullscreen poller");
}

#[cfg(target_os = "macos")]
fn detect_now() -> bool {
    mac::is_fullscreen_now()
}

#[cfg(target_os = "windows")]
fn detect_now() -> bool {
    windows::is_fullscreen_now()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn detect_now() -> bool {
    false
}

#[cfg(target_os = "macos")]
fn focused_window_monitor() -> Option<(i32, i32)> {
    mac::focused_window_monitor()
}

#[cfg(target_os = "windows")]
fn focused_window_monitor() -> Option<(i32, i32)> {
    windows::focused_window_monitor()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn focused_window_monitor() -> Option<(i32, i32)> {
    None
}
