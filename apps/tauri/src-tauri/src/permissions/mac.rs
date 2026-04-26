//! macOS mic permission prompt via AVFoundation. Direct port of
//! `apps/macos/App/PermissionsCoordinator.swift::promptMicrophone`.
//!
//! Calls `+[AVCaptureDevice authorizationStatusForMediaType:]` to read
//! the current state and `+[AVCaptureDevice
//! requestAccessForMediaType:completionHandler:]` to fire the system
//! dialog when status is `AVAuthorizationStatusNotDetermined`. The
//! completion block is a no-op — cpal queries the latest state on the
//! next `audio_start_capture`, so we don't need to act on the result.
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

/// Mirrors `AVAuthorizationStatusNotDetermined`. The only state where
/// `requestAccessForMediaType:` does anything user-visible — restricted,
/// denied, and authorized all return immediately without dialog.
const AV_NOT_DETERMINED: i64 = 0;

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {
    /// `AVMediaTypeAudio` from AVFoundation — exported as an `NSString *`.
    static AVMediaTypeAudio: *const AnyObject;
}

pub fn request_microphone() {
    let hotkey_ok = crate::hotkey::hotkey_status_current()
        .map(|s| s.ok)
        .unwrap_or(false);
    if !hotkey_ok {
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
        if status != AV_NOT_DETERMINED {
            return;
        }

        let block = RcBlock::new(|_granted: Bool| {});

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
