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
