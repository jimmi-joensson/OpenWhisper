//! Windows hotkey: Ctrl+Space chord via `tauri-plugin-global-shortcut`,
//! plus a dedicated WH_KEYBOARD_LL hook on its own thread for Escape.
//!
//! Why two surfaces:
//! - The plugin handles cross-app chord registration cleanly via
//!   `RegisterHotKey`. That's all we need for activation.
//! - Escape can't be a `RegisterHotKey` because it's an unmodified key
//!   that every app expects to receive — we observe it system-wide via a
//!   low-level keyboard hook and never swallow it.
//!
//! Mirrors `apps/windows/OpenWhisper/Hotkey/EscapeHook.cs` (low-level hook
//! + private message pump on a dedicated thread) and the chord-registration
//! pattern from `apps/windows/OpenWhisper/Hotkey/GlobalHotkey.cs`.

use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle};

use tauri::AppHandle;
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_QUIT,
};

use crate::do_cancel;

const VK_ESCAPE: u32 = 0x1B;
const WM_KEYDOWN: u32 = 0x0100;
const WM_SYSKEYDOWN: u32 = 0x0104;

const TOGGLE_SHORTCUT: &str = "Control+Space";

struct EscapeHookState {
    thread: Option<JoinHandle<()>>,
    thread_id: u32,
}

static HOOK_STATE: OnceLock<Mutex<Option<EscapeHookState>>> = OnceLock::new();

fn hook_state() -> &'static Mutex<Option<EscapeHookState>> {
    HOOK_STATE.get_or_init(|| Mutex::new(None))
}

pub fn install(app: &AppHandle) -> Result<(), String> {
    register_chord(app)?;
    install_escape_hook()?;
    Ok(())
}

/// Unregister the Ctrl+Space chord and stop the Escape hook thread —
/// fullscreen-aware path. Re-installed via [`install`] on fullscreen exit.
pub fn teardown(app: &AppHandle) {
    let _ = app.global_shortcut().unregister(TOGGLE_SHORTCUT);
    teardown_escape_hook();
}

fn teardown_escape_hook() {
    let mut guard = hook_state().lock().unwrap();
    if let Some(prev) = guard.take() {
        unsafe {
            let _ = PostThreadMessageW(prev.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = prev.thread {
            let _ = t.join();
        }
    }
}

fn register_chord(app: &AppHandle) -> Result<(), String> {
    let gs = app.global_shortcut();
    // Idempotent retry: unregister anything we'd previously registered.
    let _ = gs.unregister_all();
    gs.register(TOGGLE_SHORTCUT)
        .map_err(|e| format!("register {TOGGLE_SHORTCUT} (chord conflict?): {e}"))
}

fn install_escape_hook() -> Result<(), String> {
    // Tear down any previous hook thread before spawning a new one.
    teardown_escape_hook();

    let (tx, rx) = std::sync::mpsc::channel::<Result<u32, String>>();

    let thread = thread::Builder::new()
        .name("openwhisper-escape-hook".into())
        .spawn(move || run_hook_thread(tx))
        .map_err(|e| format!("spawn escape hook thread: {e}"))?;

    let tid = rx
        .recv()
        .map_err(|e| format!("escape hook thread died: {e}"))??;

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
        // `PostThreadMessageW` from `install_escape_hook` on retry.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg as *mut MSG, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg as *const MSG);
            DispatchMessageW(&msg as *const MSG);
        }

        let _ = UnhookWindowsHookEx(hook);
    }
}

/// Low-level keyboard hook callback. Runs on the hook thread, must complete
/// well under 300 ms or Windows silently unloads the hook
/// (`LowLevelHooksTimeout`). Off-loads do_cancel to a worker thread.
unsafe extern "system" fn hook_callback(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code >= 0 {
        let msg = w_param.0 as u32;
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            let info = unsafe { *(l_param.0 as *const KBDLLHOOKSTRUCT) };
            if info.vkCode == VK_ESCAPE {
                // do_cancel hits a Mutex inside `audio` and can briefly block;
                // never run it on the hook thread. Phase machine in the core
                // ignores cancel when not recording, so racing is fine.
                thread::spawn(|| {
                    let _ = do_cancel();
                });
            }
        }
    }
    // NEVER swallow Escape — every other app expects it to reach focus.
    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}
