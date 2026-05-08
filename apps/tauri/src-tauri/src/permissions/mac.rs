//! macOS mic permission prompt via AVFoundation. Direct port of
//! `apps/macos/App/PermissionsCoordinator.swift::promptMicrophone`.
//!
//! Calls `+[AVCaptureDevice authorizationStatusForMediaType:]` to read
//! the current state and `+[AVCaptureDevice
//! requestAccessForMediaType:completionHandler:]` to fire the system
//! dialog when status is `AVAuthorizationStatusNotDetermined`. The
//! completion block re-emits status so the UI banner clears (or
//! appears, on denial) the moment the user picks an option.
//!
//! AX gating: in release builds, gate on `AXIsProcessTrusted()` directly
//! — the OS truth. In dev, also accept `hotkey::hotkey_status_current()`
//! as a fallback because ad-hoc cdhash drift false-negatives `AXIsProcessTrusted`
//! on every rebuild (project_tcc_dev_pain), and a successful CGEventTap
//! install proves AX is operationally trusted there. Release fresh-install
//! flow MUST never fire the AVCapture mic dialog before the user has
//! granted Accessibility — both prompts firing near-simultaneously made the
//! mic dialog steal focus and read as "mic before AX".

use block2::RcBlock;
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use tauri::AppHandle;

use super::emit_status;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

// AVAuthorizationStatus values.
const AV_NOT_DETERMINED: i64 = 0;
const AV_RESTRICTED: i64 = 1;
const AV_DENIED: i64 = 2;
const AV_AUTHORIZED: i64 = 3;

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {
    /// `AVMediaTypeAudio` from AVFoundation — exported as an `NSString *`.
    static AVMediaTypeAudio: *const AnyObject;
}

/// Probe + emit. Folds AVAuthorizationStatus into the UI-facing
/// `(mic_ok, mic_state, error)` tuple.
fn emit_for_status(app: &AppHandle, status: i64) {
    let app_name = crate::product_name(app);
    match status {
        AV_AUTHORIZED => emit_status(app, true, "authorized", ""),
        // NotDetermined: the user hasn't been asked yet. Treat as ok so
        // the banner doesn't flash before the system dialog appears.
        AV_NOT_DETERMINED => emit_status(app, true, "not_determined", ""),
        AV_DENIED => emit_status(
            app,
            false,
            "denied",
            format!(
                "Microphone access denied. Grant it in System Settings → Privacy & Security → Microphone."
            ),
        ),
        AV_RESTRICTED => emit_status(
            app,
            false,
            "restricted",
            format!(
                "Microphone access is restricted (parental controls or MDM). {app_name} can't record on this device."
            ),
        ),
        _ => emit_status(app, false, "unknown", format!("Unknown mic auth status: {status}")),
    }
}

/// Side-effect-free re-probe — reads the current AVCaptureDevice
/// authorization status and re-emits via `emit_status`. No prompt is
/// fired (unlike `request_microphone`, which prompts on
/// `NotDetermined`). Used on app focus regain so the mic banner clears
/// the instant the user grants in System Settings, without requiring
/// an app relaunch.
pub fn recheck(app: &AppHandle) {
    unsafe {
        let Some(cls) = AnyClass::get(c"AVCaptureDevice") else {
            return;
        };
        let media: *const AnyObject = AVMediaTypeAudio;
        if media.is_null() {
            return;
        }
        let status: i64 = msg_send![cls, authorizationStatusForMediaType: media];
        emit_for_status(app, status);
    }
}

pub fn request_microphone(app: &AppHandle) {
    let ax_trusted = unsafe { AXIsProcessTrusted() };
    let hotkey_ok = crate::hotkey::hotkey_status_current()
        .map(|s| s.ok)
        .unwrap_or(false);
    let proceed = if cfg!(debug_assertions) {
        ax_trusted || hotkey_ok
    } else {
        ax_trusted
    };
    if !proceed {
        // AX not yet OS-trusted — don't probe AVFoundation (would race
        // the user's AX grant and steal focus from the System Settings
        // window). The AX-grant watcher in `focus.rs` re-fires this on
        // the false → true edge, so the mic prompt lands the moment AX
        // is granted without depending on a relaunch.
        return;
    }

    unsafe {
        let Some(cls) = AnyClass::get(c"AVCaptureDevice") else {
            return;
        };

        let media: *const AnyObject = AVMediaTypeAudio;
        if media.is_null() {
            return;
        }

        let status: i64 = msg_send![cls, authorizationStatusForMediaType: media];

        // Always emit the current state so the UI sees mic-denied even
        // when there's nothing to prompt.
        emit_for_status(app, status);

        if status != AV_NOT_DETERMINED {
            return;
        }

        // Status is NotDetermined → the prompt is about to fire. Bring
        // main forward so the user sees OW's chrome behind the system
        // mic dialog.
        crate::focus::bring_main_to_front(app);

        let app_for_block = app.clone();
        let block = RcBlock::new(move |_granted: Bool| {
            // Re-probe after the user's choice — `granted` is the bool the
            // dialog returned, but re-probing keeps the UI mapping in
            // one place (denied → denied banner, authorized → cleared).
            let cls = match AnyClass::get(c"AVCaptureDevice") {
                Some(c) => c,
                None => return,
            };
            let media: *const AnyObject = AVMediaTypeAudio;
            if media.is_null() {
                return;
            }
            let new_status: i64 = msg_send![cls, authorizationStatusForMediaType: media];
            emit_for_status(&app_for_block, new_status);
        });

        let _: () = msg_send![
            cls,
            requestAccessForMediaType: media,
            completionHandler: &*block,
        ];

        // AVFoundation captures the block via Block_copy for the async
        // callback. Forget our RcBlock so it outlives this scope just in
        // case the runtime's retain semantics ever drift — leaking one
        // empty closure on app boot is invisible.
        std::mem::forget(block);
    }
}
