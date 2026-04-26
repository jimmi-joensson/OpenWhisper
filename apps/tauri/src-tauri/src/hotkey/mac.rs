//! macOS hotkey via CGEventTap. Port of `archive/macos/App/HotkeyService.swift`.
//!
//! Tap-not-hold semantics for Right Command: if the user taps Right Cmd
//! with no other key pressed in between, fire the toggle. Holding Right
//! Cmd as a chord modifier (`Cmd+Q`, etc.) does *not* fire — `kVK_Escape`
//! and any keyDown event with Right Cmd held marks the press as a chord,
//! suppressing the toggle on release.
//!
//! Escape is observed on the same tap and fires `do_cancel`. Core's phase
//! machine ignores cancel when not recording, so the hook never has to
//! gate.
//!
//! Threading: tap installs on a dedicated thread w/ `CFRunLoopRun`. Main
//! thread can stop it via `CFRunLoopStop` (thread-safe per Apple). A 5 s
//! watchdog re-enables the tap if it goes silently stale (sleep/wake, TCC
//! revoke). Tap-disabled-by-timeout / by-user-input is also re-enabled
//! inline from the callback for fast recovery.

use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::mach_port::CFMachPortRef;
use core_foundation::runloop::{
    kCFRunLoopCommonModes, CFRunLoop, CFRunLoopRef, CFRunLoopRun, CFRunLoopStop,
};
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CGEventTapProxy, CallbackResult, CGEvent, EventField,
};
use tauri::AppHandle;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(
        options: *const std::ffi::c_void, // CFDictionaryRef
    ) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

/// Ask AX whether we're trusted, popping the system prompt if not. Trusted
/// apps return true silently; untrusted apps get the OS dialog AND the
/// `com.openwhisper.app` entry added to System Settings → Privacy →
/// Accessibility, where the user can flip it on.
fn ax_check_trust_with_prompt() -> bool {
    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let value = CFBoolean::true_value();
    let opts = CFDictionary::from_CFType_pairs(&[(key, value)]);
    unsafe {
        AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef() as *const std::ffi::c_void)
    }
}

use crate::{do_cancel, do_toggle};

// Bit set inside CGEventFlags when Right Command is held. See
// `IOKit/hidsystem/ev_keymap.h` `NX_DEVICERCMDKEYMASK`. Same magic as the
// Swift port (`HotkeyService.swift:29`).
const RIGHT_COMMAND_MASK: u64 = 0x0010;
// kVK_Escape from `Carbon/HIToolbox/Events.h`.
const KV_ESCAPE: i64 = 0x35;

const WATCHDOG_INTERVAL: Duration = Duration::from_secs(5);

extern "C" {
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventTapIsEnabled(tap: CFMachPortRef) -> bool;
}

#[derive(Default)]
struct TapMutState {
    right_command_down: AtomicBool,
    other_pressed_while_held: AtomicBool,
}

struct TapHandles {
    thread: JoinHandle<()>,
    watchdog: JoinHandle<()>,
    run_loop_ref: usize,
    /// Set by the watchdog stop-flag — drops the watchdog out of its sleep
    /// loop without waiting for the next 5 s tick.
    watchdog_stop: Arc<AtomicBool>,
}

// SAFETY: run_loop_ref is `*mut __CFRunLoop`. CFRunLoopStop is thread-safe
// per Apple, so sending the raw ptr across threads (in the controller) is
// sound as long as we never deref it directly — only pass it to CFRunLoopStop.
unsafe impl Send for TapHandles {}

static STATE: OnceLock<Mutex<Option<TapHandles>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<TapHandles>> {
    STATE.get_or_init(|| Mutex::new(None))
}

pub fn install(_app: &AppHandle) -> Result<(), String> {
    teardown_existing();

    // Check AX trust first — and prompt the user if needed. The prompt
    // adds OpenWhisper to System Settings → Privacy & Security →
    // Accessibility (if not already there) and shows the standard
    // "would like to control your computer" dialog. Skips the tap attempt
    // when trust is missing so we surface a clear actionable error
    // instead of "CGEventTap creation failed".
    if !ax_check_trust_with_prompt() {
        return Err(
            "Accessibility permission needed. System Settings just opened — \
             toggle OpenWhisper on, then click Restart."
                .into(),
        );
    }

    spawn_tap()
}

/// Stop the CGEventTap and watchdog without re-installing. Used by the
/// fullscreen-aware path: when the user enters a fullscreen app we don't
/// want OpenWhisper to even respond to Right Cmd taps, so we drop the
/// system-wide tap entirely. Re-installed via [`install`] on fullscreen
/// exit.
pub fn teardown() {
    teardown_existing();
}

fn teardown_existing() {
    let prev = slot().lock().unwrap().take();
    if let Some(prev) = prev {
        unsafe {
            let rl = prev.run_loop_ref as CFRunLoopRef;
            if !rl.is_null() {
                CFRunLoopStop(rl);
            }
        }
        prev.watchdog_stop.store(true, Ordering::Relaxed);
        let _ = prev.thread.join();
        let _ = prev.watchdog.join();
    }
}

fn spawn_tap() -> Result<(), String> {
    let (tx, rx) = mpsc::channel::<Result<(usize, usize), String>>();
    let state = Arc::new(TapMutState::default());
    let port_ptr: Arc<AtomicPtr<core::ffi::c_void>> = Arc::new(AtomicPtr::new(std::ptr::null_mut()));

    let state_for_thread = state.clone();
    let port_ptr_for_thread = port_ptr.clone();

    let thread = thread::Builder::new()
        .name("openwhisper-cgeventtap".into())
        .spawn(move || run_tap_thread(tx, state_for_thread, port_ptr_for_thread))
        .map_err(|e| format!("spawn tap thread: {e}"))?;

    let (run_loop_handle, _mach_port_handle) = rx
        .recv()
        .map_err(|e| format!("tap thread died before ready: {e}"))??;

    let watchdog_stop = Arc::new(AtomicBool::new(false));
    let watchdog_stop_thread = watchdog_stop.clone();
    let port_ptr_for_watchdog = port_ptr.clone();
    let watchdog = thread::Builder::new()
        .name("openwhisper-cgeventtap-watchdog".into())
        .spawn(move || run_watchdog(watchdog_stop_thread, port_ptr_for_watchdog))
        .map_err(|e| format!("spawn watchdog: {e}"))?;

    *slot().lock().unwrap() = Some(TapHandles {
        thread,
        watchdog,
        run_loop_ref: run_loop_handle,
        watchdog_stop,
    });

    Ok(())
}

fn run_tap_thread(
    ready: mpsc::Sender<Result<(usize, usize), String>>,
    state: Arc<TapMutState>,
    port_ptr: Arc<AtomicPtr<core::ffi::c_void>>,
) {
    let port_ptr_cb = port_ptr.clone();
    let state_cb = state.clone();
    let callback = move |_proxy: CGEventTapProxy, etype: CGEventType, event: &CGEvent| {
        // System fires these synthetic events when it disables the tap
        // (callback exceeded ~1 s budget, user-input policing, internal).
        // Re-enable inline before bouncing — same as the Swift port.
        if matches!(
            etype,
            CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
        ) {
            let raw = port_ptr_cb.load(Ordering::Relaxed);
            if !raw.is_null() {
                unsafe { CGEventTapEnable(raw as CFMachPortRef, true) }
            }
            return CallbackResult::Keep;
        }

        handle_event(&state_cb, etype, event);
        // Always pass through — observe, never swallow.
        CallbackResult::Keep
    };

    let tap = match unsafe {
        CGEventTap::new_unchecked(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![CGEventType::FlagsChanged, CGEventType::KeyDown],
            callback,
        )
    } {
        Ok(t) => t,
        Err(()) => {
            let trusted = unsafe { AXIsProcessTrusted() };
            let pid = std::process::id();
            let exe = std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "?".into());
            // Diagnostic to stderr — keeps the banner short.
            eprintln!("hotkey: CGEventTap failed (trusted={trusted} pid={pid} exe={exe})");

            let msg = if trusted {
                "Accessibility granted but the hotkey tap is still blocked — \
                 click Restart to relaunch the app and apply the grant."
            } else {
                "Accessibility permission needed. Open System Settings → \
                 Privacy & Security → Accessibility, toggle OpenWhisper on, \
                 then click Restart."
            };
            let _ = ready.send(Err(msg.into()));
            return;
        }
    };

    let port_raw = tap.mach_port().as_concrete_TypeRef() as *mut core::ffi::c_void;
    port_ptr.store(port_raw, Ordering::Relaxed);

    let source = match tap.mach_port().create_runloop_source(0) {
        Ok(s) => s,
        Err(()) => {
            let _ = ready.send(Err("create_runloop_source failed".into()));
            return;
        }
    };

    let runloop = CFRunLoop::get_current();
    runloop.add_source(&source, unsafe { kCFRunLoopCommonModes });
    let runloop_raw = runloop.as_concrete_TypeRef() as usize;

    // Enable, signal ready, then block on CFRunLoopRun. Returns when
    // someone calls CFRunLoopStop on this run loop's ref.
    unsafe { CGEventTapEnable(port_raw as CFMachPortRef, true) };
    let _ = ready.send(Ok((runloop_raw, port_raw as usize)));

    unsafe { CFRunLoopRun() };

    // Tap drops here — Drop calls CFMachPortInvalidate.
    drop(tap);
    // Clear port ptr so any racing watchdog tick is a no-op.
    port_ptr.store(std::ptr::null_mut(), Ordering::Relaxed);
}

fn handle_event(state: &TapMutState, etype: CGEventType, event: &CGEvent) {
    let flags = event.get_flags().bits();
    let right_down = (flags & RIGHT_COMMAND_MASK) != 0;
    let key_code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);

    match etype {
        CGEventType::FlagsChanged => {
            let was_down = state.right_command_down.load(Ordering::Relaxed);
            if right_down && !was_down {
                state.right_command_down.store(true, Ordering::Relaxed);
                state.other_pressed_while_held.store(false, Ordering::Relaxed);
            } else if !right_down && was_down {
                state.right_command_down.store(false, Ordering::Relaxed);
                let chord = state.other_pressed_while_held.swap(false, Ordering::Relaxed);
                if !chord {
                    // do_toggle is cheap (atomic + thread spawn for the
                    // recognizer load); calling it directly on the tap
                    // thread is fine.
                    if let Err(e) = do_toggle() {
                        eprintln!("Right Cmd toggle failed: {e}");
                    }
                }
            }
        }
        CGEventType::KeyDown => {
            if state.right_command_down.load(Ordering::Relaxed) {
                state.other_pressed_while_held.store(true, Ordering::Relaxed);
            }
            if key_code == KV_ESCAPE {
                let _ = do_cancel();
            }
        }
        _ => {}
    }
}

fn run_watchdog(stop: Arc<AtomicBool>, port_ptr: Arc<AtomicPtr<core::ffi::c_void>>) {
    let mut elapsed = Duration::ZERO;
    let tick = Duration::from_millis(200);
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(tick);
        elapsed += tick;
        if elapsed < WATCHDOG_INTERVAL {
            continue;
        }
        elapsed = Duration::ZERO;

        let raw = port_ptr.load(Ordering::Relaxed);
        if raw.is_null() {
            continue;
        }
        unsafe {
            if !CGEventTapIsEnabled(raw as CFMachPortRef) {
                CGEventTapEnable(raw as CFMachPortRef, true);
            }
        }
    }
}
