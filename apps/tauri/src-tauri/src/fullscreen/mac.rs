//! macOS fullscreen detection via the Accessibility framework, plus
//! cursor-monitor tracking for the pill-follow signal.
//!
//! `kAXFullScreenAttribute` on the focused window (reached via the system
//! AX element) returns true when the active app is in macOS-native
//! fullscreen (NSWindow's `toggleFullScreen:` flow). Borderless-fullscreen
//! games that just create a window covering the screen don't set this
//! attribute — same caveat the Mac shipped shell already accepts (the pill
//! hides under those via Spaces, not via geometry).
//!
//! AX is callable from any thread, so we run from the poller thread
//! directly. AX permission is already granted via `hotkey::install`
//! prompt.
//!
//! `cursor_monitor()` reads the cursor's current global-screen
//! coordinates via `CGEventCreate(null)` + `CGEventGetLocation`, then
//! locates the display whose `CGDisplayBounds` rect contains it. This
//! replaced an earlier AX-based "focused window centre" approach that
//! silently dropped Electron apps (Figma, Discord, VS Code) — those
//! apps don't reliably expose `kAXPositionAttribute` / `kAXSizeAttribute`.
//! The cursor-based approach matches Wispr Flow's pattern and works
//! uniformly across Cocoa, Electron, and fullscreen contexts. Display
//! enumeration uses `CGDisplay::active_displays()` which is callable
//! from any thread — `NSScreen.screens` would NOT be (main-thread only).

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::boolean::{CFBoolean, CFBooleanRef};
use core_foundation::string::CFString;
use core_graphics::display::CGDisplay;
use core_graphics::geometry::CGPoint;
use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use tauri::{AppHandle, Monitor};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: core_foundation::string::CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    // Cursor-location FFI. Passing null source = "use a fresh default
    // source" per Apple's CGEvent.h docs; CGEventGetLocation returns
    // the cursor's current global-screen coordinates as a CGPoint
    // (Quartz origin: primary display top-left = (0, 0), Y increases
    // downward — same space `CGDisplayBounds` returns).
    fn CGEventCreate(source: CFTypeRef) -> CFTypeRef;
    fn CGEventGetLocation(event: CFTypeRef) -> CGPoint;
}

const KAX_FOCUSED_APPLICATION: &str = "AXFocusedApplication";
const KAX_FOCUSED_WINDOW: &str = "AXFocusedWindow";
const KAX_FULL_SCREEN: &str = "AXFullScreen";

pub fn is_fullscreen_now() -> bool {
    unsafe {
        let sys = AXUIElementCreateSystemWide();
        if sys.is_null() {
            return false;
        }

        let app = copy_attr(sys, KAX_FOCUSED_APPLICATION);
        CFRelease(sys);
        let Some(app) = app else { return false };

        let win = copy_attr(app, KAX_FOCUSED_WINDOW);
        CFRelease(app);
        let Some(win) = win else { return false };

        let fs = copy_attr(win, KAX_FULL_SCREEN);
        CFRelease(win);

        match fs {
            Some(b) => {
                let cfb: CFBoolean = TCFType::wrap_under_create_rule(b as CFBooleanRef);
                bool::from(cfb)
            }
            None => false,
        }
    }
}

/// Wraps `AXUIElementCopyAttributeValue` returning a freshly-retained
/// CFTypeRef on success. Caller owns the ref count and must release.
unsafe fn copy_attr(element: CFTypeRef, attr: &str) -> Option<CFTypeRef> {
    let attr_str = CFString::new(attr);
    let mut value: CFTypeRef = std::ptr::null();
    let err =
        AXUIElementCopyAttributeValue(element, attr_str.as_concrete_TypeRef(), &mut value);
    if err == 0 && !value.is_null() {
        Some(value)
    } else {
        None
    }
}

/// Origin (top-left) of the display whose bounds contain the cursor's
/// current location, in Quartz screen coordinates (primary display's
/// top-left = `(0, 0)`, Y increases downward — same space
/// `CGDisplayBounds` returns). Returns `None` only on a CG event-create
/// or display-enumerate failure (extremely rare); the watcher's
/// "skip when None" behavior keeps the pill where it is.
pub fn cursor_monitor() -> Option<(i32, i32)> {
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            return None;
        }
        let pt = CGEventGetLocation(event);
        CFRelease(event);

        let displays = CGDisplay::active_displays().ok()?;
        for id in displays {
            let bounds = CGDisplay::new(id).bounds();
            let ox = bounds.origin.x;
            let oy = bounds.origin.y;
            let w = bounds.size.width;
            let h = bounds.size.height;
            if pt.x >= ox && pt.x < ox + w && pt.y >= oy && pt.y < oy + h {
                return Some((ox as i32, oy as i32));
            }
        }
        None
    }
}

/// Y coordinate (logical-points, Quartz top-left origin, Y-down) of
/// the bottom edge of the given monitor's work area — i.e. the top of
/// the Dock when the Dock is on this screen, or the screen's bottom
/// edge when not. Used by `place_pill` to anchor 24 px above the
/// Dock. Falls back to the screen's own bottom edge if NSScreen
/// enumeration can't match the given monitor.
///
/// MUST be called on the main thread: NSScreen is documented as
/// main-thread-only by Apple.
pub fn work_area_bottom_y(monitor: &Monitor) -> f64 {
    let scale = monitor.scale_factor();
    let mon_x_log = monitor.position().x as f64 / scale;
    let mon_y_log = monitor.position().y as f64 / scale;
    let mon_h_log = monitor.size().height as f64 / scale;
    // Fallback used in every "can't determine work area" branch — the
    // bottom edge of the monitor itself, which is what `place_pill`
    // historically anchored to (minus a fixed 80 px margin).
    let fallback = mon_y_log + mon_h_log;

    // SAFETY: contract — caller is on the main thread (place_pill is
    // dispatched via run_on_main_thread; the periodic refresh task
    // also dispatches there).
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let screens = NSScreen::screens(mtm);
    let Some(primary) = screens.firstObject() else {
        return fallback;
    };
    // Cocoa coordinate origin is the bottom-left of the primary
    // screen's frame; we use that height to convert between Cocoa
    // (Y-up) and Quartz (Y-down).
    let primary_height_cocoa = primary.frame().size.height;
    let mon_bottom_cocoa_y = primary_height_cocoa - (mon_y_log + mon_h_log);

    for screen in screens.iter() {
        let frame = screen.frame();
        if (frame.origin.x - mon_x_log).abs() < 1.0
            && (frame.origin.y - mon_bottom_cocoa_y).abs() < 1.0
        {
            // visibleFrame.origin.y in Cocoa is the Y of the *bottom*
            // edge of the work area (Cocoa is Y-up; the work area's
            // origin is at its bottom-left). Converting that to Quartz
            // gives the Y of where the Dock starts (or the screen
            // bottom when no Dock is on this screen).
            let vf = screen.visibleFrame();
            return primary_height_cocoa - vf.origin.y;
        }
    }
    fallback
}

/// Convert the watcher's origin tuple (logical points from
/// `CGDisplayBounds.origin`) into the `tauri::Monitor` whose position
/// matches. Tauri's `Monitor::position()` is physical px, so we
/// convert the Tauri side to logical points by `/ scale_factor` and
/// round — same form the watcher emits. Returns `None` if no monitor
/// matches (e.g. display was unplugged between watcher tick and this
/// call); the caller then falls back to `pill.current_monitor()`.
///
/// MUST be called on the main thread: Tauri's `available_monitors()`
/// may go through `NSScreen.screens` internally, which is
/// main-thread-only.
pub fn find_tauri_monitor(app: &AppHandle, origin: (i32, i32)) -> Option<Monitor> {
    let monitors = app.available_monitors().ok()?;
    for m in monitors {
        let scale = m.scale_factor();
        let mx = (m.position().x as f64 / scale).round() as i32;
        let my = (m.position().y as f64 / scale).round() as i32;
        if (mx, my) == origin {
            return Some(m);
        }
    }
    None
}
