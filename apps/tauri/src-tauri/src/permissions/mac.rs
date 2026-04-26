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
        let trusted = AXIsProcessTrusted();
        eprintln!("permissions: AX trusted = {trusted}");
        if !trusted {
            // Match Swift sequencing — wait for AX before prompting mic.
            // User restarts after granting AX, then mic prompt fires here.
            return;
        }

        let Some(cls) = AnyClass::get(c"AVCaptureDevice") else {
            eprintln!("permissions: AVCaptureDevice class not loaded");
            return;
        };

        let media: *const AnyObject = AVMediaTypeAudio;
        if media.is_null() {
            eprintln!("permissions: AVMediaTypeAudio symbol is null");
            return;
        }

        let status: i64 = msg_send![cls, authorizationStatusForMediaType: media];
        eprintln!(
            "permissions: mic auth status = {status} \
             (0=NotDetermined, 1=Restricted, 2=Denied, 3=Authorized)"
        );
        if status != AV_NOT_DETERMINED {
            return;
        }

        let block = RcBlock::new(|granted: Bool| {
            eprintln!(
                "permissions: mic prompt completed, granted = {}",
                granted.as_bool()
            );
        });

        eprintln!("permissions: firing AVCaptureDevice.requestAccessForMediaType:audio");
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
