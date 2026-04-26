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
//! AX gating is done by the caller via `hotkey::hotkey_status_current()`
//! rather than `AXIsProcessTrusted`. The TCC flag false-negatives in dev
//! because ad-hoc cdhash drift invalidates TCC's trusted record on every
//! rebuild (project_tcc_dev_pain), but a successful CGEventTap install
//! proves AX is operationally trusted regardless of TCC bookkeeping —
//! which is the signal that actually matters.

use block2::RcBlock;
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use tauri::AppHandle;

use super::emit_status;

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
                "Microphone access denied. Grant it in System Settings → Privacy & Security → Microphone, then reopen {app_name}."
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

pub fn request_microphone(app: &AppHandle) {
    let hotkey_ok = crate::hotkey::hotkey_status_current()
        .map(|s| s.ok)
        .unwrap_or(false);
    if !hotkey_ok {
        // AX not yet operationally trusted — don't probe AVFoundation
        // (would race the user's first AX grant). UI banner stays clear
        // until the next boot via the hotkey-watchdog → mic-prompt flow.
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
