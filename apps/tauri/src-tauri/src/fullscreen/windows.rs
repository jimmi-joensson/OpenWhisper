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

use std::mem::size_of;

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
};

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
