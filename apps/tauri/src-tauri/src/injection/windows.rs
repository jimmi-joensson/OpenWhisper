//! Windows Ctrl+V via SendInput. Direct port of
//! `apps/windows/OpenWhisper/TextInjection/TextInjector.cs::SendCtrlV`.
//!
//! Per-character SendInput Unicode delivery was tried in the WinUI 3 shell
//! and lost characters mid-stream when focus wobbled (see TextInjector.cs
//! header comment). Clipboard set + Ctrl+V is one atomic op the receiving
//! app handles in a single shot, immune to focus races.

use std::mem::size_of;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL,
};
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow};

/// VK_V from the Windows virtual-key set. Not exposed by the windows crate's
/// VIRTUAL_KEY constants (only the named keys are), so spell it out.
const VK_V: VIRTUAL_KEY = VIRTUAL_KEY(0x56);

pub fn synthesize_paste() {
    let inputs = [
        keyboard_input(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
        keyboard_input(VK_V, KEYBD_EVENT_FLAGS(0)),
        keyboard_input(VK_V, KEYEVENTF_KEYUP),
        keyboard_input(VK_CONTROL, KEYEVENTF_KEYUP),
    ];

    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        eprintln!(
            "inject: SendInput Ctrl+V sent {sent}/{} (expected all 4)",
            inputs.len()
        );
    }
}

/// True when the foreground window is a Chromium/Electron host. Detected
/// by class name — Electron + Chrome/Edge/Brave/Opera all use
/// `Chrome_WidgetWin_1` (occasionally `_0`). Used by injection/mod.rs to
/// gate the long post-paste delay: only Chromium reads clipboard formats
/// asynchronously after Ctrl+V (the bug GitHub issue #6 reports), so
/// native Win32 controls (Notepad, cmd, Windows Terminal, WinForms) and
/// non-Chromium browsers (Firefox = `MozillaWindowClass`) get the short
/// 200 ms restore window instead of paying the 2 s margin.
pub fn foreground_is_chromium() -> bool {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return false;
        }
        let mut buf = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut buf);
        if len <= 0 {
            return false;
        }
        let class = String::from_utf16_lossy(&buf[..len as usize]);
        class.starts_with("Chrome_WidgetWin")
    }
}

fn keyboard_input(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
