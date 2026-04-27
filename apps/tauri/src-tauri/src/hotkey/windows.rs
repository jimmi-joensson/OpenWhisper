//! Windows hotkey: single `WH_KEYBOARD_LL` hook owns both Ctrl+Space
//! activation AND in-recording Escape cancel.
//!
//! Why a low-level hook for activation (issue #7):
//! - `RegisterHotKey` (used by `tauri-plugin-global-shortcut`) is supposed
//!   to consume the chord, but in practice it bleeds into Electron / WPF
//!   apps that install their own keyboard hooks ahead of us in the chain
//!   (Outlook, Slack, VS Code, etc.). Pressing Ctrl+Space toggles
//!   OpenWhisper *and* triggers the focused app's own Ctrl+Space action.
//! - A `WH_KEYBOARD_LL` hook is system-wide, runs before the focused app
//!   sees the event, and can swallow it by returning a non-zero `LRESULT`
//!   instead of forwarding to `CallNextHookEx`.
//!
//! Escape is gated on `dictation::is_recording()` — outside an active
//! recording it passes through normally so users still get Escape in
//! every other app.
//!
//! KeyDown / KeyUp pairing: when we swallow a KeyDown we also swallow
//! the matching KeyUp; otherwise the focused app sees an orphan KeyUp
//! and may end up with stuck modifier state. Tracked with two atomic
//! flags (one per hotkey) on the hook thread.

use std::sync::atomic::{AtomicBool, Ordering};
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

use crate::{do_cancel, do_toggle};
use openwhisper_core::dictation;

const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_CONTROL: i32 = 0x11;
const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;

/// Set when we swallow a Ctrl+Space KeyDown so the matching KeyUp is
/// also swallowed regardless of whether Ctrl is still held when the
/// space-up event arrives.
static SPACE_SWALLOWED_DOWN: AtomicBool = AtomicBool::new(false);
/// Same idea for Escape during recording.
static ESCAPE_SWALLOWED_DOWN: AtomicBool = AtomicBool::new(false);
/// Suppress key-repeat: Windows fires WM_KEYDOWN repeatedly while the
/// chord is held. We toggle once on the leading edge, swallow the rest.
static SPACE_TOGGLE_FIRED: AtomicBool = AtomicBool::new(false);

struct EscapeHookState {
    thread: Option<JoinHandle<()>>,
    thread_id: u32,
}

static HOOK_STATE: OnceLock<Mutex<Option<EscapeHookState>>> = OnceLock::new();

fn hook_state() -> &'static Mutex<Option<EscapeHookState>> {
    HOOK_STATE.get_or_init(|| Mutex::new(None))
}

pub fn install(_app: &AppHandle) -> Result<(), String> {
    install_hook()
}

/// Stop the keyboard hook — fullscreen-aware path. Re-installed via
/// [`install`] on fullscreen exit so the foreground fullscreen app
/// receives Ctrl+Space / Escape normally.
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
    SPACE_SWALLOWED_DOWN.store(false, Ordering::Relaxed);
    ESCAPE_SWALLOWED_DOWN.store(false, Ordering::Relaxed);
    SPACE_TOGGLE_FIRED.store(false, Ordering::Relaxed);
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

        // Private message pump. WH_KEYBOARD_LL needs one on the installing
        // thread for callback dispatch. Loop exits on WM_QUIT posted via
        // `PostThreadMessageW` from `teardown_hook` on retry/fullscreen.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg as *mut MSG, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg as *const MSG);
            DispatchMessageW(&msg as *const MSG);
        }

        let _ = UnhookWindowsHookEx(hook);
    }
}

/// Low-level keyboard hook callback. Runs on the hook thread and must
/// finish well under 300 ms or Windows silently unloads the hook
/// (`LowLevelHooksTimeout`). All real work — `do_toggle`, `do_cancel` —
/// runs on a worker thread; the callback itself only inspects vkCodes
/// and atomic flags.
unsafe extern "system" fn hook_callback(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code >= 0 {
        let msg = w_param.0 as u32;
        let info = unsafe { *(l_param.0 as *const KBDLLHOOKSTRUCT) };
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        // --- Ctrl+Space activation: always swallow when the chord matches.
        if info.vkCode == VK_SPACE {
            // GetAsyncKeyState's high bit reflects "currently down".
            let ctrl_down = unsafe { GetAsyncKeyState(VK_CONTROL) } as u16 & 0x8000 != 0;
            if is_down && ctrl_down {
                if !SPACE_TOGGLE_FIRED.swap(true, Ordering::Relaxed) {
                    thread::spawn(|| {
                        if let Err(e) = do_toggle() {
                            eprintln!("Ctrl+Space toggle failed: {e}");
                        }
                    });
                }
                SPACE_SWALLOWED_DOWN.store(true, Ordering::Relaxed);
                return LRESULT(1);
            }
            if is_up && SPACE_SWALLOWED_DOWN.swap(false, Ordering::Relaxed) {
                SPACE_TOGGLE_FIRED.store(false, Ordering::Relaxed);
                return LRESULT(1);
            }
        }

        // --- Escape: only swallow while a recording is active.
        if info.vkCode == VK_ESCAPE {
            if is_down && dictation::is_recording() {
                thread::spawn(|| {
                    let _ = do_cancel();
                });
                ESCAPE_SWALLOWED_DOWN.store(true, Ordering::Relaxed);
                return LRESULT(1);
            }
            if is_up && ESCAPE_SWALLOWED_DOWN.swap(false, Ordering::Relaxed) {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}
