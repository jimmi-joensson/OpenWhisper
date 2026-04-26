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
//! AX gating matches Swift: don't prompt mic until AX is trusted, so
//! the user never sees both dialogs at once.

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

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

pub fn request_microphone() {
    unsafe {
        if !AXIsProcessTrusted() {
            // Match Swift sequencing — wait for AX before prompting mic.
            // User restarts after granting AX, then mic prompt fires here.
            return;
        }

        let cls = match AnyClass::get(c"AVCaptureDevice") {
            Some(c) => c,
            None => {
                eprintln!("permissions: AVCaptureDevice class missing");
                return;
            }
        };
        let media: *const AnyObject = AVMediaTypeAudio;

        let status: i64 = msg_send![cls, authorizationStatusForMediaType: media];
        if status != AV_NOT_DETERMINED {
            return;
        }

        let block = RcBlock::new(|_granted: Bool| {
            // No-op. New state is read by cpal on next start_capture.
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
