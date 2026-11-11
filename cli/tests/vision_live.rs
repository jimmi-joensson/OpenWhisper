//! The live pixel test of Vision (TASK-98): an app that must never be
//! captured is really not in the image.
//!
//! This is the one claim about Vision that cannot be checked against
//! a fake screen. So the test puts two windows of its own on the real
//! one, each from a helper process (`openwhisper vision test-window`,
//! hidden): a green one, and a smaller magenta one on top of it. The
//! magenta window stands for a password manager. Then it captures the
//! display four ways through the same capture, paint and scale code an
//! ask uses (`core::vision::probe_for_test`) and looks at a few pixels
//! where the magenta window is:
//!
//! | the magenta process is       | the pixels there are            |
//! | ---------------------------- | ------------------------------- |
//! | neither left out nor painted | magenta (the control)           |
//! | left out of the capture      | green: what is behind it shows  |
//! | painted over                 | black                           |
//! | left out and painted over    | black                           |
//!
//! (The two rows with "left out" are macOS only; see "On Windows"
//! below.)
//!
//! and, every time, at a point of the green window that the magenta
//! one does not cover: green, so nothing else was touched.
//!
//! The helpers are plain processes, not app bundles, so they have no
//! bundle id for a never-capture list to name. That is why the probe
//! aims at process ids.
//!
//! **Privacy.** The probe returns the colour of the points it is
//! asked about and nothing else, and this test only asks about points
//! inside its own windows, which lie above every other window. No
//! image is written, shown or printed.
//!
//! It needs a display that is on and the Screen Recording permission
//! for the terminal, and it puts two coloured rectangles near the top
//! left of the main display for a few seconds. So it does not run by
//! itself, and it is not even compiled by itself: both the hook it
//! captures through and its helper window exist only with the cargo
//! feature `vision-live-test`, which no build that ships has. On
//! macOS:
//!
//! ```text
//! OPENWHISPER_VISION_LIVE=1 cargo test -p openwhisper-cli --features vision-live-test --test vision_live -- --ignored --nocapture
//! ```
//!
//! Without a display or without the permission it fails and says
//! which; it never passes without having looked.
//!
//! # On Windows
//!
//! Windows has no way to leave an app out of a capture: a listed
//! app's windows are painted over, and that is the only layer. So the
//! test has two cases there, the control (the magenta window is in
//! the image) and painted (black where it is, green beside it). The
//! helper window is a borderless topmost Win32 popup. None of this
//! has run on a real Windows machine yet; this is the command for the
//! first run, from the repository root, in PowerShell:
//!
//! ```text
//! $env:OPENWHISPER_VISION_LIVE = "1"
//! cargo test -p openwhisper-cli --features vision-live-test --test vision_live -- --ignored --nocapture
//! ```
//!
//! (`cmd.exe`: `set OPENWHISPER_VISION_LIVE=1` first, then the same
//! `cargo test` line.) Run it with the display on and unlocked, at any
//! display scaling; two rectangles appear near the top left of the
//! primary monitor for a few seconds. A pass prints the two cases
//! with the colours it saw. If it fails, the message says at which
//! point which colour was found, and that is what to report.

#![cfg(all(feature = "vision-live-test", any(target_os = "macos", target_os = "windows")))]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use openwhisper_core::vision::{ProbeOptions, ProbeResult, probe_for_test};

/// A rectangle in the coordinates Vision uses (origin top left of the
/// main display, y downward): global display points on macOS,
/// virtual-desktop pixels on Windows.
#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// The green window, and the magenta one inside it.
const GREEN: Rect = Rect { x: 120.0, y: 140.0, width: 360.0, height: 260.0 };
const MAGENTA: Rect = Rect { x: 200.0, y: 200.0, width: 160.0, height: 110.0 };

/// Points well inside the magenta window: its centre and near its
/// corners.
const IN_MAGENTA: [(f64, f64); 5] = [(280.0, 255.0), (216.0, 214.0), (344.0, 214.0), (216.0, 296.0), (344.0, 296.0)];
/// Points of the green window that the magenta one does not cover.
const IN_GREEN_ONLY: [(f64, f64); 2] = [(150.0, 170.0), (450.0, 370.0)];

/// A helper window's process. Killed when dropped, on every way out
/// of the test.
struct Helper {
    child: Child,
}

impl Helper {
    /// Start a helper and wait until its window is ordered in.
    fn start(colour: &str, rect: Rect, level: u32) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_openwhisper"))
            .args(["vision", "test-window", "--color", colour, "--timeout-s", "60"])
            .args(["--x", &rect.x.to_string(), "--y", &rect.y.to_string()])
            .args(["--width", &rect.width.to_string(), "--height", &rect.height.to_string()])
            .args(["--level", &level.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start the helper window");
        let stdout = child.stdout.take().expect("the helper's stdout");
        // From here on the helper is killed when this function leaves
        // early.
        let mut helper = Self { child };
        let (said, heard) = mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = BufReader::new(stdout).read_line(&mut line);
            let _ = said.send(line);
        });
        match heard.recv_timeout(Duration::from_secs(15)) {
            Ok(line) if line.trim() == "ready" => helper,
            other => {
                let _ = helper.child.kill();
                let mut why = String::new();
                if let Some(mut stderr) = helper.child.stderr.take() {
                    use std::io::Read;
                    let _ = stderr.read_to_string(&mut why);
                }
                panic!("the {colour} helper window did not come up ({other:?}): {}", why.trim());
            }
        }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Colour {
    Magenta,
    Green,
    Black,
    Other,
}

/// Which of the test's colours a pixel is. Loose on purpose: a display
/// profile shifts a pure colour by a few steps.
fn colour(px: [u8; 4]) -> Colour {
    let (r, g, b) = (i32::from(px[0]), i32::from(px[1]), i32::from(px[2]));
    if r < 24 && g < 24 && b < 24 {
        Colour::Black
    } else if r > 180 && b > 180 && g < 90 {
        Colour::Magenta
    } else if g > 140 && r < 110 && b < 110 {
        Colour::Green
    } else {
        Colour::Other
    }
}

/// A pixel for a message. Only a pixel of one of the test's own
/// colours is printed as numbers; anything else could be the screen.
fn describe(px: [u8; 4]) -> String {
    match colour(px) {
        Colour::Other => "another colour".to_string(),
        known => format!("{known:?} ({}, {}, {})", px[0], px[1], px[2]),
    }
}

/// Capture once and read the test's points: the magenta ones first.
fn look(exclude: &[u32], paint: &[u32]) -> Result<ProbeResult, String> {
    probe_for_test(ProbeOptions {
        exclude_pids: exclude.to_vec(),
        paint_pids: paint.to_vec(),
        sample_points: IN_MAGENTA.iter().chain(IN_GREEN_ONLY.iter()).copied().collect(),
    })
}

/// Whether the process has a window on the screen, as the capture
/// code sees it: its window rectangles are what would be painted.
/// Reads no pixel.
fn on_screen(pid: u32) -> Result<bool, String> {
    let seen = probe_for_test(ProbeOptions { paint_pids: vec![pid], ..ProbeOptions::default() })?;
    Ok(seen.painted >= 1)
}

/// Capture, print what was seen at the test's points, and check it.
fn expect(case: &str, exclude: &[u32], paint: &[u32], where_magenta_is: Colour) -> ProbeResult {
    let seen = look(exclude, paint).unwrap_or_else(|e| panic!("{case}: the capture failed: {e}"));
    let (magenta, green) = seen.samples.split_at(IN_MAGENTA.len());
    let list = |pixels: &[[u8; 4]]| pixels.iter().map(|px| describe(*px)).collect::<Vec<_>>().join(", ");
    eprintln!(
        "{case}: {}x{} px, capture {} ms, excluded {}, painted {}\n  where the magenta window is: {}\n  green window beside it:      {}",
        seen.width,
        seen.height,
        seen.capture_ms,
        seen.excluded,
        seen.painted,
        list(magenta),
        list(green)
    );
    for (point, px) in IN_MAGENTA.iter().zip(magenta) {
        assert_eq!(colour(*px), where_magenta_is, "{case}: at {point:?}, in the magenta window, there is {}", describe(*px));
    }
    for (point, px) in IN_GREEN_ONLY.iter().zip(green) {
        assert_eq!(colour(*px), Colour::Green, "{case}: at {point:?}, in the green window, there is {}", describe(*px));
    }
    seen
}

#[test]
#[ignore = "puts two windows on the real screen; run with OPENWHISPER_VISION_LIVE=1 and --ignored"]
fn a_window_of_a_never_capture_app_is_not_in_the_image() {
    if std::env::var("OPENWHISPER_VISION_LIVE").as_deref() != Ok("1") {
        eprintln!("not run: set OPENWHISPER_VISION_LIVE=1 to put the test windows on the screen");
        return;
    }
    // Before any window: can this process capture at all? Without a
    // display or the permission the test fails here, saying which.
    // (No pixel is read here: there is no window of ours yet.)
    if let Err(e) = probe_for_test(ProbeOptions::default()) {
        panic!("the live pixel test could not look at the screen. {e}");
    }

    let green = Helper::start("00C800", GREEN, 1000);
    let magenta = Helper::start("FF00FF", MAGENTA, 1001);
    let hidden = [magenta.pid()];

    // The windows are ordered in; wait until the window list has
    // them both, and only then read a pixel: every point the test
    // looks at is then inside a window of its own.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let listed = |pid: u32| {
            on_screen(pid).unwrap_or_else(|e| panic!("the capture failed while waiting for the windows: {e}"))
        };
        if listed(green.pid()) && listed(magenta.pid()) {
            break;
        }
        assert!(Instant::now() < deadline, "the test windows did not show up on the screen within 10 seconds");
        std::thread::sleep(Duration::from_millis(100));
    }
    // One more moment for the window server to draw them.
    std::thread::sleep(Duration::from_millis(300));

    // (a) The control: nothing left out, nothing painted. The magenta
    // window is in the image, so the cases below mean something.
    let control = expect("control", &[], &[], Colour::Magenta);
    assert_eq!((control.excluded, control.painted), (0, 0));

    // (b) Left out of the capture: what is behind it shows. Only
    // where the platform can leave an app out.
    #[cfg(target_os = "macos")]
    {
        let excluded = expect("excluded", &hidden, &[], Colour::Green);
        assert_eq!(excluded.excluded, 1, "one process was to be left out");
        assert_eq!(excluded.painted, 0);
    }

    // (c) Not left out, painted over: black, and only there. On
    // Windows this is the one layer there is.
    let painted = expect("painted", &[], &hidden, Colour::Black);
    assert_eq!(painted.excluded, 0);
    assert!(painted.painted >= 1, "no rectangle was painted");

    // (d) Both, as in a real capture. On Windows asking to leave the
    // process out changes nothing: it is painted, and none is counted
    // as left out.
    let both = expect("excluded and painted", &hidden, &hidden, Colour::Black);
    assert_eq!(both.excluded, if cfg!(target_os = "macos") { 1 } else { 0 });
    assert!(both.painted >= 1);

    // The green helper was never aimed at and is still there.
    expect("control again", &[], &[], Colour::Magenta);
    drop(magenta);
    drop(green);
}

#[test]
fn the_test_colours_are_told_apart() {
    assert_eq!(colour([255, 0, 255, 255]), Colour::Magenta);
    assert_eq!(colour([251, 20, 240, 255]), Colour::Magenta);
    assert_eq!(colour([0, 200, 0, 255]), Colour::Green);
    assert_eq!(colour([40, 190, 30, 255]), Colour::Green);
    assert_eq!(colour([0, 0, 0, 255]), Colour::Black);
    assert_eq!(colour([128, 128, 128, 255]), Colour::Other);
    assert_eq!(colour([255, 255, 255, 255]), Colour::Other);
    assert_eq!(describe([0, 200, 0, 255]), "Green (0, 200, 0)");
    // A pixel that is none of the test's colours is not printed.
    assert_eq!(describe([12, 99, 201, 255]), "another colour");
    // Every point lies in the window it is meant for.
    let inside = |r: Rect, (x, y): (f64, f64)| x > r.x + 8.0 && x < r.x + r.width - 8.0 && y > r.y + 8.0 && y < r.y + r.height - 8.0;
    assert!(IN_MAGENTA.iter().all(|p| inside(MAGENTA, *p) && inside(GREEN, *p)));
    assert!(IN_GREEN_ONLY.iter().all(|p| inside(GREEN, *p) && !inside(MAGENTA, *p)));
    let grown = Rect { x: MAGENTA.x - 16.0, y: MAGENTA.y - 16.0, width: MAGENTA.width + 32.0, height: MAGENTA.height + 32.0 };
    assert!(IN_GREEN_ONLY.iter().all(|p| !inside(grown, *p)));
}
