//! Windows hotkey: single `WH_KEYBOARD_LL` hook owns both toggle (default
//! Ctrl+Space) and cancel (default Esc) chord activation. Both slots are
//! configurable from Settings → Shortcuts.
//!
//! Why a low-level hook (issue #7): `RegisterHotKey` bleeds into Electron
//! / WPF apps that install their own keyboard hooks ahead of us in the
//! chain. WH_KEYBOARD_LL is system-wide, runs before the focused app, and
//! can swallow the chord by returning a non-zero LRESULT.
//!
//! Cancel slot is gated on `dictation::is_recording()` — outside an active
//! recording the cancel binding passes through to the focused app.
//!
//! Capture mode: when `crate::hotkey::is_capture_active()` is true the hook
//! diverts to capture-only — every keyboard event is swallowed; the first
//! non-modifier KeyDown emits a chord descriptor.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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

use crate::settings::{HotkeyConfig, HotkeyKind, HotkeySettings};
use crate::{do_cancel, do_toggle};
use openwhisper_core::dictation;

const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;

const MOD_CTRL: u32 = 1 << 0;
const MOD_SHIFT: u32 = 1 << 1;
const MOD_ALT: u32 = 1 << 2;
const MOD_WIN: u32 = 1 << 3;

/// Per-slot compiled binding read lock-free from the hook callback.
/// `vk = 0` means "slot disabled" (compile failed → install errored).
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

static TOGGLE: SlotAtomic = SlotAtomic::new();
static CANCEL: SlotAtomic = SlotAtomic::new();

struct EscapeHookState {
    thread: Option<JoinHandle<()>>,
    thread_id: u32,
}

static HOOK_STATE: OnceLock<Mutex<Option<EscapeHookState>>> = OnceLock::new();

fn hook_state() -> &'static Mutex<Option<EscapeHookState>> {
    HOOK_STATE.get_or_init(|| Mutex::new(None))
}

pub fn install(_app: &AppHandle, settings: &HotkeySettings) -> Result<(), String> {
    let (tvk, tmods) = compile_config(&settings.toggle).ok_or_else(|| {
        format!(
            "Unsupported toggle hotkey: {:?} {} {:?}",
            settings.toggle.kind, settings.toggle.code, settings.toggle.mods
        )
    })?;
    let (cvk, cmods) = compile_config(&settings.cancel).ok_or_else(|| {
        format!(
            "Unsupported cancel hotkey: {:?} {} {:?}",
            settings.cancel.kind, settings.cancel.code, settings.cancel.mods
        )
    })?;
    TOGGLE.set(tvk, tmods);
    CANCEL.set(cvk, cmods);
    install_hook()
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
    TOGGLE.swallowed_down.store(false, Ordering::Relaxed);
    TOGGLE.fired.store(false, Ordering::Relaxed);
    CANCEL.swallowed_down.store(false, Ordering::Relaxed);
    CANCEL.fired.store(false, Ordering::Relaxed);
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
        let msg = w_param.0 as u32;
        let info = unsafe { *(l_param.0 as *const KBDLLHOOKSTRUCT) };
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        if crate::hotkey::is_capture_active() {
            if is_down && !is_modifier_vk(info.vkCode) {
                if let Some(code) = vk_to_chord_name(info.vkCode) {
                    let mods = unsafe { current_mod_mask() };
                    let mod_names = mods_to_names(mods);
                    crate::hotkey::deliver_capture(HotkeyConfig::chord(code, &mod_names));
                }
            }
            return LRESULT(1);
        }

        let held = unsafe { current_mod_mask() };

        // Toggle slot.
        if try_match_slot(&TOGGLE, info.vkCode, is_down, is_up, held, false) {
            return LRESULT(1);
        }
        // Cancel slot — gate on recording.
        if try_match_slot(
            &CANCEL,
            info.vkCode,
            is_down,
            is_up,
            held,
            true, /* gate_on_recording */
        ) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}

fn try_match_slot(
    slot: &SlotAtomic,
    vk: u32,
    is_down: bool,
    is_up: bool,
    held: u32,
    gate_on_recording: bool,
) -> bool {
    let target = slot.vk.load(Ordering::Relaxed);
    if target == 0 || vk != target {
        return false;
    }
    let required = slot.mods.load(Ordering::Relaxed);
    if is_down && held == required {
        if gate_on_recording && !dictation::is_recording() {
            return false;
        }
        if !slot.fired.swap(true, Ordering::Relaxed) {
            // Pick the action by slot identity. Comparing static
            // refs is the simplest tag — both pointers are unique
            // and stable for the program's lifetime.
            let toggle_ptr = std::ptr::addr_of!(TOGGLE) as *const ();
            let slot_ptr = (slot as *const SlotAtomic) as *const ();
            if std::ptr::eq(slot_ptr, toggle_ptr) {
                thread::spawn(|| {
                    if let Err(e) = do_toggle() {
                        eprintln!("hotkey toggle failed: {e}");
                    }
                });
            } else {
                thread::spawn(|| {
                    let _ = do_cancel();
                });
            }
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

fn compile_config(config: &HotkeyConfig) -> Option<(u32, u32)> {
    if !matches!(config.kind, HotkeyKind::Chord) {
        return None;
    }
    let vk = chord_name_to_vk(&config.code)?;
    let mut mods = 0u32;
    for m in &config.mods {
        mods |= mod_name_to_mask(m)?;
    }
    Some((vk, mods))
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
