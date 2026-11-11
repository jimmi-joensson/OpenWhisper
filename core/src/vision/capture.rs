//! One screenshot for one ask: the layers of [`super`] in their fixed
//! order, written against [`Platform`].

use std::time::Instant;

use serde::Serialize;

use super::paint::{self, MAX_SIDE};
use super::{AppSelector, AskCaptureToken, PermissionState, Platform, PlatformError, PointRect, id_matches};
use crate::ask::AskImage;
use crate::settings::{NeverCaptureApp, VisionSettings};

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) const NOT_HERE: &str = "screen capture is not available on this platform";

/// A screenshot, ready to send: a PNG in memory.
///
/// It is made by [`capture_for_ask`] and has one way out,
/// [`Screenshot::into_ask_image`]. Whatever is left of it is zeroed
/// when it is dropped.
#[derive(PartialEq, Eq)]
pub struct Screenshot {
    /// The PNG file's bytes.
    pub png: Vec<u8>,
    /// Size of the image in pixels (longest side at most
    /// [`paint::MAX_SIDE`]).
    pub width: u32,
    pub height: u32,
    /// How many never-capture apps were left out of the capture
    /// itself. Always 0 where the platform cannot do that.
    pub excluded_apps: usize,
    /// How many window rectangles of never-capture apps were painted
    /// black (read before and after the capture, so a window that
    /// did not move counts twice).
    pub painted_rects: usize,
    /// How long the capture itself took.
    pub capture_ms: u64,
}

/// Sizes only: a screenshot does not belong in a log.
impl std::fmt::Debug for Screenshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Screenshot({}x{}, {} bytes, excluded {}, painted {})",
            self.width,
            self.height,
            self.png.len(),
            self.excluded_apps,
            self.painted_rects
        )
    }
}

impl Drop for Screenshot {
    fn drop(&mut self) {
        self.png.fill(0);
    }
}

impl Screenshot {
    /// The image as the ask backends take it. The bytes move; nothing
    /// is copied.
    pub fn into_ask_image(mut self) -> AskImage {
        AskImage { media_type: "image/png".to_string(), data: std::mem::take(&mut self.png) }
    }
}

/// Why there is no screenshot. Every reason has a sentence for the
/// person ([`NoScreenshot::message`]) and a line for the model
/// ([`NoScreenshot::for_model`]).
#[non_exhaustive]
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum NoScreenshot {
    /// No ask is in progress. Nothing but an ask may capture.
    NoAsk,
    /// The process may not record the screen.
    NoPermission,
    /// An app on the never-capture list is in front. Nothing was
    /// captured.
    ProtectedAppInFront { name: String },
    /// Which app is in front could not be read, so the check above
    /// could not be made.
    FrontAppUnknown,
    /// The never-capture apps could not be kept out of the image for
    /// certain: their windows could not be listed, or one could not
    /// be left out of the capture.
    ProtectionFailed { detail: String },
    /// No display to capture (asleep, off, none attached).
    NoDisplay,
    /// The capture itself failed.
    CaptureFailed { detail: String },
}

impl NoScreenshot {
    /// A protected app is why; every other reason is "could not".
    pub fn is_blocked(&self) -> bool {
        matches!(self, Self::ProtectedAppInFront { .. })
    }

    /// One plain sentence for the person.
    pub fn message(&self) -> String {
        match self {
            Self::NoAsk => "A screenshot is only taken while a question is being asked.".to_string(),
            Self::NoPermission => {
                if cfg!(target_os = "macos") {
                    "No screenshot was taken: screen recording is not allowed yet. Allow it in System Settings \
under Privacy & Security, Screen & System Audio Recording, then restart the app."
                        .to_string()
                } else {
                    "No screenshot was taken: this app is not allowed to capture the screen.".to_string()
                }
            }
            Self::ProtectedAppInFront { name } => {
                format!("No screenshot was taken: {name} is in front, and it is on your never-capture list.")
            }
            Self::FrontAppUnknown => {
                "No screenshot was taken: it could not be checked which app is in front.".to_string()
            }
            Self::ProtectionFailed { detail } => format!(
                "No screenshot was taken: the apps on your never-capture list could not be kept out of it ({detail})."
            ),
            Self::NoDisplay => "No screenshot was taken: no display is on.".to_string(),
            Self::CaptureFailed { detail } => format!("No screenshot could be taken ({detail})."),
        }
    }

    /// One short line that tells the model why it got no screenshot,
    /// so its answer says so instead of guessing. Names the protected
    /// app that is in front; see [`NoScreenshot::for_model_with`] for
    /// an ask that must not.
    pub fn for_model(&self) -> String {
        self.for_model_with(true)
    }

    /// [`NoScreenshot::for_model`], with a say in whether the line may
    /// name the app that is in front.
    ///
    /// Which app a person has in front is one of the facts an ask may
    /// tell the model, and a person can switch that fact off
    /// ([`crate::settings::ContextSettings::front_app`]). With it off,
    /// `name_app` is `false`, and the line says that a protected app
    /// is in front without saying which: the reason for the missing
    /// screenshot must not carry the fact the person withheld. The
    /// sentence for the person ([`NoScreenshot::message`]) names the
    /// app either way; it never leaves the machine.
    pub fn for_model_with(&self, name_app: bool) -> String {
        match self {
            Self::NoAsk => "No screenshot was taken.".to_string(),
            Self::NoPermission => {
                "No screenshot was taken, because the app does not have the permission to record the screen yet."
                    .to_string()
            }
            Self::ProtectedAppInFront { name } if name_app => format!(
                "An app the person has marked as never to be captured ({name}, for example a password manager) \
is in front, so no screenshot was taken."
            ),
            Self::ProtectedAppInFront { .. } => {
                "An app the person has marked as never to be captured (for example a password manager) is in \
front, so no screenshot was taken."
                    .to_string()
            }
            Self::FrontAppUnknown | Self::ProtectionFailed { .. } => {
                "No screenshot was taken, because it could not be made sure that protected apps (password \
managers) stay out of it."
                    .to_string()
            }
            Self::NoDisplay => "No screenshot was taken, because no display is switched on.".to_string(),
            Self::CaptureFailed { .. } => "The screenshot could not be taken because of a technical error.".to_string(),
        }
    }
}

impl std::fmt::Display for NoScreenshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

/// Take the screenshot for the ask that `token` belongs to.
///
/// The steps, in this order; the first one that cannot be carried out
/// ends it with the reason and without an image:
///
/// 1. `token` belongs to an ask that is running. Only
///    [`crate::ask`] can make a token, and a token of an ask that
///    has ended is refused here.
/// 2. The process may record the screen.
/// 3. No app in front is on the never-capture list
///    ([`VisionSettings::effective_never_capture`]). If one is,
///    nothing is captured at all.
/// 4. The window rectangles of the listed apps are read.
/// 5. The display is captured, with the listed apps (and this app's
///    own windows) left out where the platform can.
/// 6. The rectangles are read again (a window may have moved
///    meanwhile), the app in front is checked once more, and both
///    sets of rectangles are painted black at the capture's native
///    size.
/// 7. The image is scaled down to a longest side of
///    [`paint::MAX_SIDE`].
/// 8. It is encoded as PNG, in memory.
///
/// Blocks for all of it; call it off the UI thread. Measured on the
/// Mac this was written on (Apple Silicon, one Retina display,
/// 3024 x 1964 pixels): 130 to 330 ms from the first step to the PNG,
/// of which the capture itself is the smaller part and the window
/// lists, the downscale and the PNG the larger. An earlier note said
/// "about 50 ms"; that was the capture call alone, on a good run.
pub fn capture_for_ask(token: &AskCaptureToken, settings: &VisionSettings) -> Result<Screenshot, NoScreenshot> {
    // Unit tests never see the real screen: they capture from the
    // fake a test installed, or not at all.
    #[cfg(test)]
    {
        match super::test_support::installed() {
            Some(fake) => capture_with(&*fake, token, settings),
            None => {
                if !token.is_live() {
                    return Err(NoScreenshot::NoAsk);
                }
                Err(NoScreenshot::CaptureFailed { detail: "no screen in unit tests".to_string() })
            }
        }
    }
    #[cfg(not(test))]
    {
        capture_with(&super::system(), token, settings)
    }
}

/// The name to use for a listed app that is in front: the one on the
/// list, else the app's own, else its id.
pub(super) fn label(listed: &NeverCaptureApp, app_name: &str) -> String {
    if !listed.name.trim().is_empty() {
        listed.name.trim().to_string()
    } else if !app_name.trim().is_empty() {
        app_name.trim().to_string()
    } else {
        listed.id.clone()
    }
}

/// The first app in front that is on the list, by its name.
fn protected_in_front(platform: &dyn Platform, never: &[NeverCaptureApp]) -> Result<Option<String>, NoScreenshot> {
    let front = platform.frontmost_apps().map_err(|_| NoScreenshot::FrontAppUnknown)?;
    if front.is_empty() {
        return Err(NoScreenshot::FrontAppUnknown);
    }
    for app in &front {
        if let Some(listed) = never.iter().find(|l| id_matches(&app.id, &l.id)) {
            return Ok(Some(label(listed, &app.name)));
        }
    }
    Ok(None)
}

/// A capture, painted and scaled down, still as pixels.
struct Finished {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    excluded_apps: usize,
    painted_rects: usize,
    capture_ms: u64,
    /// The captured display, for mapping a point to a pixel. Only
    /// the pixel test's probe reads it.
    #[cfg_attr(not(any(feature = "vision-live-test", test)), allow(dead_code))]
    bounds: PointRect,
}

impl Drop for Finished {
    fn drop(&mut self) {
        self.rgba.fill(0);
    }
}

/// Steps 4 to 7 of [`capture_for_ask`]: rectangles, capture,
/// rectangles again, paint, scale down. `after` runs between the
/// second read and the paint, for one more check.
fn shoot(
    platform: &dyn Platform,
    exclude: AppSelector<'_>,
    paint_over: AppSelector<'_>,
    at: Option<(f64, f64)>,
    after: &mut dyn FnMut() -> Result<(), NoScreenshot>,
) -> Result<Finished, NoScreenshot> {
    let protect = |detail: String| NoScreenshot::ProtectionFailed { detail };
    let before = platform.window_rects(paint_over).map_err(protect)?;

    let started = Instant::now();
    let mut raw = platform.capture(exclude, at).map_err(|e| match e {
        PlatformError::NoDisplay => NoScreenshot::NoDisplay,
        PlatformError::Failed(detail) => NoScreenshot::CaptureFailed { detail },
    })?;
    let capture_ms = started.elapsed().as_millis() as u64;

    // From here on `raw` holds the screen. Every early return drops
    // it, which zeroes it.
    let pixels = raw.width as usize * raw.height as usize;
    if pixels == 0 || raw.rgba.len() != pixels * 4 {
        return Err(NoScreenshot::CaptureFailed { detail: "the capture came back in an unexpected shape".to_string() });
    }
    if !raw.bounds.is_finite() || raw.bounds.width <= 0.0 || raw.bounds.height <= 0.0 {
        return Err(protect("the size of the display is not known".to_string()));
    }
    let again = platform.window_rects(paint_over).map_err(protect)?;
    after()?;

    let mut painted_rects = 0;
    for rect in before.iter().chain(again.iter()) {
        if !rect.is_finite() {
            return Err(protect("a window has no usable position".to_string()));
        }
        if let Some(px) = paint::to_pixels(rect, &raw.bounds, raw.width, raw.height) {
            paint::paint_black(&mut raw.rgba, raw.width, raw.height, px);
            painted_rects += 1;
        }
    }

    let (native_w, native_h) = (raw.width, raw.height);
    let (rgba, width, height) = paint::downscale(std::mem::take(&mut raw.rgba), native_w, native_h, MAX_SIDE);
    crate::verbose_log!(
        "[ow.vision] capture {}x{} -> {}x{} excluded_apps={} painted_rects={} capture_ms={} total_ms={}",
        native_w,
        native_h,
        width,
        height,
        raw.excluded_apps,
        painted_rects,
        capture_ms,
        started.elapsed().as_millis()
    );
    Ok(Finished { rgba, width, height, excluded_apps: raw.excluded_apps, painted_rects, capture_ms, bounds: raw.bounds })
}

/// [`capture_for_ask`] against a given platform.
pub(crate) fn capture_with(
    platform: &dyn Platform,
    token: &AskCaptureToken,
    settings: &VisionSettings,
) -> Result<Screenshot, NoScreenshot> {
    // 1. Only inside an ask.
    if !token.is_live() {
        return Err(NoScreenshot::NoAsk);
    }
    // 2. Permission. "Cannot tell" is not a yes.
    if platform.permission() != PermissionState::Granted {
        return Err(NoScreenshot::NoPermission);
    }
    // 3. A protected app in front: nothing is captured.
    let never = settings.clone().cleaned().effective_never_capture();
    let ids: Vec<String> = never.iter().map(|app| app.id.clone()).collect();
    if let Some(name) = protected_in_front(platform, &never)? {
        return Err(NoScreenshot::ProtectedAppInFront { name });
    }
    // 4 to 7. The app in front is looked at once more after the
    // capture: someone may have switched to their password manager
    // in that moment. Its window is left out and painted over
    // either way; this throws the image away as well.
    let mut recheck = || match protected_in_front(platform, &never)? {
        Some(name) => Err(NoScreenshot::ProtectedAppInFront { name }),
        None => Ok(()),
    };
    let mut done = shoot(platform, AppSelector::Ids(&ids), AppSelector::Ids(&ids), None, &mut recheck)?;
    // 8. PNG, in memory.
    let png = paint::encode_png(&done.rgba, done.width, done.height)
        .map_err(|detail| NoScreenshot::CaptureFailed { detail })?;
    done.rgba.fill(0);
    crate::verbose_log!("[ow.vision] png_bytes={}", png.len());
    Ok(Screenshot {
        png,
        width: done.width,
        height: done.height,
        excluded_apps: done.excluded_apps,
        painted_rects: done.painted_rects,
        capture_ms: done.capture_ms,
    })
}

/// What [`probe_for_test`] captures and where it looks.
#[cfg(any(feature = "vision-live-test", test))]
#[doc(hidden)]
#[derive(Clone, Debug, Default)]
pub struct ProbeOptions {
    /// Processes to leave out of the capture itself (where the
    /// platform can).
    pub exclude_pids: Vec<u32>,
    /// Processes whose window rectangles are painted black.
    pub paint_pids: Vec<u32>,
    /// Where to read a pixel, in global points (the coordinate space
    /// of [`super::PointRect`]). The display that contains the first
    /// point is the one captured. At most [`PROBE_MAX_SAMPLES`].
    pub sample_points: Vec<(f64, f64)>,
}

/// What [`probe_for_test`] saw: sizes, timings and the few pixels it
/// was asked for.
#[cfg(any(feature = "vision-live-test", test))]
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeResult {
    /// Size of the image after the downscale.
    pub width: u32,
    pub height: u32,
    pub capture_ms: u64,
    /// RGBA at each sample point, after exclusion, paint and
    /// downscale, in the order of the points.
    pub samples: Vec<[u8; 4]>,
    /// How many of `exclude_pids` were left out of the capture.
    pub excluded: usize,
    /// How many window rectangles were painted.
    pub painted: usize,
}

/// Most sample points one probe returns: enough to check a test
/// window's corners and centre, far too few to carry an image.
#[cfg(any(feature = "vision-live-test", test))]
pub const PROBE_MAX_SAMPLES: usize = 32;

/// [`probe_for_test`] against a given platform.
#[cfg(any(feature = "vision-live-test", test))]
pub(crate) fn probe_with(platform: &dyn Platform, opts: &ProbeOptions) -> Result<ProbeResult, String> {
    if opts.sample_points.len() > PROBE_MAX_SAMPLES {
        return Err(format!("at most {PROBE_MAX_SAMPLES} sample points"));
    }
    if platform.permission() != PermissionState::Granted {
        return Err(NoScreenshot::NoPermission.message());
    }
    let done = shoot(
        platform,
        AppSelector::Pids(&opts.exclude_pids),
        AppSelector::Pids(&opts.paint_pids),
        opts.sample_points.first().copied(),
        &mut || Ok(()),
    )
    .map_err(|e| e.message())?;
    let mut samples = Vec::with_capacity(opts.sample_points.len());
    for (x, y) in &opts.sample_points {
        if !done.bounds.contains(*x, *y) {
            return Err(format!("the point {x}, {y} is not on the captured display"));
        }
        let px = (((x - done.bounds.x) / done.bounds.width * done.width as f64) as u32).min(done.width - 1);
        let py = (((y - done.bounds.y) / done.bounds.height * done.height as f64) as u32).min(done.height - 1);
        let i = (py * done.width + px) as usize * 4;
        samples.push([done.rgba[i], done.rgba[i + 1], done.rgba[i + 2], done.rgba[i + 3]]);
    }
    Ok(ProbeResult {
        width: done.width,
        height: done.height,
        capture_ms: done.capture_ms,
        samples,
        excluded: done.excluded_apps,
        painted: done.painted_rects,
    })
}

/// A hook for the live pixel test, and for nothing else.
///
/// **Not in a build that ships.** It is compiled only with the cargo
/// feature `vision-live-test`, which no default feature set and no
/// shell switches on; the pixel test's command line does
/// (`cli/tests/vision_live.rs`). Without the feature this function,
/// its types and the path from a pid to a capture do not exist in the
/// binary, and a test in this crate holds that line
/// (`the_probe_hook_is_behind_its_feature`).
///
/// The claim "a never-capture app is not in the image" can only be
/// checked against a real screen: the test puts a window of one solid
/// colour on screen from a helper process, then captures and looks at
/// a few pixels where that window is: not excluded (the colour is
/// there), excluded (what is behind it shows), painted only (black).
/// To do that it has to aim at a process of its own instead of the
/// never-capture list, which is what this function allows, and it
/// has to run outside an ask.
///
/// It runs the very same capture, paint and downscale code as
/// [`capture_for_ask`] and returns only the sampled pixels (at most
/// [`PROBE_MAX_SAMPLES`]), never the image. It does not look at the
/// never-capture list, so it is not for product code: hidden from the
/// docs and named for what it is.
#[cfg(all(feature = "vision-live-test", any(target_os = "macos", target_os = "windows")))]
#[doc(hidden)]
pub fn probe_for_test(opts: ProbeOptions) -> Result<ProbeResult, String> {
    probe_with(&super::system(), &opts)
}

#[cfg(test)]
mod tests {
    use super::super::FrontApp;
    use super::super::test_support::{Fake, GREEN};
    use super::*;
    use crate::settings::VisionTrigger;

    const VAULT: &str = "com.example.vault";

    fn settings() -> VisionSettings {
        VisionSettings {
            trigger: VisionTrigger::Always,
            phrases: None,
            never_capture: Some(vec![
                NeverCaptureApp::new(VAULT, "Vault"),
                NeverCaptureApp::new("com.example.notrunning", "Not Running"),
            ]),
        }
    }

    fn token() -> AskCaptureToken {
        AskCaptureToken::for_test()
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> PointRect {
        PointRect { x, y, width, height }
    }

    fn window(id: &str, r: PointRect) -> (String, PointRect) {
        (id.to_string(), r)
    }

    /// Decode a screenshot's PNG to RGB triples.
    fn pixels(shot: &Screenshot) -> Vec<[u8; 3]> {
        let mut reader = png::Decoder::new(std::io::Cursor::new(&shot.png)).read_info().unwrap();
        let mut rgb = vec![0u8; shot.width as usize * shot.height as usize * 3];
        reader.next_frame(&mut rgb).unwrap();
        rgb.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect()
    }

    fn magenta_ish(px: &[u8; 3]) -> bool {
        px[0] > 40 && px[2] > 40
    }

    #[test]
    fn a_plain_screen_comes_back_as_a_png_of_it() {
        let fake = Fake::default();
        let shot = capture_with(&fake, &token(), &settings()).unwrap();
        // 400 x 300 points at 2x is 800 x 600: under the limit.
        assert_eq!((shot.width, shot.height), (800, 600));
        assert_eq!((shot.excluded_apps, shot.painted_rects), (0, 0));
        assert!(pixels(&shot).iter().all(|p| p == &[GREEN[0], GREEN[1], GREEN[2]]));
        // The layers ran in their order, the front app twice.
        assert_eq!(fake.calls(), ["permission", "front", "rects", "capture", "rects", "front"]);
        // The never-capture ids went to the capture as exclusions.
        assert_eq!(fake.seen.lock().unwrap().excluded, [vec![VAULT.to_string(), "com.example.notrunning".to_string()]]);

        // Neither type prints its bytes.
        let shown = format!("{shot:?}");
        assert!(shown.starts_with("Screenshot(800x600, ") && shown.ends_with("bytes, excluded 0, painted 0)"), "{shown}");
        let image = shot.into_ask_image();
        assert_eq!(image.media_type, "image/png");
        assert_eq!(&image.data[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(format!("{image:?}"), format!("AskImage(image/png, {} bytes)", image.data.len()));
    }

    #[test]
    fn a_large_display_is_scaled_down_to_the_limit() {
        let fake = Fake { bounds: rect(0.0, 0.0, 1512.0, 982.0), ..Fake::default() };
        let shot = capture_with(&fake, &token(), &settings()).unwrap();
        assert_eq!((shot.width, shot.height), (1568, 1018));
    }

    #[test]
    fn without_a_running_ask_there_is_no_capture_and_nothing_is_even_asked() {
        let fake = Fake::default();
        let dead = AskCaptureToken::dead_for_test();
        assert_eq!(capture_with(&fake, &dead, &settings()), Err(NoScreenshot::NoAsk));
        assert!(fake.calls().is_empty());
        // The public entry point refuses the same way, with or
        // without a platform to capture from.
        assert_eq!(capture_for_ask(&dead, &settings()), Err(NoScreenshot::NoAsk));
    }

    #[test]
    fn without_the_permission_there_is_no_capture() {
        for state in [PermissionState::Denied, PermissionState::Unknown] {
            let fake = Fake { permission: state, ..Fake::default() };
            assert_eq!(capture_with(&fake, &token(), &settings()), Err(NoScreenshot::NoPermission));
            assert_eq!(fake.calls(), ["permission"]);
            assert_eq!(fake.captures(), 0);
        }
    }

    #[test]
    fn a_protected_app_in_front_means_no_capture_call_at_all() {
        let fake = Fake { front: vec![Fake::front_app("COM.Example.Vault", "Vault Pro")], ..Fake::default() };
        let no = capture_with(&fake, &token(), &settings()).unwrap_err();
        // The name on the person's list wins over the app's own.
        assert_eq!(no, NoScreenshot::ProtectedAppInFront { name: "Vault".into() });
        assert!(no.is_blocked());
        assert_eq!(fake.calls(), ["permission", "front"]);
        assert_eq!(fake.captures(), 0);

        // Second in the list of apps in front (the owner of the
        // topmost window) counts as well, and so does a helper app.
        let fake = Fake {
            front: vec![Ok(vec![
                FrontApp { id: "com.example.editor".into(), name: "Editor".into(), pid: 1 },
                FrontApp { id: "com.example.vault.helper".into(), name: "".into(), pid: 2 },
            ])],
            ..Fake::default()
        };
        assert!(capture_with(&fake, &token(), &settings()).unwrap_err().is_blocked());
        assert_eq!(fake.captures(), 0);

        // An entry without a name is called by the app's name, then
        // by its id.
        let unnamed = VisionSettings { never_capture: Some(vec![NeverCaptureApp::new(VAULT, "")]), ..settings() };
        let fake = Fake { front: vec![Fake::front_app(VAULT, "Vault Pro")], ..Fake::default() };
        assert_eq!(
            capture_with(&fake, &token(), &unnamed),
            Err(NoScreenshot::ProtectedAppInFront { name: "Vault Pro".into() })
        );
        let fake = Fake { front: vec![Fake::front_app(VAULT, "")], ..Fake::default() };
        assert_eq!(
            capture_with(&fake, &token(), &unnamed),
            Err(NoScreenshot::ProtectedAppInFront { name: VAULT.into() })
        );
    }

    #[test]
    fn an_unreadable_front_app_means_no_capture() {
        for front in [Err("no answer".to_string()), Ok(Vec::new())] {
            let fake = Fake { front: vec![front], ..Fake::default() };
            assert_eq!(capture_with(&fake, &token(), &settings()), Err(NoScreenshot::FrontAppUnknown));
            assert_eq!(fake.calls(), ["permission", "front"]);
            assert_eq!(fake.captures(), 0);
        }
    }

    #[test]
    fn an_unreadable_window_list_before_the_capture_means_no_capture() {
        let fake = Fake { windows: vec![Err("the window list could not be read".into())], ..Fake::default() };
        let no = capture_with(&fake, &token(), &settings()).unwrap_err();
        assert_eq!(no, NoScreenshot::ProtectionFailed { detail: "the window list could not be read".into() });
        assert!(!no.is_blocked());
        assert_eq!(fake.calls(), ["permission", "front", "rects"]);
        assert_eq!(fake.captures(), 0);
    }

    #[test]
    fn an_unreadable_window_list_after_the_capture_throws_the_image_away() {
        let fake = Fake { windows: vec![Ok(Vec::new()), Err("gone".into())], ..Fake::default() };
        assert_eq!(
            capture_with(&fake, &token(), &settings()),
            Err(NoScreenshot::ProtectionFailed { detail: "gone".into() })
        );
        assert_eq!(fake.calls(), ["permission", "front", "rects", "capture", "rects"]);
    }

    #[test]
    fn a_protected_app_that_comes_to_the_front_during_the_capture_throws_the_image_away() {
        let fake = Fake {
            front: vec![Fake::front_app("com.example.editor", "Editor"), Fake::front_app(VAULT, "Vault")],
            ..Fake::default()
        };
        assert_eq!(
            capture_with(&fake, &token(), &settings()),
            Err(NoScreenshot::ProtectedAppInFront { name: "Vault".into() })
        );
        assert_eq!(fake.captures(), 1);
        // And when the front app cannot be read the second time.
        let fake = Fake {
            front: vec![Fake::front_app("com.example.editor", "Editor"), Err("no answer".into())],
            ..Fake::default()
        };
        assert_eq!(capture_with(&fake, &token(), &settings()), Err(NoScreenshot::FrontAppUnknown));
    }

    #[test]
    fn a_capture_that_fails_or_finds_no_display_says_which() {
        let fake = Fake { capture: Err(PlatformError::NoDisplay), ..Fake::default() };
        assert_eq!(capture_with(&fake, &token(), &settings()), Err(NoScreenshot::NoDisplay));
        assert_eq!(fake.calls(), ["permission", "front", "rects", "capture"]);
        let fake = Fake { capture: Err(PlatformError::Failed("an app could not be left out".into())), ..Fake::default() };
        assert_eq!(
            capture_with(&fake, &token(), &settings()),
            Err(NoScreenshot::CaptureFailed { detail: "an app could not be left out".into() })
        );
    }

    #[test]
    fn a_listed_app_that_is_not_running_is_fine_and_other_apps_are_not_painted() {
        let fake = Fake {
            windows: vec![Ok(vec![window("com.example.browser", rect(50.0, 50.0, 200.0, 150.0))])],
            ..Fake::default()
        };
        let shot = capture_with(&fake, &token(), &settings()).unwrap();
        assert_eq!((shot.excluded_apps, shot.painted_rects), (0, 0));
        // The browser's window is in the image, as it should be.
        assert!(pixels(&shot).iter().any(magenta_ish));
    }

    #[test]
    fn a_listed_apps_window_is_left_out_and_its_place_painted() {
        let fake = Fake {
            windows: vec![Ok(vec![window(VAULT, rect(50.0, 50.0, 200.0, 150.0))])],
            ..Fake::default()
        };
        let shot = capture_with(&fake, &token(), &settings()).unwrap();
        // Left out of the capture, and painted from both reads.
        assert_eq!((shot.excluded_apps, shot.painted_rects), (1, 2));
        let px = pixels(&shot);
        assert!(!px.iter().any(magenta_ish));
        // Centre of the window: black. A corner of the screen: green.
        let at = |x: u32, y: u32| px[(y * shot.width + x) as usize];
        assert_eq!(at(300, 250), [0, 0, 0]);
        assert_eq!(at(5, 5), [GREEN[0], GREEN[1], GREEN[2]]);
    }

    #[test]
    fn where_nothing_can_be_left_out_the_paint_alone_removes_the_window() {
        // The Windows situation: the capture has the window in it.
        for scale in [1.0, 1.5, 2.0] {
            for win in [
                rect(50.0, 50.0, 200.0, 150.0),
                rect(50.3, 50.7, 200.4, 150.2),
                rect(-40.0, -30.0, 120.0, 90.0),
                rect(350.0, 260.0, 300.0, 300.0),
            ] {
                let fake = Fake {
                    excludes: false,
                    scale,
                    windows: vec![Ok(vec![window(VAULT, win)])],
                    ..Fake::default()
                };
                let shot = capture_with(&fake, &token(), &settings()).unwrap();
                assert_eq!((shot.excluded_apps, shot.painted_rects), (0, 2));
                let px = pixels(&shot);
                assert!(!px.iter().any(magenta_ish), "magenta left at scale {scale}, window {win:?}");
                assert!(px.iter().any(|p| p == &[0, 0, 0]));
            }
        }
    }

    #[test]
    fn a_window_that_moved_during_the_capture_is_painted_in_both_places() {
        let was = rect(20.0, 20.0, 100.0, 80.0);
        let is = rect(220.0, 180.0, 100.0, 80.0);
        // What the capture shows: the window caught in both places.
        let drawn = Some(vec![window(VAULT, was), window(VAULT, is)]);
        let fake = Fake {
            excludes: false,
            windows: vec![Ok(vec![window(VAULT, was)]), Ok(vec![window(VAULT, is)])],
            drawn: drawn.clone(),
            ..Fake::default()
        };
        let shot = capture_with(&fake, &token(), &settings()).unwrap();
        assert_eq!(shot.painted_rects, 2);
        assert!(!pixels(&shot).iter().any(magenta_ish));

        // The test would notice if only one read were painted: with
        // the second read blind to the move, magenta is left.
        let blind = Fake {
            excludes: false,
            windows: vec![Ok(vec![window(VAULT, was)]), Ok(vec![window(VAULT, was)])],
            drawn,
            ..Fake::default()
        };
        assert!(pixels(&capture_with(&blind, &token(), &settings()).unwrap()).iter().any(magenta_ish));
    }

    #[test]
    fn a_window_without_a_usable_position_means_no_screenshot() {
        let fake = Fake {
            windows: vec![Ok(vec![window(VAULT, rect(f64::NAN, 0.0, 10.0, 10.0))])],
            ..Fake::default()
        };
        assert!(matches!(capture_with(&fake, &token(), &settings()), Err(NoScreenshot::ProtectionFailed { .. })));
        // And a display whose size is not known.
        let fake = Fake { bounds: rect(0.0, 0.0, 400.0, 300.0), scale: 0.0, ..Fake::default() };
        assert!(matches!(capture_with(&fake, &token(), &settings()), Err(NoScreenshot::CaptureFailed { .. })));
    }

    #[test]
    fn the_built_in_list_protects_when_the_person_never_edited_it() {
        let own = crate::settings::default_never_capture();
        let fake = Fake { front: vec![Fake::front_app(&own[0].id.to_uppercase(), "")], ..Fake::default() };
        let no = capture_with(&fake, &token(), &VisionSettings::default()).unwrap_err();
        assert_eq!(no, NoScreenshot::ProtectedAppInFront { name: own[0].name.clone() });
        // A list the person emptied protects nothing, and says so by
        // capturing: that is their choice.
        let emptied = VisionSettings { never_capture: Some(Vec::new()), ..VisionSettings::default() };
        let fake = Fake { front: vec![Fake::front_app(&own[0].id, "")], ..Fake::default() };
        assert!(capture_with(&fake, &token(), &emptied).is_ok());
    }

    #[test]
    fn every_reason_has_a_sentence_for_the_person_and_a_line_for_the_model() {
        let all = [
            NoScreenshot::NoAsk,
            NoScreenshot::NoPermission,
            NoScreenshot::ProtectedAppInFront { name: "Vault".into() },
            NoScreenshot::FrontAppUnknown,
            NoScreenshot::ProtectionFailed { detail: "the window list could not be read".into() },
            NoScreenshot::NoDisplay,
            NoScreenshot::CaptureFailed { detail: "timed out".into() },
        ];
        for no in &all {
            for text in [no.message(), no.for_model()] {
                assert!(text.ends_with('.'), "{text}");
                assert!(!text.contains('\u{2014}') && !text.contains('\u{2013}'), "{text}");
                assert!(!text.contains('\n'));
            }
            assert_eq!(no.to_string(), no.message());
            assert_eq!(no.is_blocked(), matches!(no, NoScreenshot::ProtectedAppInFront { .. }));
        }
        assert!(all[2].message().contains("Vault is in front"));
        assert!(all[2].for_model().contains("(Vault, for example a password manager) is in front, so no screenshot was taken."));
        // An ask that does not tell the model which app is in front
        // does not tell it through this line either. Every other
        // reason reads the same both ways.
        let unnamed = all[2].for_model_with(false);
        assert!(!unnamed.contains("Vault"), "{unnamed}");
        assert!(unnamed.contains("(for example a password manager) is in front, so no screenshot was taken."), "{unnamed}");
        assert_eq!(all[2].for_model_with(true), all[2].for_model());
        for no in &all {
            if !no.is_blocked() {
                assert_eq!(no.for_model_with(false), no.for_model());
            }
            let text = no.for_model_with(false);
            assert!(text.ends_with('.') && !text.contains('\u{2014}') && !text.contains('\u{2013}'), "{text}");
        }
        // The person is told which app, always.
        assert!(all[2].message().contains("Vault"));
        assert!(all[5].message().contains("no display is on"));
        // Technical detail is for the person, not for the model.
        assert!(all[6].message().contains("timed out"));
        assert!(!all[6].for_model().contains("timed out"));
        assert_eq!(
            serde_json::to_string(&all[2]).unwrap(),
            r#"{"reason":"protected-app-in-front","name":"Vault"}"#
        );
    }

    /// The hook that captures outside an ask must not be in a build
    /// nobody asked to have it in. Checked on the source, the blunt
    /// way: the function and its re-export sit behind the feature,
    /// and no feature set that ships switches the feature on.
    #[test]
    fn the_probe_hook_is_behind_its_feature() {
        let gate = "#[cfg(all(feature = \"vision-live-test\", any(target_os = \"macos\", target_os = \"windows\")))]";
        let source = include_str!("capture.rs");
        let hook = source.find("pub fn probe_for_test(").expect("the hook");
        let before = &source[..hook];
        // The attribute stands right in front of it (one more
        // attribute, `doc(hidden)`, in between).
        let attr = before.rfind(gate).expect("the feature gate in front of the hook");
        assert_eq!(before[attr + gate.len()..].trim(), "#[doc(hidden)]");
        let module = include_str!("mod.rs");
        let export = module.find("ProbeOptions, ProbeResult, probe_for_test}").expect("the re-export");
        assert!(module[..export].trim_end().ends_with(&format!("{gate}\npub use capture::{{PROBE_MAX_SAMPLES,")));
        // The feature exists, enables nothing else, and is in no
        // other feature's list.
        let manifest = include_str!("../../Cargo.toml");
        assert!(manifest.contains("\nvision-live-test = [\"vision\"]\n"), "the feature is declared as itself");
        assert_eq!(manifest.matches("\"vision-live-test\"").count(), 0, "another feature switches it on");
        assert_eq!(cfg!(feature = "vision-live-test"), false, "this test run has the hook compiled in");
    }

    #[test]
    fn the_probe_returns_only_the_pixels_it_was_asked_for() {
        let fake = Fake {
            excludes: false,
            windows: vec![Ok(vec![window("com.example.browser", rect(50.0, 50.0, 200.0, 150.0))])],
            ..Fake::default()
        };
        let opts = ProbeOptions {
            exclude_pids: vec![],
            paint_pids: vec![],
            sample_points: vec![(150.0, 125.0), (5.0, 5.0), (399.9, 299.9)],
        };
        let seen = probe_with(&fake, &opts).unwrap();
        assert_eq!((seen.width, seen.height), (800, 600));
        assert_eq!(seen.samples, [[255, 0, 255, 255], GREEN, GREEN]);
        assert_eq!((seen.excluded, seen.painted), (0, 0));

        // A point off the display, too many points, no permission.
        let off = ProbeOptions { sample_points: vec![(150.0, 125.0), (900.0, 5.0)], ..opts.clone() };
        assert!(probe_with(&fake, &off).unwrap_err().contains("not on the captured display"));
        let many = ProbeOptions { sample_points: vec![(1.0, 1.0); PROBE_MAX_SAMPLES + 1], ..opts.clone() };
        assert!(probe_with(&fake, &many).is_err());
        let denied = Fake { permission: PermissionState::Denied, ..Fake::default() };
        assert!(probe_with(&denied, &opts).is_err());
        assert_eq!(denied.captures(), 0);
    }
}
