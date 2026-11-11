//! Windows: one monitor copied with GDI, into memory.
//!
//! - **Nothing can be left out of the capture.** Windows has no way to
//!   copy a whole monitor without the windows of chosen apps (the one
//!   switch there is, `SetWindowDisplayAffinity`, belongs to the
//!   window's owner). So `exclude` is ignored, `excluded_apps` is 0,
//!   and the only layer for a listed app that is not in front is the
//!   caller's paint over [`Platform::window_rects`], read before and
//!   after the capture. That is the design (TASK-98, decision 10), and
//!   it covers more than needed when a listed window is partly hidden
//!   behind another one, which is the safe side. For the same reason
//!   this process's own windows (the pill, the answer panel) are in
//!   the image.
//! - **Which windows count.** Every top-level window (`EnumWindows`:
//!   popups, menus and floating panels are top-level windows too) that
//!   is visible, not minimised and not cloaked. Cloaked (`DWMWA_CLOAKED`)
//!   means the window manager does not draw it: it is on another
//!   virtual desktop, or it is the frame of a suspended packaged app.
//!   When the cloaked flag cannot be read the window stays in the
//!   list.
//! - **Rectangles** are the DWM extended frame bounds, the frame a
//!   person sees, without the invisible resize border `GetWindowRect`
//!   adds. `GetWindowRect` is the fallback when DWM gives nothing.
//! - **Whose window it is** is the exe file name of the owning
//!   process in lower case (`1password.exe`), the id [`crate::focus`]
//!   reports, compared with [`id_listed`]: case is ignored, and a
//!   list entry without the `.exe` ending (`1password`) covers the
//!   exe too.
//! - **An owner that cannot be identified is a refusal.** Reading the
//!   exe name means opening the process, which Windows denies for
//!   some of them (a program run as another user, as an administrator
//!   on some setups, or a protected system process). Such a window
//!   cannot be told from a password manager's, so while one is on
//!   screen and the never-capture list is not empty, `window_rects`
//!   returns `Err` and there is no screenshot. This can cost
//!   screenshots for as long as such a program has a window open.
//!   The other choice would be to skip the window, and then a
//!   password manager started as an administrator would be captured
//!   unpainted: losing a screenshot is the mistake a person can
//!   recover from. A window that closed while it was being looked at
//!   is not a refusal, it is simply gone.
//! - **Coordinates and DPI.** Windows' virtual desktop already is the
//!   space [`PointRect`] wants: origin at the primary monitor's top
//!   left corner, y down, monitors to the left or above negative. But
//!   the numbers are real pixels only for a thread that is per-monitor
//!   DPI aware. The desktop app is; a command-line process usually is
//!   not, and would get scaled numbers from some calls and real ones
//!   from others, so a painted rectangle could miss its window. Both
//!   `window_rects` and `capture` therefore make the calling thread
//!   per-monitor aware for the length of the call
//!   (`SetThreadDpiAwarenessContext`, v2, else v1) and put the old
//!   setting back after. A thread that cannot be switched is a
//!   refusal. Needs Windows 10 1607.
//! - **Which monitor.** The one that holds the foreground window, the
//!   primary one when there is no foreground window; with `at`, the
//!   one that contains that point.
//! - **The copy.** `BitBlt` from the screen with `CAPTUREBLT`, so
//!   layered (see-through) windows are in it. The mouse pointer is
//!   not. A window whose owner asked not to be captured
//!   (`SetWindowDisplayAffinity`) comes out black or absent, which is
//!   the system keeping that promise for us.
//! - **Permission.** There is none to ask for.
//!
//! Known limits:
//!
//! - A packaged (UWP) app's window belongs to
//!   `ApplicationFrameHost.exe`, not to the app's own exe. Listing the
//!   app's exe name neither recognises it as being in front nor paints
//!   its window; listing `applicationframehost.exe` covers every such
//!   app at once. Desktop programs installed from the Store (the usual
//!   password managers) own their windows and are not affected.
//! - A window is painted where it is when the windows are read. The
//!   moving picture Windows shows while a window is minimised or
//!   restored is not at that place; the read before and the read after
//!   the capture bound that gap, they do not close it.
//!
//! The parts that are plain arithmetic and rules (rectangles, names,
//! which windows are selected, the byte order) are written without a
//! Win32 call so they are unit tested on any platform. None of the
//! Win32 part has run on the machine it was written on: the
//! `windows_tests` module at the end is for the first run on a real
//! one.

use super::{AppSelector, FrontApp, PointRect, id_listed};
#[cfg(target_os = "windows")]
use super::{PermissionState, Platform, PlatformError, RawCapture};

/// The largest side of a monitor that is taken for real. Anything
/// beyond it is a display that is not there.
const MAX_SIDE: i64 = 32_768;

/// See the module docs: "An owner that cannot be identified is a
/// refusal".
const UNKNOWN_OWNER: &str = "the program behind a window on screen could not be identified";

/// A window that has to be painted and has no rectangle to paint.
const UNMEASURED: &str = "a window that must not be captured could not be measured";

/// The edges of a rectangle as Win32 reports them (a `RECT`): right
/// and bottom are one past the last pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Edges {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Edges {
    /// The same rectangle in the trait's coordinates. The space is
    /// already the right one (see the module docs), so this only
    /// changes the shape. `None` for a rectangle with no area: there
    /// is nothing of it on screen.
    fn to_rect(self) -> Option<PointRect> {
        // In 64 bits: the difference of two `i32` can overflow one.
        let width = self.right as i64 - self.left as i64;
        let height = self.bottom as i64 - self.top as i64;
        if width <= 0 || height <= 0 {
            return None;
        }
        Some(PointRect { x: self.left as f64, y: self.top as f64, width: width as f64, height: height as f64 })
    }

    /// Width and height in pixels, for a monitor. `None` when it has
    /// no area or a size no monitor has.
    fn pixel_size(self) -> Option<(u32, u32)> {
        let width = self.right as i64 - self.left as i64;
        let height = self.bottom as i64 - self.top as i64;
        if !(1..=MAX_SIDE).contains(&width) || !(1..=MAX_SIDE).contains(&height) {
            return None;
        }
        Some((width as u32, height as u32))
    }
}

/// `C:\Program Files\Vault\Vault.exe` -> `vault.exe`: the id
/// [`crate::focus`] gives a Windows app. Empty when the path has no
/// file name. Handles both separators so it can be tested off
/// Windows.
fn exe_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or_default().trim().to_lowercase()
}

/// One top-level window that is on screen (visible, not minimised,
/// not cloaked), as much of it as could be read.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SeenWindow {
    /// The owning process. 0 when it could not be read.
    pid: u32,
    /// Exe file name of the owner, lower case. `None` when the
    /// process could not be asked, or when nobody asked (a selection
    /// by pid does not need names).
    exe: Option<String>,
    /// `None` when neither DWM nor `GetWindowRect` gave a rectangle.
    edges: Option<Edges>,
}

/// The rectangles of the windows `apps` selects, and the rule for
/// what cannot be read:
///
/// - an empty selection selects nothing, whatever is on screen;
/// - a window whose owner cannot be identified (no exe name for a
///   selection by id, no pid for one by pid) might be a selected one,
///   so the whole answer is an `Err`;
/// - a selected window without a rectangle cannot be painted: `Err`;
/// - a selected window with no area contributes nothing.
fn rects_of_selected(windows: &[SeenWindow], apps: AppSelector<'_>) -> Result<Vec<PointRect>, String> {
    let nothing_selected = match apps {
        AppSelector::Ids(ids) => ids.is_empty(),
        AppSelector::Pids(pids) => pids.is_empty(),
    };
    if nothing_selected {
        return Ok(Vec::new());
    }
    let mut rects = Vec::new();
    for window in windows {
        let selected = match apps {
            AppSelector::Ids(ids) => {
                let exe = window.exe.as_deref().filter(|exe| !exe.trim().is_empty());
                id_listed(exe.ok_or_else(|| UNKNOWN_OWNER.to_string())?, ids)
            }
            AppSelector::Pids(pids) => {
                if window.pid == 0 {
                    return Err(UNKNOWN_OWNER.to_string());
                }
                pids.contains(&window.pid)
            }
        };
        if !selected {
            continue;
        }
        let edges = window.edges.ok_or_else(|| UNMEASURED.to_string())?;
        if let Some(rect) = edges.to_rect() {
            rects.push(rect);
        }
    }
    Ok(rects)
}

/// GDI hands a 32-bit copy over as blue, green, red and a fourth byte
/// that means nothing for a screen. The trait wants red, green, blue
/// and alpha 255.
fn bgra_to_rgba(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
}

/// The app in front out of a focus read ([`crate::focus::current_app`]).
/// An app that is not known must mean no screenshot, so a read that
/// came back without a process (no foreground window) or without an
/// exe name (a process that may not be asked) is an `Err`, not an app
/// with an empty id that matches no list entry.
fn front_app(id: &str, name: &str, pid: u32) -> Result<FrontApp, String> {
    if pid == 0 {
        return Err("no window is in front, or its program could not be read".to_string());
    }
    let id = id.trim();
    if id.is_empty() {
        return Err("the program in front could not be identified".to_string());
    }
    Ok(FrontApp { id: id.to_string(), name: name.trim().to_string(), pid })
}

#[cfg(target_os = "windows")]
pub(super) struct Windows;

#[cfg(target_os = "windows")]
impl Platform for Windows {
    fn permission(&self) -> PermissionState {
        // Windows has no grant for copying the screen.
        PermissionState::Granted
    }

    fn frontmost_apps(&self) -> Result<Vec<FrontApp>, String> {
        // The same read, ids and names as everywhere else in the app.
        let focus = crate::focus::current_app();
        Ok(vec![front_app(&focus.app_id, &focus.app_name, focus.pid)?])
    }

    fn window_rects(&self, apps: AppSelector<'_>) -> Result<Vec<PointRect>, String> {
        // Exe names are only read when they are compared: a selection
        // by pid, or an empty list, opens no process.
        let with_names = matches!(apps, AppSelector::Ids(ids) if !ids.is_empty());
        let windows = sys::windows_on_screen(with_names)?;
        rects_of_selected(&windows, apps)
    }

    fn capture(&self, _exclude: AppSelector<'_>, at: Option<(f64, f64)>) -> Result<RawCapture, PlatformError> {
        // `_exclude`: nothing can be left out here (module docs). The
        // caller paints `window_rects` instead.
        sys::capture(at)
    }
}

/// Everything that calls Win32.
#[cfg(target_os = "windows")]
mod sys {
    use std::collections::HashMap;
    use std::ffi::c_void;

    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT};
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, GetMonitorInfoW, HBITMAP, HDC, HGDIOBJ,
        MONITOR_DEFAULTTONULL, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint, MonitorFromWindow, ReleaseDC,
        SRCCOPY, SelectObject,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        DPI_AWARENESS_PER_MONITOR_AWARE, GetAwarenessFromDpiAwarenessContext, GetThreadDpiAwarenessContext,
        SetThreadDpiAwarenessContext,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
    };
    use windows::core::{BOOL, PWSTR};

    use super::{Edges, PlatformError, RawCapture, SeenWindow, bgra_to_rgba, exe_name};

    /// Makes the calling thread per-monitor DPI aware until dropped,
    /// so that every coordinate read meanwhile is in real pixels (see
    /// the module docs).
    pub(super) struct PerMonitorDpi {
        /// What the thread was before. Null when nothing was changed.
        previous: DPI_AWARENESS_CONTEXT,
    }

    impl PerMonitorDpi {
        pub(super) fn enter() -> Result<Self, String> {
            // SAFETY: these calls take and return plain values and
            // only change a setting of the calling thread.
            unsafe {
                // v2 exists from Windows 10 1703, v1 from 1607. A null
                // answer means the system did not take the value.
                let mut previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
                if previous.0.is_null() {
                    previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE);
                }
                // From here the guard puts the old setting back,
                // also on the refusal below.
                let guard = Self { previous };
                // Believe the result, not the call: what counts is
                // what the thread is now.
                if GetAwarenessFromDpiAwarenessContext(GetThreadDpiAwarenessContext())
                    != DPI_AWARENESS_PER_MONITOR_AWARE
                {
                    return Err("the screen's real pixel coordinates could not be read".to_string());
                }
                Ok(guard)
            }
        }
    }

    impl Drop for PerMonitorDpi {
        fn drop(&mut self) {
            if !self.previous.0.is_null() {
                // SAFETY: a context this thread had a moment ago.
                unsafe { SetThreadDpiAwarenessContext(self.previous) };
            }
        }
    }

    /// `EnumWindows` callback: note every top-level window. `lparam`
    /// points at a `Vec<HWND>`. Nothing else happens in here; the
    /// windows are looked at after the enumeration.
    unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is the `Vec<HWND>` passed by
        // `windows_on_screen`, alive for the whole enumeration and not
        // touched by anyone else.
        unsafe { (*(lparam.0 as *mut Vec<HWND>)).push(hwnd) };
        // Keep going.
        BOOL(1)
    }

    /// Every top-level window that is on screen, on any monitor. With
    /// `with_names`, the exe name of each owner is read too (once per
    /// process). `Err` when the windows cannot be listed or the thread
    /// cannot be made DPI aware.
    pub(super) fn windows_on_screen(with_names: bool) -> Result<Vec<SeenWindow>, String> {
        let _dpi = PerMonitorDpi::enter()?;
        let mut handles: Vec<HWND> = Vec::new();
        // SAFETY: `handles` outlives the call, and the callback is the
        // only user of the pointer while `EnumWindows` runs.
        unsafe { EnumWindows(Some(collect_window), LPARAM(&mut handles as *mut Vec<HWND> as isize)) }
            .map_err(|_| "the window list could not be read".to_string())?;

        let mut names: HashMap<u32, Option<String>> = HashMap::new();
        let mut seen = Vec::new();
        for hwnd in handles {
            // SAFETY: Win32 reads about a window handle. A handle
            // whose window has closed makes them fail, nothing more.
            unsafe {
                if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() || is_cloaked(hwnd) {
                    continue;
                }
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                let exe = if with_names && pid != 0 {
                    names
                        .entry(pid)
                        .or_insert_with(|| exe_path(pid).map(|path| exe_name(&path)).filter(|name| !name.is_empty()))
                        .clone()
                } else {
                    None
                };
                let edges = frame_edges(hwnd);
                // Something about it could not be read. If that is
                // because the window closed meanwhile, it is not on
                // screen any more and has no say.
                let unread = pid == 0 || (with_names && exe.is_none()) || edges.is_none();
                if unread && !IsWindow(Some(hwnd)).as_bool() {
                    continue;
                }
                seen.push(SeenWindow { pid, exe, edges });
            }
        }
        Ok(seen)
    }

    /// Whether the window manager keeps the window off the screen
    /// although it is "visible". A flag that cannot be read counts as
    /// not cloaked: the window stays in the list, which errs towards
    /// painting.
    unsafe fn is_cloaked(hwnd: HWND) -> bool {
        let mut cloaked = 0u32;
        // SAFETY: the out pointer is a `u32` of the size passed.
        let read = unsafe {
            DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, size_of::<u32>() as u32)
        };
        read.is_ok() && cloaked != 0
    }

    /// The window's frame as a person sees it; `GetWindowRect` when
    /// DWM gives nothing or an empty frame. `None` when both fail.
    unsafe fn frame_edges(hwnd: HWND) -> Option<Edges> {
        let edges = |r: RECT| Edges { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
        let mut frame = RECT::default();
        // SAFETY: the out pointers are `RECT`s of the size passed.
        unsafe {
            let from_dwm = DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut frame as *mut RECT as *mut c_void,
                size_of::<RECT>() as u32,
            );
            if from_dwm.is_ok() && edges(frame).to_rect().is_some() {
                return Some(edges(frame));
            }
            let mut outer = RECT::default();
            GetWindowRect(hwnd, &mut outer).ok()?;
            Some(edges(outer))
        }
    }

    /// Full path of the process's exe. `None` for a process this one
    /// may not ask, or one that has exited. (The same read as in
    /// `focus/windows.rs`, which keeps its own private.)
    unsafe fn exe_path(pid: u32) -> Option<String> {
        // SAFETY: a buffer we own, and the handle opened here is
        // closed before returning.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let result = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
            let _ = CloseHandle(handle);
            result.ok()?;
            Some(String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]))
        }
    }

    /// The screen's device context, given back when dropped.
    struct ScreenDc(HDC);

    impl Drop for ScreenDc {
        fn drop(&mut self) {
            // SAFETY: a context from `GetDC(None)`, released once.
            unsafe { ReleaseDC(None, self.0) };
        }
    }

    /// A memory device context, deleted when dropped.
    struct MemoryDc(HDC);

    impl Drop for MemoryDc {
        fn drop(&mut self) {
            // SAFETY: a context from `CreateCompatibleDC`, deleted
            // once.
            unsafe {
                let _ = DeleteDC(self.0);
            }
        }
    }

    /// A bitmap, deleted when dropped. It holds a copy of the screen,
    /// so it must not outlive the capture on any path.
    struct Bitmap(HBITMAP);

    impl Drop for Bitmap {
        fn drop(&mut self) {
            // SAFETY: a bitmap from `CreateCompatibleBitmap`, deleted
            // once, after it has left the device context (`Selected`
            // is dropped first).
            unsafe {
                let _ = DeleteObject(self.0.into());
            }
        }
    }

    /// A bitmap selected into a device context; the context gets its
    /// old one back when this is dropped.
    struct Selected {
        dc: HDC,
        previous: HGDIOBJ,
    }

    impl Drop for Selected {
        fn drop(&mut self) {
            // SAFETY: the object the context held before.
            unsafe { SelectObject(self.dc, self.previous) };
        }
    }

    /// Copy one whole monitor. See [`super::Platform::capture`] for
    /// which one.
    pub(super) fn capture(at: Option<(f64, f64)>) -> Result<RawCapture, PlatformError> {
        let failed = |why: &str| PlatformError::Failed(why.to_string());
        let _dpi = PerMonitorDpi::enter().map_err(PlatformError::Failed)?;
        // SAFETY (this block): Win32 calls with values and buffers we
        // own. Every device context and bitmap made here sits in a
        // guard that releases it on every way out.
        unsafe {
            let monitor = match at {
                Some((x, y)) => {
                    let on_a_monitor = x.is_finite() && y.is_finite();
                    let point = POINT { x: x.floor() as i32, y: y.floor() as i32 };
                    let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONULL);
                    if !on_a_monitor || monitor.is_invalid() {
                        return Err(failed("there is no display at that point"));
                    }
                    monitor
                }
                // No foreground window is a null handle, and then
                // this is the primary monitor.
                None => MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY),
            };
            if monitor.is_invalid() {
                return Err(PlatformError::NoDisplay);
            }
            let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return Err(failed("the display could not be measured"));
            }
            let m = info.rcMonitor;
            let edges = Edges { left: m.left, top: m.top, right: m.right, bottom: m.bottom };
            let (Some((width, height)), Some(bounds)) = (edges.pixel_size(), edges.to_rect()) else {
                return Err(PlatformError::NoDisplay);
            };
            let (w, h) = (width as i32, height as i32);

            let screen = GetDC(None);
            if screen.is_invalid() {
                return Err(failed("the screen could not be opened"));
            }
            let screen = ScreenDc(screen);
            let memory = CreateCompatibleDC(Some(screen.0));
            if memory.is_invalid() {
                return Err(failed("no memory for the screen copy"));
            }
            let memory = MemoryDc(memory);
            let bitmap = CreateCompatibleBitmap(screen.0, w, h);
            if bitmap.is_invalid() {
                return Err(failed("no memory for the screen copy"));
            }
            let bitmap = Bitmap(bitmap);
            {
                let previous = SelectObject(memory.0, bitmap.0.into());
                if previous.is_invalid() {
                    return Err(failed("the screen copy could not be set up"));
                }
                let _selected = Selected { dc: memory.0, previous };
                // Fails on a locked screen and on the secure desktop
                // (a permission prompt): then there is no screenshot.
                BitBlt(memory.0, 0, 0, w, h, Some(screen.0), m.left, m.top, SRCCOPY | CAPTUREBLT)
                    .map_err(|_| failed("the screen could not be copied"))?;
                // The bitmap leaves the device context here, which
                // `GetDIBits` asks for.
            }

            // From here `raw` holds the screen. Every early return
            // drops it, which zeroes it.
            let mut raw = RawCapture {
                rgba: vec![0u8; width as usize * height as usize * 4],
                width,
                height,
                bounds,
                excluded_apps: 0,
            };
            // 32 bits, uncompressed, rows from the top (the negative
            // height): four bytes per pixel and no row padding.
            let mut format = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let rows = GetDIBits(
                screen.0,
                bitmap.0,
                0,
                height,
                Some(raw.rgba.as_mut_ptr() as *mut c_void),
                &mut format,
                DIB_RGB_COLORS,
            );
            if rows != h {
                return Err(failed("the screen copy came back short"));
            }
            bgra_to_rgba(&mut raw.rgba);
            Ok(raw)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edges(left: i32, top: i32, right: i32, bottom: i32) -> Edges {
        Edges { left, top, right, bottom }
    }

    fn window(pid: u32, exe: Option<&str>, at: Option<Edges>) -> SeenWindow {
        SeenWindow { pid, exe: exe.map(str::to_string), edges: at }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn edges_become_rectangles_in_the_same_space() {
        assert_eq!(
            edges(100, 50, 420, 290).to_rect(),
            Some(PointRect { x: 100.0, y: 50.0, width: 320.0, height: 240.0 })
        );
        // A monitor to the left of and above the primary one.
        assert_eq!(
            edges(-1920, -200, 0, 880).to_rect(),
            Some(PointRect { x: -1920.0, y: -200.0, width: 1920.0, height: 1080.0 })
        );
        // No area, or inside out: nothing on screen.
        assert_eq!(edges(10, 10, 10, 50).to_rect(), None);
        assert_eq!(edges(10, 10, 50, 10).to_rect(), None);
        assert_eq!(edges(50, 50, 10, 10).to_rect(), None);
        assert_eq!(edges(0, 0, 0, 0).to_rect(), None);
        // The widest rectangle 32 bits can say does not overflow.
        let all = edges(i32::MIN, i32::MIN, i32::MAX, i32::MAX).to_rect().unwrap();
        assert_eq!(all.width, u32::MAX as f64);
        assert!(all.is_finite());
    }

    #[test]
    fn a_monitor_has_a_size_a_monitor_can_have() {
        assert_eq!(edges(0, 0, 2560, 1440).pixel_size(), Some((2560, 1440)));
        assert_eq!(edges(-3840, 0, 0, 2160).pixel_size(), Some((3840, 2160)));
        assert_eq!(edges(0, 0, 32_768, 1).pixel_size(), Some((32_768, 1)));
        assert_eq!(edges(0, 0, 32_769, 1080).pixel_size(), None);
        assert_eq!(edges(0, 0, 0, 1080).pixel_size(), None);
        assert_eq!(edges(0, 0, 1920, -1).pixel_size(), None);
        assert_eq!(edges(i32::MIN, 0, i32::MAX, 1080).pixel_size(), None);
    }

    #[test]
    fn the_exe_name_is_the_file_name_in_lower_case() {
        assert_eq!(exe_name(r"C:\Program Files\Vault\Vault.EXE"), "vault.exe");
        assert_eq!(exe_name(r"C:\Users\me\AppData\Local\1Password\app\8\1Password.exe"), "1password.exe");
        assert_eq!(exe_name("C:/tools/KeePassXC.exe"), "keepassxc.exe");
        assert_eq!(exe_name("notepad.exe"), "notepad.exe");
        assert_eq!(exe_name(r"C:\dir\"), "");
        assert_eq!(exe_name(""), "");
    }

    #[test]
    fn listed_exe_names_select_their_windows_whatever_the_case() {
        let a = edges(0, 0, 800, 600);
        let b = edges(-500, 100, -100, 400);
        let windows = [
            window(10, Some("notepad.exe"), Some(edges(5, 5, 50, 50))),
            window(20, Some("1password.exe"), Some(a)),
            // A second window of the same app (a popup), on another monitor.
            window(20, Some("1password.exe"), Some(b)),
            window(30, Some("bitwarden.exe"), Some(edges(900, 0, 1000, 100))),
        ];
        let listed = ids(&["1Password.exe", "KeePassXC.exe"]);
        assert_eq!(
            rects_of_selected(&windows, AppSelector::Ids(&listed)),
            Ok(vec![a.to_rect().unwrap(), b.to_rect().unwrap()])
        );
        // An app that is not running contributes nothing.
        let not_running = ids(&["keepassxc.exe"]);
        assert_eq!(rects_of_selected(&windows, AppSelector::Ids(&not_running)), Ok(Vec::new()));
        // An entry without the `.exe` ending still covers the exe
        // (the rule that lets an id cover what extends it with a
        // dot), which errs towards painting.
        let stem = ids(&["1password"]);
        assert_eq!(rects_of_selected(&windows, AppSelector::Ids(&stem)).map(|r| r.len()), Ok(2));
        // The start of a name is not the name.
        let start = ids(&["1pass", "password.exe"]);
        assert_eq!(rects_of_selected(&windows, AppSelector::Ids(&start)), Ok(Vec::new()));
    }

    #[test]
    fn a_window_without_a_known_owner_means_no_answer() {
        let listed = ids(&["1password.exe"]);
        let unknown = [
            window(10, Some("notepad.exe"), Some(edges(0, 0, 10, 10))),
            window(77, None, Some(edges(0, 0, 800, 600))),
        ];
        assert_eq!(rects_of_selected(&unknown, AppSelector::Ids(&listed)), Err(UNKNOWN_OWNER.to_string()));
        // A name that is there but empty is no name.
        let blank = [window(77, Some(" "), Some(edges(0, 0, 800, 600)))];
        assert_eq!(rects_of_selected(&blank, AppSelector::Ids(&listed)), Err(UNKNOWN_OWNER.to_string()));
        // Also when a listed window was found before the unknown one:
        // never half an answer.
        let after = [
            window(20, Some("1password.exe"), Some(edges(0, 0, 10, 10))),
            window(77, None, Some(edges(0, 0, 800, 600))),
        ];
        assert_eq!(rects_of_selected(&after, AppSelector::Ids(&listed)), Err(UNKNOWN_OWNER.to_string()));
        // With nothing on the list there is nothing to protect, and
        // an unknown owner costs nothing.
        assert_eq!(rects_of_selected(&unknown, AppSelector::Ids(&[])), Ok(Vec::new()));
        assert_eq!(rects_of_selected(&unknown, AppSelector::Pids(&[])), Ok(Vec::new()));
    }

    #[test]
    fn a_selected_window_must_have_a_rectangle() {
        let listed = ids(&["1password.exe"]);
        let unmeasured = [window(20, Some("1password.exe"), None)];
        assert_eq!(rects_of_selected(&unmeasured, AppSelector::Ids(&listed)), Err(UNMEASURED.to_string()));
        assert_eq!(rects_of_selected(&unmeasured, AppSelector::Pids(&[20])), Err(UNMEASURED.to_string()));
        // Another app's window that could not be measured does not matter.
        let other = [
            window(10, Some("notepad.exe"), None),
            window(20, Some("1password.exe"), Some(edges(0, 0, 10, 10))),
        ];
        assert_eq!(rects_of_selected(&other, AppSelector::Ids(&listed)).map(|r| r.len()), Ok(1));
        // A selected window with no area has nothing to paint.
        let empty = [window(20, Some("1password.exe"), Some(edges(40, 40, 40, 40)))];
        assert_eq!(rects_of_selected(&empty, AppSelector::Ids(&listed)), Ok(Vec::new()));
    }

    #[test]
    fn processes_select_their_windows_without_names() {
        let a = edges(10, 20, 110, 220);
        let windows = [window(10, None, Some(edges(0, 0, 5, 5))), window(20, None, Some(a)), window(30, None, None)];
        assert_eq!(rects_of_selected(&windows, AppSelector::Pids(&[20, 99])), Ok(vec![a.to_rect().unwrap()]));
        assert_eq!(rects_of_selected(&windows, AppSelector::Pids(&[99])), Ok(Vec::new()));
        // A window whose process could not be read might be a selected one.
        let unknown = [window(20, None, Some(a)), window(0, None, Some(a))];
        assert_eq!(rects_of_selected(&unknown, AppSelector::Pids(&[20])), Err(UNKNOWN_OWNER.to_string()));
    }

    #[test]
    fn the_screen_copy_becomes_rgba_with_full_alpha() {
        // Blue, green, red, and a fourth byte GDI leaves at anything.
        let mut pixels = vec![1, 2, 3, 0, 250, 128, 7, 99];
        bgra_to_rgba(&mut pixels);
        assert_eq!(pixels, vec![3, 2, 1, 255, 7, 128, 250, 255]);
        let mut none: Vec<u8> = Vec::new();
        bgra_to_rgba(&mut none);
        assert!(none.is_empty());
    }

    #[test]
    fn the_app_in_front_is_known_or_there_is_no_answer() {
        assert_eq!(
            front_app(" slack.exe ", " Slack ", 4242),
            Ok(FrontApp { id: "slack.exe".into(), name: "Slack".into(), pid: 4242 })
        );
        // A name is nice to have; the id is what the list is checked against.
        assert_eq!(front_app("vault.exe", "", 7).map(|app| app.id), Ok("vault.exe".to_string()));
        // No foreground window.
        assert!(front_app("", "", 0).is_err());
        // A process that may not be asked for its exe.
        assert!(front_app("", "", 4242).is_err());
        assert!(front_app("  ", "Something", 4242).is_err());
    }
}

/// For the first run on a real Windows machine; none of this has run
/// where it was written. Nothing here keeps or prints a pixel.
#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
    };
    use windows::core::{PCWSTR, w};

    use super::*;

    /// A plain window of this process, closed when dropped (also when
    /// an assertion fails).
    struct TestWindow(HWND);

    impl Drop for TestWindow {
        fn drop(&mut self) {
            // SAFETY: a window this thread created.
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }

    #[test]
    fn nothing_needs_granting_and_the_app_in_front_is_never_an_empty_list() {
        assert_eq!(Windows.permission(), PermissionState::Granted);
        // A session without a foreground window (a build agent) has
        // no app in front; that is an `Err`, never an empty list.
        match Windows.frontmost_apps() {
            Ok(front) => {
                assert!(!front.is_empty());
                assert!(front.iter().all(|app| app.pid != 0 && !app.id.is_empty()));
                assert!(front.iter().all(|app| app.id == app.id.to_lowercase()));
            }
            Err(why) => assert!(!why.is_empty()),
        }
    }

    #[test]
    fn an_empty_selection_selects_nothing() {
        assert_eq!(Windows.window_rects(AppSelector::Ids(&[])), Ok(Vec::new()));
        assert_eq!(Windows.window_rects(AppSelector::Pids(&[])), Ok(Vec::new()));
    }

    #[test]
    fn a_window_of_this_process_is_found_where_it_is() {
        // Created while the thread is per-monitor aware, so the
        // numbers given here are real pixels, like the ones read back.
        let dpi = sys::PerMonitorDpi::enter().expect("per-monitor DPI awareness");
        // SAFETY: a window of a system class, with no parent, menu or
        // extra data.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("STATIC"),
                PCWSTR::null(),
                WS_POPUP | WS_VISIBLE,
                120,
                90,
                300,
                200,
                None,
                None,
                None,
                None,
            )
        }
        .expect("a test window");
        let _window = TestWindow(hwnd);
        drop(dpi);
        let expected = PointRect { x: 120.0, y: 90.0, width: 300.0, height: 200.0 };

        let own = [std::process::id()];
        let rects = Windows.window_rects(AppSelector::Pids(&own)).expect("window rectangles by pid");
        assert!(rects.contains(&expected), "{rects:?}");

        // The same window by exe name. On a machine with a window
        // whose owner cannot be identified this is the documented
        // refusal instead.
        let exe = std::env::current_exe().expect("own exe");
        let name = vec![exe_name(&exe.to_string_lossy())];
        match Windows.window_rects(AppSelector::Ids(&name)) {
            Ok(rects) => assert!(rects.contains(&expected), "{rects:?}"),
            Err(why) => assert_eq!(why, UNKNOWN_OWNER),
        }

        // A name nobody has selects nothing.
        let nobody = vec!["openwhisper-test-not-installed.exe".to_string()];
        if let Ok(rects) = Windows.window_rects(AppSelector::Ids(&nobody)) {
            assert!(rects.is_empty());
        }
    }

    /// A real capture, for a person to run on a desktop session:
    /// `OPENWHISPER_VISION_LIVE=1 cargo test -p openwhisper-core
    /// --no-default-features --features tauri --lib vision::windows -- --ignored`.
    /// It asserts on sizes and the alpha channel only and keeps
    /// nothing.
    #[test]
    #[ignore = "captures the real screen; run by hand with OPENWHISPER_VISION_LIVE=1"]
    fn live_capture_is_one_monitor_at_its_pixel_size() {
        if std::env::var("OPENWHISPER_VISION_LIVE").as_deref() != Ok("1") {
            return;
        }
        let raw = match Windows.capture(AppSelector::Ids(&[]), None) {
            Ok(raw) => raw,
            Err(PlatformError::NoDisplay) => return,
            Err(PlatformError::Failed(why)) => panic!("capture failed: {why}"),
        };
        assert_eq!(raw.rgba.len(), raw.width as usize * raw.height as usize * 4);
        // Real pixels: the monitor's rectangle is as large as the image.
        assert_eq!((raw.bounds.width, raw.bounds.height), (raw.width as f64, raw.height as f64));
        assert_eq!(raw.excluded_apps, 0);
        assert!(raw.rgba.chunks_exact(4).all(|pixel| pixel[3] == 255));

        // The same monitor again when asked for by a point on it.
        let (x, y) = raw.bounds.centre();
        let again = Windows.capture(AppSelector::Ids(&[]), Some((x, y))).expect("capture at a point");
        assert_eq!(again.bounds, raw.bounds);
        assert_eq!((again.width, again.height), (raw.width, raw.height));
        // And no monitor is that far out.
        assert!(Windows.capture(AppSelector::Ids(&[]), Some((-1.0e7, -1.0e7))).is_err());
    }
}
