//! Windows fullscreen detection — direct port of
//! `apps/windows/OpenWhisper/PillWindow.xaml.cs::IsForegroundAppFullscreen`.
//!
//! True when the foreground window's bounds match (or exceed) the full
//! monitor rect — exclusively-fullscreen games (D3D), borderless
//! fullscreen, and presentation modes all match. The work area is NOT
//! used as the comparison basis; that would also catch ordinary
//! maximized windows, which should NOT gate dictation off.
//!
//! Self-window check uses the foreground window's process id (via
//! GetWindowThreadProcessId) compared to GetCurrentProcessId, rather
//! than tracking individual HWNDs. Same effect, less bookkeeping —
//! covers main, pill, and any future window we add.
//!
//! Shell-surface exclusion: clicking the empty desktop on Win 11 sets
//! the foreground window to `Progman` / `WorkerW`, whose rects span
//! the whole monitor. Without filtering these, the pill hides every
//! time the user clicks away from a real app. Same goes for the
//! taskbar (`Shell_TrayWnd`, `Shell_SecondaryTrayWnd`) which
//! technically covers the full monitor width on the screen edge it
//! lives on.

use std::mem::size_of;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
};

/// Window classes for the Windows desktop + taskbar — never treat them
/// as fullscreen apps. Order matters only for readability.
const SHELL_CLASSES: &[&str] = &[
    "Progman",                // Desktop (Program Manager)
    "WorkerW",                // Desktop wallpaper worker
    "Shell_TrayWnd",          // Primary taskbar
    "Shell_SecondaryTrayWnd", // Secondary-monitor taskbar
];

fn window_class_name(hwnd: HWND) -> Option<String> {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    if len <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

pub fn is_fullscreen_now() -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return false;
        }

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == GetCurrentProcessId() {
            return false;
        }

        if let Some(class) = window_class_name(fg) {
            if SHELL_CLASSES.iter().any(|c| *c == class) {
                return false;
            }
        }

        let mut win_rect = RECT::default();
        if GetWindowRect(fg, &mut win_rect).is_err() {
            return false;
        }

        let monitor = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        if monitor.is_invalid() {
            return false;
        }

        let mut mi = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
            return false;
        }

        win_rect.left <= mi.rcMonitor.left
            && win_rect.top <= mi.rcMonitor.top
            && win_rect.right >= mi.rcMonitor.right
            && win_rect.bottom >= mi.rcMonitor.bottom
    }
}
