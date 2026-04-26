//! Fullscreen detection — fires a callback when the foreground app
//! enters / exits fullscreen.
//!
//! C6 wires consumers (hotkey teardown, pill hide). This module just
//! delivers the API: register a callback once via [`install`], read
//! current state via [`is_active`].
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
use std::sync::OnceLock;
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

/// Latest known fullscreen state. `false` until the first poll completes
/// (~500 ms after [`install`]).
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Register the change callback and start the poll thread. First call
/// wins; subsequent calls are no-ops.
pub fn install<F>(on_change: F)
where
    F: Fn(bool) + Send + Sync + 'static,
{
    let _ = CALLBACK.set(Box::new(on_change));
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
