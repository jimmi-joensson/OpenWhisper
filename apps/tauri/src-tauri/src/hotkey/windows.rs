//! Windows hotkey: a single `WH_KEYBOARD_LL` hook owns chord activation
//! for every slot in the table in `hotkey/mod.rs` (`SLOTS`): toggle
//! (default Ctrl+Space), cancel (default Esc) and the optional ones.
//! All are configurable from Settings → Shortcuts. Chords only: a
//! modifier-tap binding is unsupported on Windows.
//!
//! Why a low-level hook (issue #7): `RegisterHotKey` bleeds into Electron
//! / WPF apps that install their own keyboard hooks ahead of us in the
//! chain. WH_KEYBOARD_LL is system-wide, runs before the focused app, and
//! can swallow the chord by returning a non-zero LRESULT.
//!
//! A slot marked `gate_on_recording` (cancel) only matches while
//! `dictation::is_recording()`. Outside an active recording its binding
//! passes through to the focused app.
//!
//! Capture mode: when `crate::hotkey::is_capture_active()` is true the hook
//! diverts to capture-only — every keyboard event is swallowed; the first
//! non-modifier KeyDown emits a chord descriptor.
//!
//! Re-install policy: install once at boot + on every settings save; never
//! on a periodic timer. An earlier watchdog tried reinstalling every 3 s
//! to defend chain order, which under sustained churn corrupted the
//! kernel input thread state on Windows (stuck modifier flags + scancode
//! reordering surviving process exit). Manual recovery is via the
//! existing `hotkey_retry` Tauri command.
//!
//! Logging policy: actual errors (e.g. spawned `do_toggle()` failed) use
//! always-on `eprintln!`. Per-event diagnostics (install slots, hook
//! liveness, chord match, capture, mod mismatch, per-keydown trace) are
//! gated behind `verbose_log!` so a normal `pnpm dev:tauri` console stays
//! quiet. Run with `pnpm dev:tauri --verbose` to see them.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle};

use tauri::AppHandle;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_QUIT,
};

use super::{SkippedSlot, SlotSpec, SLOTS};
use crate::settings::{HotkeyConfig, HotkeyKind, HotkeySettings};
use openwhisper_core::{dictation, verbose_log};

const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;

const MOD_CTRL: u32 = 1 << 0;
const MOD_SHIFT: u32 = 1 << 1;
const MOD_ALT: u32 = 1 << 2;
const MOD_WIN: u32 = 1 << 3;

/// Global event counter — incremented on every callback invocation. Powers
/// the "first event received" diagnostic eprintln so we can confirm the
/// hook is alive.
static EVENT_COUNT: AtomicU64 = AtomicU64::new(0);

/// Per-slot compiled binding read lock-free from the hook callback.
/// `vk = 0` means "nothing installed for this slot": an optional slot
/// that is unbound or was skipped, or any slot before the first install.
struct SlotAtomic {
    vk: AtomicU32,
    mods: AtomicU32,
    swallowed_down: AtomicBool,
    fired: AtomicBool,
}

impl SlotAtomic {
    const fn new() -> Self {
        Self {
            vk: AtomicU32::new(0),
            mods: AtomicU32::new(0),
            swallowed_down: AtomicBool::new(false),
            fired: AtomicBool::new(false),
        }
    }

    fn set(&self, vk: u32, mods: u32) {
        self.vk.store(vk, Ordering::Relaxed);
        self.mods.store(mods, Ordering::Relaxed);
        self.swallowed_down.store(false, Ordering::Relaxed);
        self.fired.store(false, Ordering::Relaxed);
    }
}

/// One per entry of `SLOTS`, same order: `HOOK_SLOTS[i]` holds the
/// binding of `SLOTS[i]`, and the hook fires `SLOTS[i].action`.
static HOOK_SLOTS: [SlotAtomic; SLOTS.len()] = [const { SlotAtomic::new() }; SLOTS.len()];

struct EscapeHookState {
    thread: Option<JoinHandle<()>>,
    thread_id: u32,
}

static HOOK_STATE: OnceLock<Mutex<Option<EscapeHookState>>> = OnceLock::new();

fn hook_state() -> &'static Mutex<Option<EscapeHookState>> {
    HOOK_STATE.get_or_init(|| Mutex::new(None))
}

/// Install the hook for every bound slot. `Ok` carries the optional
/// slots that were left out (see `hotkey::compile_slots`).
pub fn install(_app: &AppHandle, settings: &HotkeySettings) -> Result<Vec<SkippedSlot>, String> {
    let (compiled, skipped) = super::compile_slots(settings, compile_config)?;
    // Unbound and skipped slots go back to "nothing installed".
    for slot in &HOOK_SLOTS {
        slot.set(0, 0);
    }
    for (index, (vk, mods)) in compiled {
        HOOK_SLOTS[index].set(vk, mods);
        verbose_log!(
            "[ow.hotkey/win] install: {} vk=0x{:02X} mods=0x{:02X}",
            SLOTS[index].target.as_str(),
            vk,
            mods
        );
    }
    install_hook()?;
    Ok(skipped)
}

pub fn teardown(_app: &AppHandle) {
    teardown_hook();
}

fn teardown_hook() {
    let mut guard = hook_state().lock().unwrap();
    if let Some(prev) = guard.take() {
        unsafe {
            let _ = PostThreadMessageW(prev.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = prev.thread {
            let _ = t.join();
        }
    }
    for slot in &HOOK_SLOTS {
        slot.swallowed_down.store(false, Ordering::Relaxed);
        slot.fired.store(false, Ordering::Relaxed);
    }
}

fn install_hook() -> Result<(), String> {
    teardown_hook();

    let (tx, rx) = std::sync::mpsc::channel::<Result<u32, String>>();

    let thread = thread::Builder::new()
        .name("openwhisper-keyboard-hook".into())
        .spawn(move || run_hook_thread(tx))
        .map_err(|e| format!("spawn keyboard hook thread: {e}"))?;

    let tid = rx
        .recv()
        .map_err(|e| format!("keyboard hook thread died: {e}"))??;

    let mut guard = hook_state().lock().unwrap();
    *guard = Some(EscapeHookState {
        thread: Some(thread),
        thread_id: tid,
    });
    Ok(())
}

fn run_hook_thread(ready: std::sync::mpsc::Sender<Result<u32, String>>) {
    unsafe {
        let tid = GetCurrentThreadId();
        let hmod = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(e) => {
                let _ = ready.send(Err(format!("GetModuleHandle: {e}")));
                return;
            }
        };
        let hinst: HINSTANCE = hmod.into();
        let hook: HHOOK =
            match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_callback), Some(hinst), 0) {
                Ok(h) => h,
                Err(e) => {
                    let _ = ready.send(Err(format!(
                        "SetWindowsHookEx WH_KEYBOARD_LL failed (AV blocking?): {e}"
                    )));
                    return;
                }
            };

        let _ = ready.send(Ok(tid));

        let mut msg = MSG::default();
        while GetMessageW(&mut msg as *mut MSG, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg as *const MSG);
            DispatchMessageW(&msg as *const MSG);
        }

        let _ = UnhookWindowsHookEx(hook);
    }
}

unsafe fn current_mod_mask() -> u32 {
    let down = |vk: i32| -> bool { (GetAsyncKeyState(vk) as u16) & 0x8000 != 0 };
    let mut m = 0u32;
    if down(0x11) {
        m |= MOD_CTRL;
    }
    if down(0x10) {
        m |= MOD_SHIFT;
    }
    if down(0x12) {
        m |= MOD_ALT;
    }
    if down(0x5B) || down(0x5C) {
        m |= MOD_WIN;
    }
    m
}

unsafe extern "system" fn hook_callback(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code >= 0 {
        let prev = EVENT_COUNT.fetch_add(1, Ordering::Relaxed);
        if prev == 0 {
            verbose_log!("[ow.hotkey/win] hook alive — first event received");
        }
        let msg = w_param.0 as u32;
        let info = unsafe { *(l_param.0 as *const KBDLLHOOKSTRUCT) };
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        if crate::hotkey::is_capture_active() {
            if is_down && !is_modifier_vk(info.vkCode) {
                if let Some(code) = vk_to_chord_name(info.vkCode) {
                    let mods = unsafe { current_mod_mask() };
                    let mod_names = mods_to_names(mods);
                    verbose_log!(
                        "[ow.hotkey/win] capture: vk=0x{:02X} ({}) mods=0x{:02X} ({:?})",
                        info.vkCode, code, mods, mod_names
                    );
                    crate::hotkey::deliver_capture(HotkeyConfig::chord(code, &mod_names));
                }
            }
            return LRESULT(1);
        }

        let held = unsafe { current_mod_mask() };

        // Per-keydown trace, gated behind --verbose. Diagnoses chord
        // detection: shows the exact vk + held-mod mask the hook sees on
        // every press, to compare with what each slot was installed
        // with (the `install:` lines).
        if is_down {
            verbose_log!(
                "[ow.hotkey/win] keydown vk=0x{:02X} held_mods=0x{:02X}",
                info.vkCode,
                held
            );
        }

        // First matching slot wins, in table order.
        for (slot, spec) in HOOK_SLOTS.iter().zip(SLOTS) {
            if try_match_slot(slot, spec, info.vkCode, is_down, is_up, held) {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}

fn try_match_slot(
    slot: &SlotAtomic,
    spec: &SlotSpec,
    vk: u32,
    is_down: bool,
    is_up: bool,
    held: u32,
) -> bool {
    let target = slot.vk.load(Ordering::Relaxed);
    if target == 0 || vk != target {
        return false;
    }
    let label = spec.target.as_str();
    let required = slot.mods.load(Ordering::Relaxed);
    if is_down && held != required && held != 0 {
        // Target vk arrived under SOME modifier(s) but not the configured
        // mask — e.g., you bound Ctrl+Space but pressed Shift+Space. Skip
        // the bare-vk case (held == 0) so the log doesn't fire on every
        // word-separator space typed in prose.
        verbose_log!(
            "[ow.hotkey/win] {label}: target vk=0x{:02X} arrived but mods mismatch \
             (held=0x{:02X}, need=0x{:02X})",
            vk, held, required
        );
    }
    if is_down && held == required {
        if spec.gate_on_recording && !dictation::is_recording() {
            return false;
        }
        if !slot.fired.swap(true, Ordering::Relaxed) {
            verbose_log!(
                "[ow.hotkey/win] {label}: chord matched (vk=0x{:02X} mods=0x{:02X}), firing",
                vk, held
            );
            // The action hands its work to a thread; the hook callback
            // has to return within the system's hook timeout.
            (spec.action)();
        }
        slot.swallowed_down.store(true, Ordering::Relaxed);
        return true;
    }
    if is_up && slot.swallowed_down.swap(false, Ordering::Relaxed) {
        slot.fired.store(false, Ordering::Relaxed);
        return true;
    }
    false
}

/// Compile a binding to `(vk, modifier mask)`. `Err` is a sentence for
/// the user, shown when an optional slot is left out because of it.
fn compile_config(config: &HotkeyConfig) -> Result<(u32, u32), String> {
    if !matches!(config.kind, HotkeyKind::Chord) {
        return Err(
            "A single modifier key is unsupported on Windows. Record a key combination instead."
                .into(),
        );
    }
    let unsupported = || "This key combination is not supported on Windows.".to_string();
    let vk = chord_name_to_vk(&config.code).ok_or_else(unsupported)?;
    let mut mods = 0u32;
    for m in &config.mods {
        mods |= mod_name_to_mask(m).ok_or_else(unsupported)?;
    }
    Ok((vk, mods))
}

fn chord_name_to_vk(name: &str) -> Option<u32> {
    Some(match name {
        "Space" => 0x20,
        "Tab" => 0x09,
        "Return" => 0x0D,
        "Escape" => 0x1B,
        "Delete" => 0x08,
        "ForwardDelete" => 0x2E,
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        "F1" => 0x70, "F2" => 0x71, "F3" => 0x72, "F4" => 0x73,
        "F5" => 0x74, "F6" => 0x75, "F7" => 0x76, "F8" => 0x77,
        "F9" => 0x78, "F10" => 0x79, "F11" => 0x7A, "F12" => 0x7B,
        n if n.len() == 1 => {
            let b = n.as_bytes()[0];
            if b.is_ascii_uppercase() || b.is_ascii_digit() {
                b as u32
            } else {
                return None;
            }
        }
        _ => return None,
    })
}

fn vk_to_chord_name(vk: u32) -> Option<&'static str> {
    Some(match vk {
        0x20 => "Space",
        0x09 => "Tab",
        0x0D => "Return",
        0x1B => "Escape",
        0x08 => "Delete",
        0x2E => "ForwardDelete",
        0x25 => "ArrowLeft",
        0x26 => "ArrowUp",
        0x27 => "ArrowRight",
        0x28 => "ArrowDown",
        0x70 => "F1", 0x71 => "F2", 0x72 => "F3", 0x73 => "F4",
        0x74 => "F5", 0x75 => "F6", 0x76 => "F7", 0x77 => "F8",
        0x78 => "F9", 0x79 => "F10", 0x7A => "F11", 0x7B => "F12",
        0x41 => "A", 0x42 => "B", 0x43 => "C", 0x44 => "D", 0x45 => "E",
        0x46 => "F", 0x47 => "G", 0x48 => "H", 0x49 => "I", 0x4A => "J",
        0x4B => "K", 0x4C => "L", 0x4D => "M", 0x4E => "N", 0x4F => "O",
        0x50 => "P", 0x51 => "Q", 0x52 => "R", 0x53 => "S", 0x54 => "T",
        0x55 => "U", 0x56 => "V", 0x57 => "W", 0x58 => "X", 0x59 => "Y",
        0x5A => "Z",
        0x30 => "0", 0x31 => "1", 0x32 => "2", 0x33 => "3", 0x34 => "4",
        0x35 => "5", 0x36 => "6", 0x37 => "7", 0x38 => "8", 0x39 => "9",
        _ => return None,
    })
}

fn mod_name_to_mask(name: &str) -> Option<u32> {
    Some(match name {
        "Ctrl" | "Control" => MOD_CTRL,
        "Shift" => MOD_SHIFT,
        "Alt" | "Option" => MOD_ALT,
        "Win" | "Cmd" => MOD_WIN,
        _ => return None,
    })
}

fn mods_to_names(mask: u32) -> Vec<&'static str> {
    let mut out = Vec::new();
    if mask & MOD_CTRL != 0 {
        out.push("Ctrl");
    }
    if mask & MOD_ALT != 0 {
        out.push("Alt");
    }
    if mask & MOD_SHIFT != 0 {
        out.push("Shift");
    }
    if mask & MOD_WIN != 0 {
        out.push("Win");
    }
    out
}

fn is_modifier_vk(vk: u32) -> bool {
    matches!(
        vk,
        0x10 | 0x11 | 0x12
        | 0xA0 | 0xA1 | 0xA2 | 0xA3 | 0xA4 | 0xA5
        | 0x5B | 0x5C
        | 0x14 | 0x90
    )
}
