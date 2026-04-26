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
//! AX gating is done by the caller (lib.rs), keyed on
//! `hotkey::hotkey_status_current()` rather than `AXIsProcessTrusted`.
//! The TCC flag false-negatives in dev because ad-hoc cdhash drift
//! invalidates TCC's trusted record on every rebuild
//! (project_tcc_dev_pain), but a successful CGEventTap install proves
//! AX is operationally trusted regardless of what TCC's bookkeeping
//! says — which is the signal that actually matters.

use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use block2::RcBlock;
use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject, Bool};

/// dev-run.sh launches the .app via `open`, which routes stderr to
/// /dev/null. eprintln is invisible — append-log to a fixed path so the
/// diagnostic flow stays readable. Cleared on app launch so each run
/// produces a self-contained trace.
const LOG_PATH: &str = "/tmp/openwhisper-permissions.log";

fn dbg_log(msg: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let line = format!("[{now:.3}] {msg}\n");
    eprintln!("{}", line.trim_end());
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(LOG_PATH) {
        let _ = f.write_all(line.as_bytes());
    }
}

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
    // Truncate at each call so the file holds one boot's trace, not
    // accumulating noise from previous launches.
    let _ = std::fs::write(LOG_PATH, "");
    dbg_log("permissions: request_microphone() entered");
    unsafe {
        let Some(cls) = AnyClass::get(c"AVCaptureDevice") else {
            dbg_log("permissions: AVCaptureDevice class not loaded");
            return;
        };

        let media: *const AnyObject = AVMediaTypeAudio;
        if media.is_null() {
            dbg_log("permissions: AVMediaTypeAudio symbol is null");
            return;
        }

        let status: i64 = msg_send![cls, authorizationStatusForMediaType: media];
        dbg_log(&format!(
            "permissions: mic auth status = {status} \
             (0=NotDetermined, 1=Restricted, 2=Denied, 3=Authorized)"
        ));
        if status != AV_NOT_DETERMINED {
            return;
        }

        let block = RcBlock::new(|granted: Bool| {
            dbg_log(&format!(
                "permissions: mic prompt completed, granted = {}",
                granted.as_bool()
            ));
        });

        dbg_log("permissions: firing AVCaptureDevice.requestAccessForMediaType:audio");
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
