//! macOS fullscreen detection via the Accessibility framework.
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
//! `focused_window_monitor()` reuses the same AX walk down to the
//! focused window, then queries `kAXPositionAttribute` +
//! `kAXSizeAttribute` (both packed as `AXValue` boxes — extracted via
//! `AXValueGetValue`) and locates the display whose `CGDisplayBounds`
//! rect contains the window's centre. Display enumeration uses
//! `CGGetActiveDisplayList` (via the `core-graphics` crate's safe
//! `CGDisplay::active_displays()`) which is callable from any thread —
//! `NSScreen.screens` would NOT be (main-thread only).

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::boolean::{CFBoolean, CFBooleanRef};
use core_foundation::string::CFString;
use core_graphics::display::CGDisplay;
use core_graphics::geometry::{CGPoint, CGSize};
use tauri::{AppHandle, Monitor};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: core_foundation::string::CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXValueGetValue(
        value: CFTypeRef,
        the_type: u32,
        value_ptr: *mut std::ffi::c_void,
    ) -> u8;
}

const KAX_FOCUSED_APPLICATION: &str = "AXFocusedApplication";
const KAX_FOCUSED_WINDOW: &str = "AXFocusedWindow";
const KAX_FULL_SCREEN: &str = "AXFullScreen";
const KAX_POSITION: &str = "AXPosition";
const KAX_SIZE: &str = "AXSize";

// AXValueType constants from <ApplicationServices/AXValue.h>.
const KAX_VALUE_CG_POINT_TYPE: u32 = 1;
const KAX_VALUE_CG_SIZE_TYPE: u32 = 2;

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

/// Origin (top-left) of the display whose bounds contain the focused
/// window's centre, in Quartz screen coordinates (primary display's
/// top-left = `(0, 0)`, Y increases downward — same space
/// `CGDisplayBounds` returns).
///
/// Returns `None` when AX is denied, no app/window has focus, or the
/// position/size attributes can't be unpacked. Stable across ticks on a
/// fixed display arrangement; the watcher in `mod.rs` only fires when
/// the tuple changes.
pub fn focused_window_monitor() -> Option<(i32, i32)> {
    unsafe {
        let sys = AXUIElementCreateSystemWide();
        if sys.is_null() {
            return None;
        }
        let app = copy_attr(sys, KAX_FOCUSED_APPLICATION);
        CFRelease(sys);
        let app = app?;

        let win = copy_attr(app, KAX_FOCUSED_WINDOW);
        CFRelease(app);
        let win = win?;

        let pos_ref = copy_attr(win, KAX_POSITION);
        let size_ref = copy_attr(win, KAX_SIZE);
        CFRelease(win);

        // Both refs must be released regardless of which (if any) is
        // present — release on every branch.
        let (pos, size) = match (pos_ref, size_ref) {
            (Some(p), Some(s)) => {
                let mut pos = CGPoint::new(0.0, 0.0);
                let mut size = CGSize::new(0.0, 0.0);
                let ok = AXValueGetValue(
                    p,
                    KAX_VALUE_CG_POINT_TYPE,
                    &mut pos as *mut _ as *mut std::ffi::c_void,
                ) != 0
                    && AXValueGetValue(
                        s,
                        KAX_VALUE_CG_SIZE_TYPE,
                        &mut size as *mut _ as *mut std::ffi::c_void,
                    ) != 0;
                CFRelease(p);
                CFRelease(s);
                if !ok {
                    return None;
                }
                (pos, size)
            }
            (p, s) => {
                if let Some(p) = p {
                    CFRelease(p);
                }
                if let Some(s) = s {
                    CFRelease(s);
                }
                return None;
            }
        };

        let cx = pos.x + size.width / 2.0;
        let cy = pos.y + size.height / 2.0;

        let displays = CGDisplay::active_displays().ok()?;
        for id in displays {
            let bounds = CGDisplay::new(id).bounds();
            let ox = bounds.origin.x;
            let oy = bounds.origin.y;
            let w = bounds.size.width;
            let h = bounds.size.height;
            if cx >= ox && cx < ox + w && cy >= oy && cy < oy + h {
                return Some((ox as i32, oy as i32));
            }
        }
        None
    }
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
