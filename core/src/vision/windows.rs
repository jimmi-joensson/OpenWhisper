//! Windows: not built yet (TASK-98.2 replaces this file).
//!
//! Until then every ask on Windows that wants the screen ends as "no
//! screenshot" with a plain reason, which is what the rest of Vision
//! does with any platform that cannot keep its promises.
//!
//! For whoever builds it, the contract is in [`super`] (the section
//! "The platform seam") and on each method of [`Platform`]. In short:
//!
//! - `frontmost_apps`: the process that owns the foreground window,
//!   with the id [`crate::focus`] reports for it (the exe file name
//!   in lower case, `1password.exe`).
//! - `window_rects`: the visible, not cloaked top-level windows of
//!   the selected processes (`AppSelector::Ids` holds exe file names,
//!   compare with [`super::id_listed`]; `AppSelector::Pids` holds
//!   process ids), as their DWM extended frame bounds, in
//!   virtual-desktop coordinates (origin at the primary monitor's top
//!   left corner, y down, physical pixels: the process must be
//!   per-monitor DPI aware). `Err` when the windows cannot be
//!   enumerated or an owner cannot be read.
//! - `capture`: the whole monitor that holds the foreground window
//!   (or, with `at`, the monitor containing that point), as RGBA at
//!   its native pixel size with `bounds` set to the monitor's
//!   rectangle in the same coordinates. Windows has no way to leave
//!   an app out of a monitor capture, so `exclude` is ignored and
//!   `excluded_apps` is 0: the caller paints `window_rects` instead,
//!   read before and after the capture.

use super::{AppSelector, FrontApp, PermissionState, Platform, PlatformError, PointRect, RawCapture};

const NOT_BUILT: &str = "screen capture is not built for Windows yet";

pub(super) struct Windows;

impl Platform for Windows {
    fn permission(&self) -> PermissionState {
        // Windows has no grant for capturing the screen.
        PermissionState::Granted
    }

    fn frontmost_apps(&self) -> Result<Vec<FrontApp>, String> {
        let focus = crate::focus::current_app();
        if focus.pid == 0 {
            return Err("the foreground window's process could not be read".to_string());
        }
        Ok(vec![FrontApp { id: focus.app_id, name: focus.app_name, pid: focus.pid }])
    }

    fn window_rects(&self, _apps: AppSelector<'_>) -> Result<Vec<PointRect>, String> {
        Err(NOT_BUILT.to_string())
    }

    fn capture(&self, _exclude: AppSelector<'_>, _at: Option<(f64, f64)>) -> Result<RawCapture, PlatformError> {
        Err(PlatformError::Failed(NOT_BUILT.to_string()))
    }
}
