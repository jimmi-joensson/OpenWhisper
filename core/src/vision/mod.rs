//! Vision: a screenshot of the display the person is working on, as
//! context for an ask (TASK-98).
//!
//! ```text
//! ask --> decide (settings + the question's text, on the machine)
//!           |
//!           +-- no capture ---------------------------> text only
//!           +-- capture --> capture_for_ask --> PNG --> first turn
//!           +-- ask the model --> text first; the screen follows
//!                                 only when the model asks for it
//! ```
//!
//! This is a privacy feature first and a capture second. The rules:
//!
//! - **A screenshot exists only inside an ask.** The one entry point,
//!   [`capture_for_ask`], takes an [`AskCaptureToken`]. That type has
//!   no public constructor: only code inside [`crate::ask`] can make
//!   one, so the dictation pipeline, a timer or a shell command has
//!   nothing to call this module with. That is a fact the compiler
//!   keeps. On top of it the token is checked when it is used: the ask
//!   it was made for must still be running.
//! - **In memory only.** The capture is a pixel buffer, then a PNG in
//!   a `Vec<u8>`, handed to the ask backend as an
//!   [`crate::ask::AskImage`] and dropped when that request returns.
//!   Nothing in this module opens a file for writing, nothing here is
//!   logged except sizes and timings in the verbose log, and no type
//!   that holds pixels prints them (`Debug` shows sizes). Buffers are
//!   zeroed when they are dropped.
//! - **Apps on the never-capture list never reach the image**
//!   ([`crate::settings::VisionSettings::never_capture`], password
//!   managers by default). Three layers, in this order:
//!   1. one of them is in front: no capture at all;
//!   2. where the platform can (macOS), they are left out of the
//!      capture itself, so what is behind their windows shows;
//!   3. their window rectangles, read before *and* after the capture,
//!      are painted black in the buffer at its native size, before it
//!      is scaled down ([`paint`]). On Windows this is the only layer
//!      for a window that is not in front.
//! - **Fail closed.** If any of these steps cannot be carried out
//!   (no permission, the app in front or the window list cannot be
//!   read, an app could not be left out, no display), there is no
//!   screenshot, and [`NoScreenshot`] says why in plain words: one
//!   sentence for the person and one for the model, so the answer
//!   says that the screen was not seen instead of guessing.
//!
//! What a list of apps cannot cover, and the settings say so: a
//! password manager's browser extension (its popup is part of the
//! browser's window), and a password shown on a web page or in a
//! terminal.
//!
//! Which display: the one that holds the front app's first window on
//! screen; the main display when that cannot be told. Always a whole
//! display, never more than one.
//!
//! # The platform seam
//!
//! Everything above is written against [`Platform`], so the rules run
//! in unit tests against a fake with no screen. `mac.rs` implements it
//! with ScreenCaptureKit; `windows.rs` is a stub until TASK-98.2. The
//! contract, for whoever implements a platform:
//!
//! **Coordinates.** Rectangles ([`PointRect`]) are in the desktop's
//! global coordinate space: origin at the top left corner of the
//! primary display, x to the right, y **down**, in the unit the
//! platform reports window bounds in (points on macOS, where a Retina
//! pixel is half a point; virtual-desktop pixels on Windows, where
//! the process must be per-monitor DPI aware so that these equal
//! physical pixels). A display to the left of or above the primary
//! one has negative coordinates. [`RawCapture::bounds`] is the
//! captured display in that same space; the image covers exactly
//! that rectangle, so a point `(x, y)` is at pixel
//! `((x - bounds.x) * width / bounds.width,
//!   (y - bounds.y) * height / bounds.height)`.
//! The mapping, the outward rounding and the 2 px grow are done here
//! ([`paint::to_pixels`]); a platform only reports rectangles.
//!
//! **Pixels.** [`RawCapture::rgba`] is `width * height * 4` bytes,
//! RGBA, 8 bits each, alpha 255, rows from the top, no row padding,
//! at the display's native pixel size (not scaled down), preferably
//! in sRGB.
//!
//! **App ids** are the ones [`crate::focus`] reports: the bundle id on
//! macOS, the exe file name on Windows. Compare with [`id_matches`]
//! (case is ignored; on macOS an id also covers its helper apps).
//!
//! **Errors are refusals.** A method that cannot do its whole job
//! returns `Err`; it never returns a partial answer (half a window
//! list, a capture with an app it could not leave out). The caller
//! turns every `Err` into "no screenshot".

use serde::Serialize;

mod capture;
pub mod paint;
pub mod trigger;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "windows")]
mod windows;

pub use crate::ask::AskCaptureToken;
pub use capture::{NoScreenshot, Screenshot, capture_for_ask};
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use capture::{PROBE_MAX_SAMPLES, ProbeOptions, ProbeResult, probe_for_test};
pub use trigger::{Decision, decide, matched_phrase};

/// Whether this process may record the screen.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionState {
    Granted,
    /// Not granted: never asked, or refused. macOS does not tell the
    /// two apart.
    Denied,
    /// The platform could not say. Treated as not granted.
    Unknown,
}

impl PermissionState {
    /// Stable id, same as the serialized form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Denied => "denied",
            Self::Unknown => "unknown",
        }
    }
}

/// A rectangle in the desktop's global coordinate space. See the
/// module docs for the convention.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl PointRect {
    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.width.is_finite() && self.height.is_finite()
    }

    /// The point is inside (right and bottom edges excluded).
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    pub fn centre(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

/// An app that counts as being in front.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FrontApp {
    /// As [`crate::focus::FocusContext::app_id`]. May be empty for a
    /// program that has no such id; it then matches no list entry.
    pub id: String,
    /// Name to show a person; may be empty.
    pub name: String,
    pub pid: u32,
}

/// Which apps a platform call is about.
#[derive(Clone, Copy, Debug)]
pub enum AppSelector<'a> {
    /// Apps by id, compared with [`id_matches`]. What a real capture
    /// uses: the ids of the never-capture list.
    Ids(&'a [String]),
    /// Processes by id. Only the pixel test uses this
    /// ([`probe_for_test`]), to aim at its own helper window.
    Pids(&'a [u32]),
}

/// One captured display, as the platform delivers it. See the module
/// docs for the pixel layout.
pub struct RawCapture {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// The captured display in global coordinates.
    pub bounds: PointRect,
    /// How many of the selected apps were running with something to
    /// leave out and were left out of the capture itself. 0 on a
    /// platform that cannot do that.
    pub excluded_apps: usize,
}

/// Sizes only: a screenshot does not belong in a log.
impl std::fmt::Debug for RawCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RawCapture({}x{}, {} bytes)", self.width, self.height, self.rgba.len())
    }
}

impl Drop for RawCapture {
    fn drop(&mut self) {
        self.rgba.fill(0);
    }
}

/// Why a platform could not capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformError {
    /// There is no display to capture: every display is asleep or
    /// off, or none is attached.
    NoDisplay,
    /// Anything else, in a few plain words ("the capture timed out").
    Failed(String),
}

/// What Vision needs from the operating system. See the module docs
/// for the contract every method keeps.
pub trait Platform {
    /// Whether this process may record the screen. Must not prompt.
    fn permission(&self) -> PermissionState;

    /// Every app that may count as being in front right now: the app
    /// with keyboard focus and, where the platform can tell, the
    /// owner of the topmost normal window when that is another app.
    /// Usually one entry. Never empty: when nothing can be read this
    /// is an `Err`, and there will be no capture.
    fn frontmost_apps(&self) -> Result<Vec<FrontApp>, String>;

    /// The rectangles of every window the selected apps have on
    /// screen right now, on any display and in any window layer
    /// (popups and floating panels included), in global coordinates.
    /// A selected app that is not running, or has no window on
    /// screen, simply contributes nothing. `Err` when the window list
    /// or the owners of the windows cannot be read.
    fn window_rects(&self, apps: AppSelector<'_>) -> Result<Vec<PointRect>, String>;

    /// Capture one whole display at its native pixel size.
    ///
    /// - Which display: with `at`, the one that contains that global
    ///   point (the pixel test); without, the one that holds the
    ///   front app's first window on screen, or the main display when
    ///   that cannot be told.
    /// - `exclude`: where the platform can leave apps out of the
    ///   capture itself, these are left out, and so are this
    ///   process's own windows (the pill, the answer panel). If one
    ///   of them has a window on screen and cannot be left out, this
    ///   is an `Err`. A platform without such a mechanism ignores
    ///   `exclude`, reports `excluded_apps: 0`, and relies on the
    ///   caller painting [`Platform::window_rects`].
    /// - The mouse pointer is not part of the image.
    /// - No display at all is [`PlatformError::NoDisplay`].
    fn capture(&self, exclude: AppSelector<'_>, at: Option<(f64, f64)>) -> Result<RawCapture, PlatformError>;
}

/// Whether the app `app_id` is the listed app `listed_id`: the same
/// id ignoring case, or (bundle ids) one of its helper apps, whose
/// ids extend the app's own with a dot (`com.example.vault.helper`
/// for `com.example.vault`).
pub fn id_matches(app_id: &str, listed_id: &str) -> bool {
    let app = app_id.trim().to_lowercase();
    let listed = listed_id.trim().to_lowercase();
    if app.is_empty() || listed.is_empty() {
        return false;
    }
    app == listed || (app.len() > listed.len() + 1 && app.starts_with(&listed) && app.as_bytes()[listed.len()] == b'.')
}

/// Whether the app is any of the listed ones ([`id_matches`]).
pub fn id_listed(app_id: &str, listed: &[String]) -> bool {
    listed.iter().any(|l| id_matches(app_id, l))
}

/// On a platform without screen capture.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
struct Unsupported;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl Platform for Unsupported {
    fn permission(&self) -> PermissionState {
        PermissionState::Granted
    }
    fn frontmost_apps(&self) -> Result<Vec<FrontApp>, String> {
        Err(capture::NOT_HERE.to_string())
    }
    fn window_rects(&self, _apps: AppSelector<'_>) -> Result<Vec<PointRect>, String> {
        Err(capture::NOT_HERE.to_string())
    }
    fn capture(&self, _exclude: AppSelector<'_>, _at: Option<(f64, f64)>) -> Result<RawCapture, PlatformError> {
        Err(PlatformError::Failed(capture::NOT_HERE.to_string()))
    }
}

/// The platform of this build. Not used by unit tests, which never
/// touch the real screen (see [`capture_for_ask`]).
#[cfg_attr(test, allow(dead_code))]
fn system() -> impl Platform {
    #[cfg(target_os = "macos")]
    {
        mac::Mac
    }
    #[cfg(target_os = "windows")]
    {
        windows::Windows
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Unsupported
    }
}

/// Whether this process may record the screen, without asking for
/// it. For the settings pane and `openwhisper vision status`.
///
/// - macOS: the Screen Recording grant
///   (`CGPreflightScreenCaptureAccess`). The desktop app has its own;
///   a CLI run from a terminal uses the terminal's. A grant given
///   while the app runs only counts after the app is restarted.
/// - Elsewhere: [`PermissionState::Granted`], there is nothing to
///   grant.
pub fn permission() -> PermissionState {
    system().permission()
}

/// Ask the system for the right to record the screen. Returns whether
/// the process has it now.
///
/// - macOS: `CGRequestScreenCaptureAccess`. The first call shows the
///   system's prompt and adds the app to the Screen Recording list in
///   System Settings; later calls show nothing. Either way the answer
///   is the current state, and a grant given just now is only seen
///   after a restart.
/// - Elsewhere: `true`.
///
/// Only ever called because a person pressed a button for it.
pub fn request_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        mac::request_permission()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// A scripted [`Platform`] for unit tests, here and in the ask
/// module, and the switch that makes [`capture_for_ask`] use it.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::{Arc, Mutex};

    pub(crate) const GREEN: [u8; 4] = [0, 200, 0, 255];
    pub(crate) const MAGENTA: [u8; 4] = [255, 0, 255, 255];

    /// What the fake was asked, in order: `"permission"`, `"front"`,
    /// `"rects"`, `"capture"`.
    #[derive(Default, Debug)]
    pub(crate) struct Seen {
        pub calls: Vec<&'static str>,
        pub captures: usize,
        pub excluded: Vec<Vec<String>>,
    }

    /// A screen of one solid colour with the windows of some apps on
    /// it, each filled with magenta in what `capture` returns (unless
    /// the app was excluded and `excludes` is set).
    pub(crate) struct Fake {
        pub permission: PermissionState,
        /// One answer per call; the last one repeats.
        pub front: Vec<Result<Vec<FrontApp>, String>>,
        /// `(app id, rectangle)`. One list per call of `window_rects`;
        /// the last one repeats.
        pub windows: Vec<Result<Vec<(String, PointRect)>, String>>,
        pub capture: Result<(), PlatformError>,
        /// What is on the screen at the moment of the capture, when
        /// that is not the first list of `windows` (a window that is
        /// moving shows somewhere the first read did not see).
        pub drawn: Option<Vec<(String, PointRect)>>,
        pub bounds: PointRect,
        pub scale: f64,
        /// Leaves excluded apps out of the image, like macOS.
        pub excludes: bool,
        pub seen: Mutex<Seen>,
        pub front_calls: Mutex<usize>,
        pub rect_calls: Mutex<usize>,
    }

    impl Default for Fake {
        fn default() -> Self {
            Self {
                permission: PermissionState::Granted,
                front: vec![Ok(vec![FrontApp { id: "com.example.editor".into(), name: "Editor".into(), pid: 41 }])],
                windows: vec![Ok(Vec::new())],
                capture: Ok(()),
                drawn: None,
                bounds: PointRect { x: 0.0, y: 0.0, width: 400.0, height: 300.0 },
                scale: 2.0,
                excludes: true,
                seen: Mutex::new(Seen::default()),
                front_calls: Mutex::new(0),
                rect_calls: Mutex::new(0),
            }
        }
    }

    impl Fake {
        pub(crate) fn front_app(id: &str, name: &str) -> Result<Vec<FrontApp>, String> {
            Ok(vec![FrontApp { id: id.into(), name: name.into(), pid: 77 }])
        }

        pub(crate) fn captures(&self) -> usize {
            self.seen.lock().unwrap().captures
        }

        pub(crate) fn calls(&self) -> Vec<&'static str> {
            self.seen.lock().unwrap().calls.clone()
        }

        fn nth<T: Clone>(list: &[T], counter: &Mutex<usize>) -> T {
            let mut n = counter.lock().unwrap();
            let item = list[(*n).min(list.len() - 1)].clone();
            *n += 1;
            item
        }

        fn selected(apps: AppSelector<'_>, id: &str) -> bool {
            match apps {
                AppSelector::Ids(ids) => id_listed(id, ids),
                AppSelector::Pids(_) => false,
            }
        }
    }

    impl Platform for Fake {
        fn permission(&self) -> PermissionState {
            self.seen.lock().unwrap().calls.push("permission");
            self.permission
        }

        fn frontmost_apps(&self) -> Result<Vec<FrontApp>, String> {
            self.seen.lock().unwrap().calls.push("front");
            Self::nth(&self.front, &self.front_calls)
        }

        fn window_rects(&self, apps: AppSelector<'_>) -> Result<Vec<PointRect>, String> {
            self.seen.lock().unwrap().calls.push("rects");
            let windows = Self::nth(&self.windows, &self.rect_calls)?;
            Ok(windows.into_iter().filter(|(id, _)| Self::selected(apps, id)).map(|(_, r)| r).collect())
        }

        fn capture(&self, exclude: AppSelector<'_>, _at: Option<(f64, f64)>) -> Result<RawCapture, PlatformError> {
            {
                let mut seen = self.seen.lock().unwrap();
                seen.calls.push("capture");
                seen.captures += 1;
                if let AppSelector::Ids(ids) = exclude {
                    seen.excluded.push(ids.to_vec());
                }
            }
            self.capture.clone()?;
            let width = (self.bounds.width * self.scale).round() as u32;
            let height = (self.bounds.height * self.scale).round() as u32;
            let mut rgba: Vec<u8> = GREEN.iter().copied().cycle().take(width as usize * height as usize * 4).collect();
            // The windows as they are at the moment of the capture:
            // the list the first `window_rects` call returned, unless
            // the test says otherwise.
            let on_screen = self.drawn.clone().unwrap_or_else(|| self.windows[0].clone().unwrap_or_default());
            let mut excluded = std::collections::HashSet::new();
            for (id, rect) in &on_screen {
                if self.excludes && Self::selected(exclude, id) {
                    excluded.insert(id.clone());
                    continue;
                }
                let x0 = (((rect.x - self.bounds.x) * self.scale).floor().max(0.0) as u32).min(width);
                let y0 = (((rect.y - self.bounds.y) * self.scale).floor().max(0.0) as u32).min(height);
                let x1 = (((rect.x + rect.width - self.bounds.x) * self.scale).ceil().max(0.0) as u32).min(width);
                let y1 = (((rect.y + rect.height - self.bounds.y) * self.scale).ceil().max(0.0) as u32).min(height);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = (y * width + x) as usize * 4;
                        rgba[i..i + 4].copy_from_slice(&MAGENTA);
                    }
                }
            }
            Ok(RawCapture { rgba, width, height, bounds: self.bounds, excluded_apps: excluded.len() })
        }
    }

    static PLATFORM: Mutex<Option<Arc<Fake>>> = Mutex::new(None);

    /// The fake [`capture_for_ask`] uses, if a test installed one.
    pub(crate) fn installed() -> Option<Arc<Fake>> {
        PLATFORM.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Removes the installed fake when dropped.
    pub(crate) struct Installed(pub Arc<Fake>);

    impl Drop for Installed {
        fn drop(&mut self) {
            *PLATFORM.lock().unwrap_or_else(|p| p.into_inner()) = None;
        }
    }

    /// Make [`capture_for_ask`] capture from `fake` until the returned
    /// guard is dropped. The caller holds a lock that keeps other
    /// tests from doing the same meanwhile (the ask tests hold
    /// [`crate::settings::CACHE_TEST_LOCK`] through their scope).
    pub(crate) fn install(fake: Fake) -> Installed {
        let fake = Arc::new(fake);
        *PLATFORM.lock().unwrap_or_else(|p| p.into_inner()) = Some(fake.clone());
        Installed(fake)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_match_ignoring_case_and_cover_helper_apps() {
        assert!(id_matches("com.1password.1password", "com.1password.1password"));
        assert!(id_matches("COM.Apple.Passwords", "com.apple.passwords"));
        assert!(id_matches(" 1Password.exe ", "1password.exe"));
        assert!(id_matches("com.example.vault.helper", "com.example.vault"));
        assert!(id_matches("com.example.vault.helper.gpu", "com.example.vault"));
        // Not a helper: another app whose id merely starts the same.
        assert!(!id_matches("com.example.vaulted", "com.example.vault"));
        assert!(!id_matches("com.example.vault.", "com.example.vault"));
        assert!(!id_matches("com.example", "com.example.vault"));
        assert!(!id_matches("", "com.example.vault"));
        assert!(!id_matches("com.example.vault", ""));
        assert!(!id_matches("", ""));
        let listed = vec!["bitwarden.exe".to_string(), "com.example.vault".to_string()];
        assert!(id_listed("Bitwarden.exe", &listed));
        assert!(!id_listed("notepad.exe", &listed));
        assert!(!id_listed("bitwarden.exe", &[]));
    }

    #[test]
    fn rectangles_know_their_inside() {
        let r = PointRect { x: -10.0, y: 20.0, width: 100.0, height: 50.0 };
        assert!(r.contains(-10.0, 20.0));
        assert!(r.contains(89.9, 69.9));
        assert!(!r.contains(90.0, 30.0));
        assert!(!r.contains(0.0, 70.0));
        assert_eq!(r.centre(), (40.0, 45.0));
        assert!(r.is_finite());
        assert!(!PointRect { x: f64::NAN, ..r }.is_finite());
    }

    #[test]
    fn states_have_stable_ids_and_pixels_are_never_printed() {
        for state in [PermissionState::Granted, PermissionState::Denied, PermissionState::Unknown] {
            assert_eq!(serde_json::to_string(&state).unwrap(), format!("\"{}\"", state.as_str()));
        }
        let raw = RawCapture {
            rgba: vec![7; 16],
            width: 2,
            height: 2,
            bounds: PointRect { x: 0.0, y: 0.0, width: 2.0, height: 2.0 },
            excluded_apps: 0,
        };
        assert_eq!(format!("{raw:?}"), "RawCapture(2x2, 16 bytes)");
    }

    /// The module's promise that a capture never touches the disk,
    /// checked the blunt way: none of its source files names an API
    /// that creates or writes a file. (This test's own list is built
    /// from pieces so that it does not find itself.)
    #[test]
    fn nothing_in_this_module_opens_a_file_for_writing() {
        let sources = [
            ("mod.rs", include_str!("mod.rs")),
            ("capture.rs", include_str!("capture.rs")),
            ("paint.rs", include_str!("paint.rs")),
            ("trigger.rs", include_str!("trigger.rs")),
            ("mac.rs", include_str!("mac.rs")),
            ("windows.rs", include_str!("windows.rs")),
        ];
        let banned: Vec<String> = [
            ("std::", "fs"),
            ("fs", "::write"),
            ("File", "::create"),
            ("Open", "Options"),
            ("temp", "file"),
            ("temp", "_dir"),
            ("write", "ToFile"),
            ("write", "ToURL"),
            ("CGImage", "Destination"),
            ("NSBitmap", "ImageRep"),
            ("NSPaste", "board"),
            ("Create", "FileW"),
            ("Write", "File"),
            ("println", "!"),
            ("eprintln", "!"),
            ("dbg", "!"),
        ]
        .iter()
        .map(|(a, b)| format!("{a}{b}"))
        .collect();
        for (name, source) in sources {
            for word in &banned {
                assert!(!source.contains(word.as_str()), "{name} mentions {word}");
            }
        }
    }
}
