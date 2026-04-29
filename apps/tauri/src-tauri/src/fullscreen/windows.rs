//! Windows fullscreen detection — direct port of
//! `apps/windows/OpenWhisper/PillWindow.xaml.cs::IsForegroundAppFullscreen`.
//!
//! True when the foreground window's bounds match (or exceed) the full
//! monitor rect — exclusively-fullscreen games (D3D), borderless
//! fullscreen, and presentation modes all match. The work area is NOT
//! used as the comparison basis; that would also catch ordinary
//! maximized windows, which should NOT gate dictation off.
//!
//! Chromeless-monitor edge case: the rect-vs-`rcMonitor` test alone is
//! not enough on a monitor without a taskbar (third-party shell, or a
//! secondary display with the taskbar disabled in Settings →
//! Personalization → Taskbar → "Show my taskbar on all displays" off).
//! On those screens `rcWork == rcMonitor`, so a normally-maximized
//! browser/IDE/Slack matches the geometry and trips the check — the
//! user sees the pill disappear and the hotkey deactivate the moment
//! they switch to that screen.
//!
//! Strategy: check `rcWork == rcMonitor` BEFORE applying any style-bit
//! filter. On a normal screen with a taskbar, `rcWork < rcMonitor`, so
//! a maximized normal window only reaches `rcWork`; if `win_rect`
//! reaches `rcMonitor` we know it must be fullscreen and we return
//! true with no further checks. Style bits are consulted only on
//! chromeless screens, where maximized and fullscreen both reach
//! `rcMonitor` and we need a tiebreaker.
//!
//! Tiebreaker on chromeless screens: `WS_MAXIMIZE` plus chrome bits
//! (`WS_CAPTION` titlebar or `WS_THICKFRAME` sizing border). Real
//! maximized windows always retain at least one chrome bit; popup-style
//! fullscreen flows (Chromium F11, D3D-exclusive games, PowerPoint
//! slideshow, Win+Shift+Enter terminal full-screen) strip both, so the
//! tiebreaker excludes them correctly.
//!
//! Known limitation: UWP fullscreen apps (Minecraft Bedrock, Xbox app)
//! run inside `ApplicationFrameWindow` which keeps WS_MAXIMIZE *and*
//! chrome bits even in fullscreen. They are detected correctly on any
//! screen with a visible taskbar (rcWork-vs-rcMonitor short-circuit),
//! but mis-detected as "maximized normal" if the user runs them
//! fullscreen on a chromeless secondary monitor. Documented; not
//! chased without a reproduction request.
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

use tauri::{AppHandle, Monitor};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowThreadProcessId, GWL_STYLE, WS_CAPTION, WS_MAXIMIZE, WS_THICKFRAME,
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

/// Foreground HWND + its window rect + the MONITORINFO of the
/// containing monitor — `Some` only when no skip condition fires
/// (invalid HWND, OW's own pid, shell-class window, or any GDI call
/// failing). Used by `is_fullscreen_now`; the HWND is in the tuple so
/// the chromeless branch reads style bits from the same window that
/// produced the rect (no second `GetForegroundWindow` race).
fn foreground_monitor_info() -> Option<(HWND, RECT, MONITORINFO)> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return None;
        }

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == GetCurrentProcessId() {
            return None;
        }

        if let Some(class) = window_class_name(fg) {
            if SHELL_CLASSES.iter().any(|c| *c == class) {
                return None;
            }
        }

        let mut win_rect = RECT::default();
        if GetWindowRect(fg, &mut win_rect).is_err() {
            return None;
        }

        let monitor = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        if monitor.is_invalid() {
            return None;
        }

        let mut mi = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
            return None;
        }

        Some((fg, win_rect, mi))
    }
}

pub fn is_fullscreen_now() -> bool {
    let Some((fg, win_rect, mi)) = foreground_monitor_info() else {
        return false;
    };

    let covers_monitor = win_rect.left <= mi.rcMonitor.left
        && win_rect.top <= mi.rcMonitor.top
        && win_rect.right >= mi.rcMonitor.right
        && win_rect.bottom >= mi.rcMonitor.bottom;
    if !covers_monitor {
        return false;
    }

    let chromeless = mi.rcWork.left == mi.rcMonitor.left
        && mi.rcWork.top == mi.rcMonitor.top
        && mi.rcWork.right == mi.rcMonitor.right
        && mi.rcWork.bottom == mi.rcMonitor.bottom;
    if !chromeless {
        return true;
    }

    unsafe {
        let style = GetWindowLongPtrW(fg, GWL_STYLE) as u32;
        let is_maximized = style & WS_MAXIMIZE.0 != 0;
        let has_chrome = style & (WS_CAPTION.0 | WS_THICKFRAME.0) != 0;
        !(is_maximized && has_chrome)
    }
}

/// Origin `(left, top)` of the monitor hosting the cursor, in
/// physical-px virtual-screen coordinates (same space as
/// `MONITORINFO.rcMonitor`). Returns `None` only on a `GetCursorPos`
/// or `GetMonitorInfoW` failure (extremely rare).
///
/// Cursor-tracking replaced an earlier focused-window approach
/// (TASK-55.3) which routed through `GetForegroundWindow` + AX-style
/// window rect lookup. Cursor-based works uniformly across all app
/// frameworks (Win32, Electron, UWP) and matches the macOS sibling.
pub fn cursor_monitor() -> Option<(i32, i32)> {
    unsafe {
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_err() {
            return None;
        }
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        if monitor.is_invalid() {
            return None;
        }
        let mut mi = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
            return None;
        }
        Some((mi.rcMonitor.left, mi.rcMonitor.top))
    }
}

/// Y coordinate (logical points; Quartz / virtual-screen top-left
/// origin) of the bottom of the given monitor's work area — i.e. the
/// top edge of the taskbar on this monitor, or the screen's bottom
/// edge when the taskbar isn't present here. Falls back to the
/// monitor's own bottom edge if `MonitorFromPoint` /
/// `GetMonitorInfoW` fails.
pub fn work_area_bottom_y(monitor: &Monitor) -> f64 {
    let scale = monitor.scale_factor();
    let mon_x = monitor.position().x;
    let mon_y = monitor.position().y;
    let mon_w = monitor.size().width as i32;
    let mon_h = monitor.size().height as i32;
    let fallback = (mon_y + mon_h) as f64 / scale;

    let center = POINT {
        x: mon_x + mon_w / 2,
        y: mon_y + mon_h / 2,
    };
    unsafe {
        let hmon = MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST);
        if hmon.is_invalid() {
            return fallback;
        }
        let mut mi = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(hmon, &mut mi).as_bool() {
            return fallback;
        }
        // rcWork is in physical pixels in virtual-screen coords; the
        // pill placement math is in logical points.
        mi.rcWork.bottom as f64 / scale
    }
}

/// Look up the `tauri::Monitor` whose position matches the watcher's
/// origin tuple. Both sides are physical px in virtual-screen
/// coordinates on Windows, so a direct compare is correct — no
/// conversion needed (unlike the macOS sibling).
pub fn find_tauri_monitor(app: &AppHandle, origin: (i32, i32)) -> Option<Monitor> {
    let monitors = app.available_monitors().ok()?;
    for m in monitors {
        let p = m.position();
        if (p.x, p.y) == origin {
            return Some(m);
        }
    }
    None
}
