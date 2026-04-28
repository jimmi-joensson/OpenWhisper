//! macOS hotkey via CGEventTap. Two configurable slots — toggle (start/stop)
//! and cancel (discard the current recording). Each slot supports both
//! modifier-tap (single key, tap-not-hold semantics) and chord (mods + key).
//!
//! Tap-not-hold semantics for modifier-tap: the slot fires on release iff
//! no other key was pressed while the modifier was held — so holding the
//! modifier as a chord (`Cmd+Q`, etc.) does *not* fire the slot.
//!
//! Chord semantics: fire on the configured non-modifier KeyDown when the
//! configured high-level modifier mask matches exactly. Both KeyDown and
//! the matching KeyUp are swallowed so the focused app does not also
//! receive the chord.
//!
//! Cancel slot is gated on `dictation::is_recording()` — outside an active
//! recording the cancel binding passes through normally so the focused app
//! still sees Escape (or whatever the user picked).
//!
//! Capture mode: when `crate::hotkey::is_capture_active()` is true the
//! handler diverts to capture-only — every keyboard event is swallowed,
//! the first eligible event is delivered to the front-end via
//! `hotkey::deliver_capture`, and capture mode auto-exits.
//!
//! Threading: tap installs on a dedicated thread w/ `CFRunLoopRun`. A 5 s
//! watchdog re-enables the tap if it goes silently stale.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicPtr, Ordering};
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

use crate::settings::{HotkeyConfig, HotkeyKind, HotkeySettings};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(
        options: *const std::ffi::c_void, // CFDictionaryRef
    ) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

fn ax_trust_check(app: &AppHandle) -> bool {
    if unsafe { AXIsProcessTrusted() } {
        return true;
    }
    // Bring main forward right before the OS prompt fires — without
    // this, an `accessory`-policy app stays behind whatever owned focus
    // (often Terminal during dev, Finder for first-launch users), and
    // the user misses OW's banner explaining the grant.
    crate::focus::bring_main_to_front(app);
    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let value = CFBoolean::true_value();
    let opts = CFDictionary::from_CFType_pairs(&[(key, value)]);
    unsafe {
        AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef() as *const std::ffi::c_void)
    }
}

use crate::{do_cancel, do_toggle};
use openwhisper_core::dictation;

// Device-side modifier bits (NX_DEVICE*KEYMASK from
// `IOKit/hidsystem/ev_keymap.h`). Used for modifier-tap detection — they
// distinguish left vs. right modifier keys; high-level CGEventFlags
// collapse them.
const NX_DEVICE_LCTRL: u64 = 0x0001;
const NX_DEVICE_LSHIFT: u64 = 0x0002;
const NX_DEVICE_RSHIFT: u64 = 0x0004;
const NX_DEVICE_LCMD: u64 = 0x0008;
const NX_DEVICE_RCMD: u64 = 0x0010;
const NX_DEVICE_LOPT: u64 = 0x0020;
const NX_DEVICE_ROPT: u64 = 0x0040;
const NX_DEVICE_RCTRL: u64 = 0x2000;

// High-level CGEventFlags — collapse left/right. Used for chord matching.
const CG_FLAG_SHIFT: u64 = 0x0002_0000;
const CG_FLAG_CONTROL: u64 = 0x0004_0000;
const CG_FLAG_ALT: u64 = 0x0008_0000;
const CG_FLAG_CMD: u64 = 0x0010_0000;
const CG_FLAG_MOD_MASK: u64 = CG_FLAG_SHIFT | CG_FLAG_CONTROL | CG_FLAG_ALT | CG_FLAG_CMD;

const WATCHDOG_INTERVAL: Duration = Duration::from_secs(5);

extern "C" {
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventTapIsEnabled(tap: CFMachPortRef) -> bool;
}

/// Per-slot compiled config + atomics. One for toggle, one for cancel.
struct SlotState {
    kind: HotkeyKind,
    /// kVK of the configured key. Modifier-tap = the modifier kVK; chord
    /// = the non-modifier kVK.
    code_kv: i64,
    /// Modifier-tap = device-side bit. Chord = high-level mask (collapsed).
    mod_mask: u64,
    /// Action this slot fires when matched.
    action: SlotAction,
    /// Only fire if `dictation::is_recording()` — true for cancel slot.
    gate_on_recording: bool,
    /// Modifier-tap state.
    mod_down: AtomicBool,
    other_pressed_while_held: AtomicBool,
    /// Chord-mode KeyUp pairing.
    chord_keydown_swallowed: AtomicBool,
    chord_fired: AtomicBool,
}

#[derive(Clone, Copy)]
enum SlotAction {
    Toggle,
    Cancel,
}

impl SlotState {
    fn from_config(
        cfg: &HotkeyConfig,
        action: SlotAction,
        gate_on_recording: bool,
    ) -> Option<Self> {
        match cfg.kind {
            HotkeyKind::ModifierTap => {
                let (kv, mask) = mac_modifier_lookup(&cfg.code)?;
                Some(Self {
                    kind: HotkeyKind::ModifierTap,
                    code_kv: kv,
                    mod_mask: mask,
                    action,
                    gate_on_recording,
                    mod_down: AtomicBool::new(false),
                    other_pressed_while_held: AtomicBool::new(false),
                    chord_keydown_swallowed: AtomicBool::new(false),
                    chord_fired: AtomicBool::new(false),
                })
            }
            HotkeyKind::Chord => {
                let kv = mac_chord_kv_lookup(&cfg.code)?;
                let mut mask: u64 = 0;
                for m in &cfg.mods {
                    mask |= mac_chord_mod_mask(m)?;
                }
                Some(Self {
                    kind: HotkeyKind::Chord,
                    code_kv: kv,
                    mod_mask: mask,
                    action,
                    gate_on_recording,
                    mod_down: AtomicBool::new(false),
                    other_pressed_while_held: AtomicBool::new(false),
                    chord_keydown_swallowed: AtomicBool::new(false),
                    chord_fired: AtomicBool::new(false),
                })
            }
        }
    }

    fn fire(&self) {
        match self.action {
            SlotAction::Toggle => {
                if let Err(e) = do_toggle() {
                    eprintln!("hotkey toggle failed: {e}");
                }
            }
            SlotAction::Cancel => {
                std::thread::spawn(|| {
                    let _ = do_cancel();
                });
            }
        }
    }

    fn gate_passes(&self) -> bool {
        !self.gate_on_recording || dictation::is_recording()
    }
}

#[derive(Default)]
struct CaptureState {
    /// kVK of the modifier currently pressed alone, or `-1`.
    pending_mod: AtomicI64,
    chord_seen: AtomicBool,
}

struct TapMutState {
    toggle: SlotState,
    cancel: SlotState,
    capture: CaptureState,
}

struct TapHandles {
    thread: JoinHandle<()>,
    watchdog: JoinHandle<()>,
    run_loop_ref: usize,
    watchdog_stop: Arc<AtomicBool>,
}

unsafe impl Send for TapHandles {}

static STATE: OnceLock<Mutex<Option<TapHandles>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<TapHandles>> {
    STATE.get_or_init(|| Mutex::new(None))
}

pub fn install(app: &AppHandle, settings: &HotkeySettings) -> Result<(), String> {
    teardown_existing();
    let app_name = crate::product_name(app);

    if !ax_trust_check(app) {
        return Err(format!(
            "Accessibility permission needed. System Settings just opened — \
             toggle {app_name} on, then click Restart."
        ));
    }

    let toggle = SlotState::from_config(&settings.toggle, SlotAction::Toggle, false).ok_or_else(
        || {
            format!(
                "Unsupported toggle hotkey: kind={:?} code={} mods={:?}",
                settings.toggle.kind, settings.toggle.code, settings.toggle.mods
            )
        },
    )?;
    let cancel = SlotState::from_config(&settings.cancel, SlotAction::Cancel, true).ok_or_else(
        || {
            format!(
                "Unsupported cancel hotkey: kind={:?} code={} mods={:?}",
                settings.cancel.kind, settings.cancel.code, settings.cancel.mods
            )
        },
    )?;

    let capture = CaptureState::default();
    capture.pending_mod.store(-1, Ordering::Relaxed);
    let state = Arc::new(TapMutState { toggle, cancel, capture });

    spawn_tap(app_name, state)
}

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

fn spawn_tap(app_name: String, state: Arc<TapMutState>) -> Result<(), String> {
    let (tx, rx) = mpsc::channel::<Result<(usize, usize), String>>();
    let port_ptr: Arc<AtomicPtr<core::ffi::c_void>> = Arc::new(AtomicPtr::new(std::ptr::null_mut()));

    let state_for_thread = state.clone();
    let port_ptr_for_thread = port_ptr.clone();

    let thread = thread::Builder::new()
        .name("openwhisper-cgeventtap".into())
        .spawn(move || run_tap_thread(tx, state_for_thread, port_ptr_for_thread, app_name))
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
    app_name: String,
) {
    let port_ptr_cb = port_ptr.clone();
    let state_cb = state.clone();
    let callback = move |_proxy: CGEventTapProxy, etype: CGEventType, event: &CGEvent| {
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

        if handle_event(&state_cb, etype, event) {
            CallbackResult::Drop
        } else {
            CallbackResult::Keep
        }
    };

    let tap = match unsafe {
        CGEventTap::new_unchecked(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![
                CGEventType::FlagsChanged,
                CGEventType::KeyDown,
                CGEventType::KeyUp,
            ],
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
            eprintln!("hotkey: CGEventTap failed (trusted={trusted} pid={pid} exe={exe})");

            let msg: String = if trusted {
                "Accessibility granted but the hotkey tap is still blocked — \
                 click Restart to relaunch the app and apply the grant."
                    .into()
            } else {
                format!(
                    "Accessibility permission needed. Open System Settings → \
                     Privacy & Security → Accessibility, toggle {app_name} on, \
                     then click Restart."
                )
            };
            let _ = ready.send(Err(msg));
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

    unsafe { CGEventTapEnable(port_raw as CFMachPortRef, true) };
    let _ = ready.send(Ok((runloop_raw, port_raw as usize)));

    unsafe { CFRunLoopRun() };

    drop(tap);
    port_ptr.store(std::ptr::null_mut(), Ordering::Relaxed);
}

/// Returns `true` if the event should be dropped (swallowed). Called once
/// per keyboard event from the CGEventTap callback.
fn handle_event(state: &TapMutState, etype: CGEventType, event: &CGEvent) -> bool {
    if crate::hotkey::is_capture_active() {
        return capture_handle_event(&state.capture, etype, event);
    }

    let flags = event.get_flags().bits();
    let key_code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);

    // Modifier-tap dispatch on FlagsChanged. Each slot tracks its own
    // press/release state — we route the same event to both slots so a
    // user who configured both as different modifier-taps gets both
    // tracked correctly.
    if matches!(etype, CGEventType::FlagsChanged) {
        let mut swallow = false;
        if matches!(state.toggle.kind, HotkeyKind::ModifierTap) {
            swallow |= modifier_tap_step(&state.toggle, flags, key_code);
        }
        if matches!(state.cancel.kind, HotkeyKind::ModifierTap) {
            swallow |= modifier_tap_step(&state.cancel, flags, key_code);
        }
        return swallow;
    }

    // Mark "other key pressed" for any active modifier-tap slot the
    // moment a non-modifier KeyDown arrives.
    if matches!(etype, CGEventType::KeyDown) {
        for slot_ref in [&state.toggle, &state.cancel] {
            if matches!(slot_ref.kind, HotkeyKind::ModifierTap)
                && slot_ref.mod_down.load(Ordering::Relaxed)
            {
                slot_ref.other_pressed_while_held.store(true, Ordering::Relaxed);
            }
        }
    }

    match etype {
        CGEventType::KeyDown => {
            for slot_ref in [&state.toggle, &state.cancel] {
                if matches!(slot_ref.kind, HotkeyKind::Chord)
                    && slot_ref.gate_passes()
                    && key_code == slot_ref.code_kv
                    && (flags & CG_FLAG_MOD_MASK) == slot_ref.mod_mask
                {
                    if !slot_ref.chord_fired.swap(true, Ordering::Relaxed) {
                        slot_ref.fire();
                    }
                    slot_ref.chord_keydown_swallowed.store(true, Ordering::Relaxed);
                    return true;
                }
            }
            false
        }
        CGEventType::KeyUp => {
            for slot_ref in [&state.toggle, &state.cancel] {
                if matches!(slot_ref.kind, HotkeyKind::Chord)
                    && key_code == slot_ref.code_kv
                    && slot_ref
                        .chord_keydown_swallowed
                        .swap(false, Ordering::Relaxed)
                {
                    slot_ref.chord_fired.store(false, Ordering::Relaxed);
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

/// Run one modifier-tap state step for a single slot. Returns true iff
/// this event should be swallowed (i.e. the FlagsChanged keycode matches
/// the slot's configured modifier).
fn modifier_tap_step(slot_ref: &SlotState, flags: u64, key_code: i64) -> bool {
    let mod_active = (flags & slot_ref.mod_mask) != 0;
    let was_down = slot_ref.mod_down.load(Ordering::Relaxed);
    if mod_active && !was_down {
        slot_ref.mod_down.store(true, Ordering::Relaxed);
        slot_ref.other_pressed_while_held.store(false, Ordering::Relaxed);
        key_code == slot_ref.code_kv
    } else if !mod_active && was_down {
        slot_ref.mod_down.store(false, Ordering::Relaxed);
        let chord = slot_ref.other_pressed_while_held.swap(false, Ordering::Relaxed);
        if !chord && slot_ref.gate_passes() {
            slot_ref.fire();
        }
        key_code == slot_ref.code_kv
    } else {
        false
    }
}

/// Capture-mode handler — running while the Settings → Shortcuts pane has
/// asked us to record the next eligible event. Swallows everything; emits
/// the first valid descriptor we see.
fn capture_handle_event(capture: &CaptureState, etype: CGEventType, event: &CGEvent) -> bool {
    let flags = event.get_flags().bits();
    let key_code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);

    match etype {
        CGEventType::FlagsChanged => {
            let pending = capture.pending_mod.load(Ordering::Relaxed);
            let Some(name) = mac_modifier_kv_to_name(key_code) else {
                return true;
            };
            let device_mask = mac_modifier_kv_to_device_mask(key_code).unwrap_or(0);
            let pressed = (flags & device_mask) != 0;

            if pressed {
                capture.pending_mod.store(key_code, Ordering::Relaxed);
                capture.chord_seen.store(false, Ordering::Relaxed);
            } else if pending == key_code {
                capture.pending_mod.store(-1, Ordering::Relaxed);
                let chord = capture.chord_seen.swap(false, Ordering::Relaxed);
                if !chord {
                    crate::hotkey::deliver_capture(HotkeyConfig::modifier_tap(name));
                }
            } else {
                capture.pending_mod.store(-1, Ordering::Relaxed);
                capture.chord_seen.store(false, Ordering::Relaxed);
            }
            true
        }
        CGEventType::KeyDown => {
            capture.chord_seen.store(true, Ordering::Relaxed);
            if let Some(code) = mac_chord_kv_to_name(key_code) {
                let mods = mac_flags_to_chord_mods(flags);
                crate::hotkey::deliver_capture(HotkeyConfig::chord(code, &mods));
                capture.pending_mod.store(-1, Ordering::Relaxed);
            }
            true
        }
        CGEventType::KeyUp => true,
        _ => false,
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

// ---------------------------------------------------------------------------
// Symbolic-name ↔ kVK / mask tables.

fn mac_modifier_lookup(name: &str) -> Option<(i64, u64)> {
    Some(match name {
        "RightCommand" => (0x36, NX_DEVICE_RCMD),
        "LeftCommand" => (0x37, NX_DEVICE_LCMD),
        "RightShift" => (0x3C, NX_DEVICE_RSHIFT),
        "LeftShift" => (0x38, NX_DEVICE_LSHIFT),
        "RightOption" => (0x3D, NX_DEVICE_ROPT),
        "LeftOption" => (0x3A, NX_DEVICE_LOPT),
        "RightControl" => (0x3E, NX_DEVICE_RCTRL),
        "LeftControl" => (0x3B, NX_DEVICE_LCTRL),
        _ => return None,
    })
}

fn mac_modifier_kv_to_name(kv: i64) -> Option<&'static str> {
    Some(match kv {
        0x36 => "RightCommand",
        0x37 => "LeftCommand",
        0x3C => "RightShift",
        0x38 => "LeftShift",
        0x3D => "RightOption",
        0x3A => "LeftOption",
        0x3E => "RightControl",
        0x3B => "LeftControl",
        _ => return None,
    })
}

fn mac_modifier_kv_to_device_mask(kv: i64) -> Option<u64> {
    Some(match kv {
        0x36 => NX_DEVICE_RCMD,
        0x37 => NX_DEVICE_LCMD,
        0x3C => NX_DEVICE_RSHIFT,
        0x38 => NX_DEVICE_LSHIFT,
        0x3D => NX_DEVICE_ROPT,
        0x3A => NX_DEVICE_LOPT,
        0x3E => NX_DEVICE_RCTRL,
        0x3B => NX_DEVICE_LCTRL,
        _ => return None,
    })
}

fn mac_chord_kv_lookup(name: &str) -> Option<i64> {
    Some(match name {
        "Space" => 0x31,
        "Tab" => 0x30,
        "Return" => 0x24,
        "Escape" => 0x35,
        "Delete" => 0x33,
        "ForwardDelete" => 0x75,
        "ArrowLeft" => 0x7B,
        "ArrowRight" => 0x7C,
        "ArrowUp" => 0x7E,
        "ArrowDown" => 0x7D,
        "A" => 0x00, "S" => 0x01, "D" => 0x02, "F" => 0x03, "H" => 0x04,
        "G" => 0x05, "Z" => 0x06, "X" => 0x07, "C" => 0x08, "V" => 0x09,
        "B" => 0x0B, "Q" => 0x0C, "W" => 0x0D, "E" => 0x0E, "R" => 0x0F,
        "Y" => 0x10, "T" => 0x11, "O" => 0x1F, "U" => 0x20, "I" => 0x22,
        "P" => 0x23, "L" => 0x25, "J" => 0x26, "K" => 0x28, "N" => 0x2D,
        "M" => 0x2E,
        "1" => 0x12, "2" => 0x13, "3" => 0x14, "4" => 0x15, "5" => 0x17,
        "6" => 0x16, "7" => 0x1A, "8" => 0x1C, "9" => 0x19, "0" => 0x1D,
        "F1" => 0x7A, "F2" => 0x78, "F3" => 0x63, "F4" => 0x76,
        "F5" => 0x60, "F6" => 0x61, "F7" => 0x62, "F8" => 0x64,
        "F9" => 0x65, "F10" => 0x6D, "F11" => 0x67, "F12" => 0x6F,
        _ => return None,
    })
}

fn mac_chord_kv_to_name(kv: i64) -> Option<&'static str> {
    Some(match kv {
        0x31 => "Space", 0x30 => "Tab", 0x24 => "Return",
        0x35 => "Escape", 0x33 => "Delete", 0x75 => "ForwardDelete",
        0x7B => "ArrowLeft", 0x7C => "ArrowRight",
        0x7E => "ArrowUp", 0x7D => "ArrowDown",
        0x00 => "A", 0x01 => "S", 0x02 => "D", 0x03 => "F", 0x04 => "H",
        0x05 => "G", 0x06 => "Z", 0x07 => "X", 0x08 => "C", 0x09 => "V",
        0x0B => "B", 0x0C => "Q", 0x0D => "W", 0x0E => "E", 0x0F => "R",
        0x10 => "Y", 0x11 => "T", 0x1F => "O", 0x20 => "U", 0x22 => "I",
        0x23 => "P", 0x25 => "L", 0x26 => "J", 0x28 => "K", 0x2D => "N",
        0x2E => "M",
        0x12 => "1", 0x13 => "2", 0x14 => "3", 0x15 => "4", 0x17 => "5",
        0x16 => "6", 0x1A => "7", 0x1C => "8", 0x19 => "9", 0x1D => "0",
        0x7A => "F1", 0x78 => "F2", 0x63 => "F3", 0x76 => "F4",
        0x60 => "F5", 0x61 => "F6", 0x62 => "F7", 0x64 => "F8",
        0x65 => "F9", 0x6D => "F10", 0x67 => "F11", 0x6F => "F12",
        _ => return None,
    })
}

fn mac_chord_mod_mask(name: &str) -> Option<u64> {
    Some(match name {
        "Cmd" => CG_FLAG_CMD,
        "Shift" => CG_FLAG_SHIFT,
        "Alt" | "Option" => CG_FLAG_ALT,
        "Ctrl" | "Control" => CG_FLAG_CONTROL,
        _ => return None,
    })
}

fn mac_flags_to_chord_mods(flags: u64) -> Vec<&'static str> {
    let mut out = Vec::new();
    if flags & CG_FLAG_CONTROL != 0 {
        out.push("Ctrl");
    }
    if flags & CG_FLAG_ALT != 0 {
        out.push("Alt");
    }
    if flags & CG_FLAG_SHIFT != 0 {
        out.push("Shift");
    }
    if flags & CG_FLAG_CMD != 0 {
        out.push("Cmd");
    }
    out
}
