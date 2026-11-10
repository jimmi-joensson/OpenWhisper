//! macOS focus read.
//!
//! App identity comes from `NSWorkspace.frontmostApplication`, which
//! needs no permission. The window title needs the Accessibility API
//! (`AXFocusedWindow`, then `AXTitle`), which only answers once the
//! process has the Accessibility grant; without it the title stays
//! empty.

use std::ffi::c_void;

use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};

use super::FocusContext;

type AXUIElementRef = *const c_void;

/// `kAXErrorSuccess`.
const AX_SUCCESS: i32 = 0;

/// How long one Accessibility request may wait for the other app.
/// The default is 6 seconds, far too long for the dictation path.
const AX_TIMEOUT_SECONDS: f32 = 0.25;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout_seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
}

pub(super) fn read(with_title: bool) -> FocusContext {
    // The pool matters off the main thread: the dictation pipeline
    // calls this from a worker that has no pool of its own.
    let mut ctx = autoreleasepool(|_| {
        let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
            return FocusContext::default();
        };
        FocusContext {
            app_id: app.bundleIdentifier().map(|s| s.to_string()).unwrap_or_default(),
            app_name: app.localizedName().map(|s| s.to_string()).unwrap_or_default(),
            pid: u32::try_from(app.processIdentifier()).unwrap_or(0),
            ..FocusContext::default()
        }
    });
    if with_title && ctx.pid != 0 {
        ctx.window_title = window_title(ctx.pid as i32).unwrap_or_default();
    }
    ctx
}

/// The running apps a person would call apps: regular activation
/// policy (a Dock icon; no agents, no helpers), with a bundle id, and
/// not this process. Needs no permission.
pub(super) fn running() -> Vec<(String, String)> {
    let own_pid = std::process::id();
    autoreleasepool(|_| {
        let apps = NSWorkspace::sharedWorkspace().runningApplications();
        let mut out = Vec::new();
        for app in apps.iter() {
            if app.activationPolicy() != NSApplicationActivationPolicy::Regular {
                continue;
            }
            if u32::try_from(app.processIdentifier()).ok() == Some(own_pid) {
                continue;
            }
            let Some(id) = app.bundleIdentifier().map(|s| s.to_string()).filter(|s| !s.trim().is_empty()) else {
                continue;
            };
            let name = app.localizedName().map(|s| s.to_string()).unwrap_or_default();
            out.push((id, name));
        }
        out
    })
}

/// Title of the app's focused window, when Accessibility is granted
/// and the app answers in time.
fn window_title(pid: i32) -> Option<String> {
    // SAFETY: plain C calls. Every element and value returned by a
    // Create/Copy function is wrapped at once so it is released.
    unsafe {
        if !AXIsProcessTrusted() {
            return None;
        }
        let app = AXUIElementCreateApplication(pid);
        if app.is_null() {
            return None;
        }
        let app = CFType::wrap_under_create_rule(app as CFTypeRef);
        AXUIElementSetMessagingTimeout(app.as_CFTypeRef() as AXUIElementRef, AX_TIMEOUT_SECONDS);
        let window = copy_attribute(&app, "AXFocusedWindow")?;
        let title = copy_attribute(&window, "AXTitle")?;
        title.downcast::<CFString>().map(|s| s.to_string())
    }
}

unsafe fn copy_attribute(element: &CFType, name: &'static str) -> Option<CFType> {
    let attribute = CFString::from_static_string(name);
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: `element` is a live AXUIElement, `value` is a valid out
    // pointer, and a returned value follows the Copy rule.
    let err = unsafe {
        AXUIElementCopyAttributeValue(
            element.as_CFTypeRef() as AXUIElementRef,
            attribute.as_concrete_TypeRef(),
            &mut value,
        )
    };
    if err != AX_SUCCESS || value.is_null() {
        return None;
    }
    Some(unsafe { CFType::wrap_under_create_rule(value) })
}
