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

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::boolean::{CFBoolean, CFBooleanRef};
use core_foundation::string::CFString;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: core_foundation::string::CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
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
