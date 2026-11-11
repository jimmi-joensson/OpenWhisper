//! macOS: one display captured through ScreenCaptureKit, into memory.
//!
//! - **Leaving apps out.** `SCContentFilter(display:
//!   excludingApplications:exceptingWindows:)` captures a display
//!   without the windows of the given apps: what is behind them
//!   shows, as if they were not running. The never-capture apps and
//!   this process go in that list. Checked by a run on a real screen
//!   (TASK-98 notes); the pixel test repeats it.
//! - **Windows and their owners** come from
//!   `CGWindowListCopyWindowInfo`, which needs no permission for
//!   bounds, layer and owner pid. Windows are not filtered by layer:
//!   a password manager's quick-access panel floats above the normal
//!   layer and has to be painted like any other window of it.
//! - **Which app a window belongs to** is asked per pid
//!   (`NSRunningApplication(processIdentifier:)`), never read off
//!   `NSWorkspace.runningApplications`: that list is only refreshed by
//!   the main run loop, which a command-line process does not run.
//! - **A window whose owner is not an app counts as not listed.** The
//!   lookup above answers "no app" for the processes behind the menu
//!   bar, the Dock, the wallpaper and many system overlays, which are
//!   on every screen at every moment. Windows refuses to capture when
//!   it cannot name a window's owner (`windows.rs`); doing the same
//!   here would mean no screenshot, ever. It is also not needed for
//!   the layer that counts on macOS: the capture leaves a listed app
//!   out by the app's own identity as ScreenCaptureKit knows it
//!   (`SCRunningApplication.bundleIdentifier`), not by this lookup,
//!   and it is refused when a listed app that has a window on screen
//!   is not among the apps ScreenCaptureKit can leave out. So what
//!   this lookup decides is which rectangles get the extra coat of
//!   paint, and whether the topmost window's owner counts as "in
//!   front". The difference to Windows is on purpose, and a test
//!   below pins it (`a_window_whose_owner_is_not_an_app_counts_as_not_listed`).
//! - **Permission.** Screen Recording, read with
//!   `CGPreflightScreenCaptureAccess` (never prompts). ScreenCaptureKit
//!   is not touched without it, because asking it for content is what
//!   makes the system show its prompt.
//! - **No display.** With every display asleep ScreenCaptureKit lists
//!   none. That is [`PlatformError::NoDisplay`]; nothing here wakes a
//!   display.
//!
//! Works from any thread (the completion handlers run on
//! ScreenCaptureKit's own queue; the caller waits on a channel).
//! Needs macOS 14 for `SCScreenshotManager`.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::AnyThread;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_app_kit::NSRunningApplication;
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{
    CGBitmapInfo, CGDataProvider, CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGGetActiveDisplayList,
    CGImage, CGMainDisplayID, CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess,
    CGWindowListCopyWindowInfo, CGWindowListOption, kCGColorSpaceSRGB,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSNumber, NSString};
use objc2_screen_capture_kit::{
    SCContentFilter, SCRunningApplication, SCScreenshotManager, SCShareableContent, SCStreamConfiguration,
};

use super::{
    AppSelector, DisplayInfo, FrontApp, PermissionState, Platform, PlatformError, PointRect, RawCapture, id_listed,
};

/// How long ScreenCaptureKit gets to answer. A capture call comes
/// back in well under a second (the whole path, lists and PNG
/// included, was measured at 130 to 330 ms); this is for a system
/// that is stuck.
const TIMEOUT: Duration = Duration::from_secs(5);

/// A reference a completion handler hands to the thread that waits
/// for it, with the release built in.
///
/// The handler runs on ScreenCaptureKit's queue, retains what it was
/// given and sends it over a channel. The waiting thread gives up
/// after [`TIMEOUT`]. A reference sent as a bare address would then
/// be owned by nobody: for the screenshot that is a retained image of
/// the screen that stays in memory until the process ends. Wrapped in
/// this, the reference is released wherever the value is dropped: by
/// the thread that received it, by the send that found nobody
/// listening, or with the channel when the receiver goes away with
/// the value still in it.
struct Handed<T>(T);

// SAFETY: the wrapped reference is used by one thread at a time (the
// handler's queue until it is sent, then the receiver, or nobody),
// and the objects handed over here (`CGImage`, `SCShareableContent`)
// are immutable once made and may be released from any thread.
unsafe impl<T> Send for Handed<T> {}

/// Send what a completion handler was given to the waiting thread.
/// When nobody waits any more (the timeout), the value comes back
/// from the send and is dropped here, which releases it.
fn hand_over<T>(tx: &mpsc::Sender<Result<Handed<T>, String>>, value: Result<T, String>) {
    let _unclaimed = tx.send(value.map(Handed));
}

pub(super) struct Mac;

pub(super) fn request_permission() -> bool {
    CGRequestScreenCaptureAccess()
}

/// A window on screen.
struct Window {
    pid: i32,
    layer: i64,
    bounds: PointRect,
    /// Something a person can see: not fully transparent, and larger
    /// than a dot. Only used to tell which app is in front; every
    /// window of a protected app is painted, seen or not.
    visible: bool,
}

/// The windows on screen, front to back, on every display. `Err` when
/// the system does not hand out the list.
fn windows_on_screen() -> Result<Vec<Window>, String> {
    autoreleasepool(|_| {
        let options = CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
        let Some(list) = CGWindowListCopyWindowInfo(options, 0) else {
            return Err("the window list could not be read".to_string());
        };
        // A CFArray of CFDictionary is toll-free bridged.
        // SAFETY: `list` is a live CFArray for the whole block; its
        // elements are dictionaries with string keys.
        let list: &NSArray<NSDictionary<NSString, AnyObject>> =
            unsafe { &*(CFRetained::as_ptr(&list).as_ptr() as *const _) };
        let number = |d: &NSDictionary<NSString, AnyObject>, key: &str| -> Option<f64> {
            let value = d.objectForKey(&NSString::from_str(key))?;
            Some(value.downcast_ref::<NSNumber>()?.as_f64())
        };
        let mut out = Vec::new();
        for entry in list.iter() {
            // A window whose owner or bounds cannot be read cannot be
            // told from a protected one: the whole list is no good.
            let unreadable = || "a window on screen could not be read".to_string();
            let pid = number(&entry, "kCGWindowOwnerPID").ok_or_else(unreadable)?;
            let layer = number(&entry, "kCGWindowLayer").unwrap_or(0.0) as i64;
            let bounds = entry.objectForKey(&NSString::from_str("kCGWindowBounds")).ok_or_else(unreadable)?;
            let bounds = bounds.downcast_ref::<NSDictionary>().ok_or_else(unreadable)?;
            // SAFETY: the bounds dictionary has string keys and
            // number values (CGRectCreateDictionaryRepresentation).
            let bounds: &NSDictionary<NSString, AnyObject> = unsafe { &*(bounds as *const _ as *const _) };
            let (Some(x), Some(y), Some(width), Some(height)) =
                (number(bounds, "X"), number(bounds, "Y"), number(bounds, "Width"), number(bounds, "Height"))
            else {
                return Err(unreadable());
            };
            let alpha = number(&entry, "kCGWindowAlpha").unwrap_or(1.0);
            let visible = alpha > 0.0 && width >= 8.0 && height >= 8.0;
            out.push(Window { pid: pid as i32, layer, bounds: PointRect { x, y, width, height }, visible });
        }
        Ok(out)
    })
}

/// Bundle id and name of the app that is process `pid`. `None` for a
/// process that is not an app (it then is on nobody's list).
fn app_of(pid: i32) -> Option<(String, String)> {
    autoreleasepool(|_| {
        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?;
        let id = app.bundleIdentifier().map(|s| s.to_string()).unwrap_or_default();
        let name = app.localizedName().map(|s| s.to_string()).unwrap_or_default();
        Some((id, name))
    })
}

/// Whether the owner of a window is one of the listed apps. `app` is
/// what [`app_of`] said about the owning process: `None` for a
/// process that is not an app, which is on nobody's list (see the
/// module docs for why that is not a refusal here).
fn owner_listed(app: Option<(String, String)>, ids: &[String]) -> bool {
    app.is_some_and(|(id, _)| id_listed(&id, ids))
}

/// The pids among the owners of `windows` that `apps` selects.
fn selected_pids(apps: AppSelector<'_>, windows: &[Window]) -> HashSet<i32> {
    let owners: HashSet<i32> = windows.iter().map(|w| w.pid).collect();
    match apps {
        AppSelector::Pids(pids) => owners.into_iter().filter(|pid| pids.contains(&(*pid as u32))).collect(),
        AppSelector::Ids(ids) => {
            if ids.is_empty() {
                return HashSet::new();
            }
            owners.into_iter().filter(|pid| owner_listed(app_of(*pid), ids)).collect()
        }
    }
}

fn display_bounds(display_id: u32) -> PointRect {
    let b = CGDisplayBounds(display_id);
    PointRect { x: b.origin.x, y: b.origin.y, width: b.size.width, height: b.size.height }
}

fn active_displays() -> Vec<u32> {
    let mut ids = [0u32; 16];
    let mut count = 0u32;
    // SAFETY: the buffer holds `ids.len()` entries and `count` is a
    // valid out pointer.
    unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) };
    ids[..(count as usize).min(ids.len())].to_vec()
}

fn display_at(x: f64, y: f64) -> Option<u32> {
    active_displays().into_iter().find(|id| display_bounds(*id).contains(x, y))
}

/// The display that holds the front app's first window on screen
/// (a normal window if it has one, else any), by the window's centre.
fn focused_display(windows: &[Window]) -> Option<u32> {
    let pid = crate::focus::current_app().pid as i32;
    if pid == 0 {
        return None;
    }
    let own = |w: &&Window| w.pid == pid && w.visible;
    let window = windows.iter().filter(own).find(|w| w.layer == 0).or_else(|| windows.iter().find(own))?;
    let (x, y) = window.bounds.centre();
    display_at(x, y)
}

fn error_text(err: *mut NSError) -> String {
    if err.is_null() {
        return "no reason given".to_string();
    }
    // SAFETY: a non-null NSError handed to a completion handler is
    // live for the duration of the handler.
    let e = unsafe { &*err };
    format!("{} code {}", e.domain(), e.code())
}

/// What can be captured right now: displays, apps, windows. Blocks
/// until ScreenCaptureKit answers.
fn shareable_content() -> Result<Retained<SCShareableContent>, String> {
    let (tx, rx) = mpsc::channel::<Result<Handed<Retained<SCShareableContent>>, String>>();
    let block = RcBlock::new(move |content: *mut SCShareableContent, err: *mut NSError| {
        let kept = if content.is_null() {
            Err(format!("the system did not list the screen's content ({})", error_text(err)))
        } else {
            // SAFETY: `content` is a live object; retaining it keeps
            // it alive past the handler.
            unsafe { Retained::retain(content) }
                .ok_or_else(|| "the system did not list the screen's content".to_string())
        };
        hand_over(&tx, kept);
    });
    // SAFETY: the block has the signature the method expects.
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&block) };
    let content = rx
        .recv_timeout(TIMEOUT)
        .map_err(|_| "the system did not answer in time".to_string())??;
    Ok(content.0)
}

/// Copy a captured image into tightly packed RGBA. ScreenCaptureKit
/// hands back 32-bit BGRA (little-endian, alpha first).
// `ByteOrder32Little` is deprecated in favour of a newer spelling of
// the same bit; the value CoreGraphics reports has not changed.
#[allow(deprecated)]
fn rgba_of(image: &CGImage) -> Result<(Vec<u8>, u32, u32), String> {
    let width = CGImage::width(Some(image));
    let height = CGImage::height(Some(image));
    let stride = CGImage::bytes_per_row(Some(image));
    let bits = CGImage::bits_per_pixel(Some(image));
    let info = CGImage::bitmap_info(Some(image));
    if bits != 32 || !info.contains(CGBitmapInfo::ByteOrder32Little) || width == 0 || height == 0 {
        return Err("the capture came back in an unexpected pixel format".to_string());
    }
    let provider = CGImage::data_provider(Some(image)).ok_or("the capture has no pixels")?;
    let data = CGDataProvider::data(Some(&provider)).ok_or("the capture has no pixels")?;
    if stride < width * 4 || data.len() < stride * (height - 1) + width * 4 {
        return Err("the capture came back shorter than its size".to_string());
    }
    // SAFETY: `data` is a live CFData of `len` bytes for this scope.
    let source = unsafe { std::slice::from_raw_parts(data.byte_ptr(), data.len()) };
    let mut rgba = vec![0u8; width * height * 4];
    for y in 0..height {
        let row = &source[y * stride..y * stride + width * 4];
        let out = &mut rgba[y * width * 4..(y + 1) * width * 4];
        for (o, i) in out.chunks_exact_mut(4).zip(row.chunks_exact(4)) {
            o[0] = i[2];
            o[1] = i[1];
            o[2] = i[0];
            o[3] = 255;
        }
    }
    Ok((rgba, width as u32, height as u32))
}

impl Platform for Mac {
    fn permission(&self) -> PermissionState {
        if CGPreflightScreenCaptureAccess() { PermissionState::Granted } else { PermissionState::Denied }
    }

    fn frontmost_apps(&self) -> Result<Vec<FrontApp>, String> {
        let mut front = Vec::new();
        // The app with keyboard focus.
        let focus = crate::focus::current_app();
        if focus.pid != 0 {
            front.push(FrontApp { id: focus.app_id, name: focus.app_name, pid: focus.pid });
        }
        // And the owner of the topmost normal window that is not one
        // of ours, read off the window list, which is always current.
        // Usually the same app; when it is not, both count.
        let own_pid = std::process::id() as i32;
        let windows = windows_on_screen()?;
        if let Some(top) = windows.iter().find(|w| w.layer == 0 && w.visible && w.pid != own_pid)
            && !front.iter().any(|app| app.pid == top.pid as u32)
            && let Some((id, name)) = app_of(top.pid)
        {
            front.push(FrontApp { id, name, pid: top.pid as u32 });
        }
        if front.is_empty() {
            return Err("no app is in front".to_string());
        }
        Ok(front)
    }

    fn window_rects(&self, apps: AppSelector<'_>) -> Result<Vec<PointRect>, String> {
        let windows = windows_on_screen()?;
        let pids = selected_pids(apps, &windows);
        Ok(windows.into_iter().filter(|w| pids.contains(&w.pid)).map(|w| w.bounds).collect())
    }

    fn capture(&self, exclude: AppSelector<'_>, at: Option<(f64, f64)>) -> Result<RawCapture, PlatformError> {
        let failed = PlatformError::Failed;
        // Never without the grant: see the module docs.
        if !CGPreflightScreenCaptureAccess() {
            return Err(failed("screen recording is not allowed".to_string()));
        }
        autoreleasepool(|_| {
            let windows = windows_on_screen().map_err(failed)?;
            let content = shareable_content().map_err(failed)?;
            // SAFETY (this block): plain property reads and one
            // capture call on live ScreenCaptureKit objects.
            unsafe {
                let displays = content.displays();
                if displays.is_empty() {
                    return Err(PlatformError::NoDisplay);
                }
                let wanted = match at {
                    Some((x, y)) => display_at(x, y),
                    None => focused_display(&windows),
                };
                let main = CGMainDisplayID();
                let display = wanted
                    .and_then(|id| displays.iter().find(|d| d.displayID() == id))
                    .or_else(|| if at.is_none() { displays.iter().find(|d| d.displayID() == main) } else { None })
                    .or_else(|| if at.is_none() { displays.iter().next() } else { None })
                    .ok_or_else(|| failed("there is no display at that point".to_string()))?;
                let display_id = display.displayID();

                // The apps to leave out: the selected ones, and this
                // process (the pill and the answer panel).
                let own_pid = std::process::id() as i32;
                let is_selected = |app: &SCRunningApplication| match exclude {
                    AppSelector::Pids(pids) => pids.contains(&(app.processID() as u32)),
                    AppSelector::Ids(ids) => id_listed(&app.bundleIdentifier().to_string(), ids),
                };
                let mut left_out: Vec<Retained<SCRunningApplication>> = Vec::new();
                let mut left_out_pids: HashMap<i32, bool> = HashMap::new();
                for app in content.applications().iter() {
                    let selected = is_selected(&app);
                    if selected || app.processID() == own_pid {
                        left_out_pids.insert(app.processID(), selected);
                        left_out.push(app);
                    }
                }
                // Every selected app that has a window on screen must
                // be among them. One that ScreenCaptureKit does not
                // know cannot be left out, and then nothing is
                // captured.
                for pid in selected_pids(exclude, &windows) {
                    if !left_out_pids.contains_key(&pid) {
                        return Err(failed("an app that must not be captured could not be left out".to_string()));
                    }
                }
                let excluded_apps = left_out_pids.values().filter(|selected| **selected).count();

                let left_out = NSArray::from_retained_slice(&left_out);
                let no_windows = NSArray::new();
                let filter = SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
                    SCContentFilter::alloc(),
                    &display,
                    &left_out,
                    &no_windows,
                );
                // The filter's rectangle is in points; the scale is
                // the display's backing scale (2 on Retina). Capture
                // at native size: the paint layer works on real
                // pixels, the downscale comes after it.
                let rect = filter.contentRect();
                let scale = filter.pointPixelScale() as f64;
                let (width, height) = ((rect.size.width * scale).round(), (rect.size.height * scale).round());
                if !(width >= 1.0 && height >= 1.0 && width <= 32_768.0 && height <= 32_768.0) {
                    return Err(PlatformError::NoDisplay);
                }
                let config = SCStreamConfiguration::new();
                config.setWidth(width as usize);
                config.setHeight(height as usize);
                config.setShowsCursor(false);
                config.setColorSpaceName(kCGColorSpaceSRGB);

                let (tx, rx) = mpsc::channel::<Result<Handed<CFRetained<CGImage>>, String>>();
                let block = RcBlock::new(move |image: *mut CGImage, err: *mut NSError| {
                    let kept = match std::ptr::NonNull::new(image) {
                        // A CGImage is a CF type: retain it past the
                        // handler. From here the reference is in a
                        // `Handed`, so it is released on every path,
                        // also when the wait below has timed out and
                        // nobody will ever receive it.
                        Some(image) => Ok(CFRetained::retain(image)),
                        None => Err(format!("the system returned no image ({})", error_text(err))),
                    };
                    hand_over(&tx, kept);
                });
                SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
                    &filter,
                    &config,
                    Some(&block),
                );
                // On a timeout `rx` is dropped here. An image that
                // arrives later is released by `hand_over`; one that
                // is in the channel already is released with it.
                let image = rx
                    .recv_timeout(TIMEOUT)
                    .map_err(|_| failed("the capture did not finish in time".to_string()))?
                    .map_err(failed)?
                    .0;

                let (rgba, width, height) = rgba_of(&image).map_err(failed)?;
                Ok(RawCapture { rgba, width, height, bounds: display_bounds(display_id), excluded_apps })
            }
        })
    }

    fn target_display(&self) -> Result<DisplayInfo, PlatformError> {
        // CoreGraphics only: ScreenCaptureKit is what makes the system
        // show its prompt, and a dry run must never do that.
        let displays = active_displays();
        if displays.is_empty() {
            return Err(PlatformError::NoDisplay);
        }
        let windows = windows_on_screen().map_err(PlatformError::Failed)?;
        let main = CGMainDisplayID();
        let id = focused_display(&windows)
            .filter(|id| displays.contains(id))
            .or_else(|| displays.contains(&main).then_some(main))
            .unwrap_or(displays[0]);
        let bounds = display_bounds(id);
        let mode = CGDisplayCopyDisplayMode(id)
            .ok_or_else(|| PlatformError::Failed("the display could not be measured".to_string()))?;
        let (width, height) = (CGDisplayMode::pixel_width(Some(&mode)), CGDisplayMode::pixel_height(Some(&mode)));
        if !(1..=32_768).contains(&width) || !(1..=32_768).contains(&height) {
            return Err(PlatformError::NoDisplay);
        }
        Ok(DisplayInfo {
            x: bounds.x,
            y: bounds.y,
            point_width: bounds.width,
            point_height: bounds.height,
            width: width as u32,
            height: height as u32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads that need no permission and show nothing of the screen:
    /// they must work on any Mac, with the display on or asleep.
    #[test]
    fn the_permission_and_the_window_list_can_be_read_without_capturing() {
        let state = Mac.permission();
        assert!(matches!(state, PermissionState::Granted | PermissionState::Denied));
        // An empty selection selects nothing, whatever is on screen.
        if let Ok(rects) = Mac.window_rects(AppSelector::Ids(&[])) {
            assert!(rects.is_empty());
        }
        if let Ok(rects) = Mac.window_rects(AppSelector::Pids(&[])) {
            assert!(rects.is_empty());
        }
        // An app that is not installed owns no window. This walks the
        // owner of every window on screen without looking at one.
        let nobody = vec!["dev.openwhisper.test.not-installed".to_string()];
        if let Ok(rects) = Mac.window_rects(AppSelector::Ids(&nobody)) {
            assert!(rects.is_empty());
        }
        // Whoever is in front, the answer is never an empty list.
        if let Ok(front) = Mac.frontmost_apps() {
            assert!(!front.is_empty());
            assert!(front.iter().all(|app| app.pid != 0));
        }
    }

    #[test]
    fn a_window_whose_owner_is_not_an_app_counts_as_not_listed() {
        let listed = vec!["com.example.vault".to_string()];
        // The lookup's three answers: no app, an app that is not
        // listed, a listed app (or its helper).
        assert!(!owner_listed(None, &listed));
        assert!(!owner_listed(Some(("com.example.editor".into(), "Editor".into())), &listed));
        assert!(!owner_listed(Some((String::new(), String::new())), &listed));
        assert!(owner_listed(Some(("COM.Example.Vault".into(), "Vault".into())), &listed));
        assert!(owner_listed(Some(("com.example.vault.helper".into(), String::new())), &listed));

        // Against the system: a window owned by a process that is no
        // app (launchd, and this test) selects nothing and is not an
        // error, where Windows would refuse.
        let window = |pid: i32| Window {
            pid,
            layer: 0,
            bounds: PointRect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
            visible: true,
        };
        let windows = [window(1), window(std::process::id() as i32)];
        assert!(selected_pids(AppSelector::Ids(&listed), &windows).is_empty());
        // A selection by pid needs no lookup at all.
        assert_eq!(selected_pids(AppSelector::Pids(&[1]), &windows), HashSet::from([1]));
    }

    /// What the timeout used to leak: a reference handed over after
    /// the waiting thread gave up, or left in the channel when it
    /// did. Both are released now.
    #[test]
    fn a_reference_nobody_receives_is_released() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        /// Stands for a retained image: counts its release.
        struct Kept(Arc<AtomicUsize>);
        impl Drop for Kept {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let released = Arc::new(AtomicUsize::new(0));

        // The receiver gave up before the handler ran.
        let (tx, rx) = mpsc::channel::<Result<Handed<Kept>, String>>();
        drop(rx);
        hand_over(&tx, Ok(Kept(released.clone())));
        assert_eq!(released.load(Ordering::SeqCst), 1);

        // The handler ran, and the receiver gave up without taking it.
        let (tx, rx) = mpsc::channel::<Result<Handed<Kept>, String>>();
        hand_over(&tx, Ok(Kept(released.clone())));
        assert_eq!(released.load(Ordering::SeqCst), 1);
        drop(rx);
        assert_eq!(released.load(Ordering::SeqCst), 2);

        // The ordinary case: received, used, released once.
        let (tx, rx) = mpsc::channel::<Result<Handed<Kept>, String>>();
        hand_over(&tx, Ok(Kept(released.clone())));
        let got = rx.recv_timeout(Duration::from_secs(1)).unwrap().unwrap().0;
        assert_eq!(released.load(Ordering::SeqCst), 2);
        drop(got);
        assert_eq!(released.load(Ordering::SeqCst), 3);
        // An error carries no reference.
        hand_over::<Kept>(&tx, Err("no image".to_string()));
        assert!(rx.recv_timeout(Duration::from_secs(1)).unwrap().is_err());
    }

    #[test]
    fn the_display_a_capture_would_take_can_be_measured_without_capturing() {
        match Mac.target_display() {
            Ok(display) => {
                assert!(display.width >= 1 && display.height >= 1);
                assert!(display.point_width > 0.0 && display.point_height > 0.0);
                let scale = f64::from(display.width) / display.point_width;
                assert!((0.5..=4.0).contains(&scale), "scale {scale}");
            }
            // A machine without a display, or one that cannot be
            // measured: a reason, never a guess.
            Err(PlatformError::NoDisplay) => {}
            Err(PlatformError::Failed(why)) => assert!(!why.is_empty()),
        }
    }

    /// A real capture, for a person to run with the display on and
    /// the Screen Recording grant given to the terminal:
    /// `OPENWHISPER_VISION_LIVE=1 cargo test -p openwhisper-core
    /// --no-default-features --features tauri --lib vision::mac -- --ignored`.
    /// It asserts on sizes only and keeps nothing.
    #[test]
    #[ignore = "captures the real screen; run by hand with OPENWHISPER_VISION_LIVE=1"]
    fn live_capture_of_the_focused_display_has_the_displays_proportions() {
        if std::env::var("OPENWHISPER_VISION_LIVE").as_deref() != Ok("1") {
            return;
        }
        assert_eq!(Mac.permission(), PermissionState::Granted, "the terminal needs the Screen Recording grant");
        let raw = match Mac.capture(AppSelector::Ids(&[]), None) {
            Ok(raw) => raw,
            Err(PlatformError::NoDisplay) => return,
            Err(PlatformError::Failed(why)) => panic!("capture failed: {why}"),
        };
        assert_eq!(raw.rgba.len(), raw.width as usize * raw.height as usize * 4);
        assert!(raw.bounds.width > 0.0 && raw.bounds.height > 0.0);
        let scale = raw.width as f64 / raw.bounds.width;
        assert!((scale - raw.height as f64 / raw.bounds.height).abs() < 0.05);
        assert!((1.0..=4.0).contains(&scale));
    }
}
