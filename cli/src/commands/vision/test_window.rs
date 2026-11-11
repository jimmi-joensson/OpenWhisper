//! `openwhisper vision test-window` (hidden): the helper window of the
//! live pixel test (`cli/tests/vision_live.rs`).
//!
//! Shows a borderless window of one solid colour above everything
//! else and stays until killed, or until its timeout. The test starts
//! two of these and checks what a capture shows where they are.
//!
//! This whole file is compiled only with the cargo feature
//! `vision-live-test`. A build without it has no such subcommand.

use std::time::Duration;

use anyhow::{Result, bail};
use clap::Args;

#[derive(Args, Debug)]
pub(super) struct TestWindowArgs {
    /// The window's colour, as RRGGBB.
    #[arg(long)]
    color: String,
    /// Left edge, in the coordinates Vision uses: global display
    /// points on macOS, virtual-desktop pixels on Windows.
    #[arg(long, allow_negative_numbers = true)]
    x: f64,
    /// Top edge, in the same coordinates (y grows downward).
    #[arg(long, allow_negative_numbers = true)]
    y: f64,
    #[arg(long)]
    width: f64,
    #[arg(long)]
    height: f64,
    /// Window level; a higher one is in front. macOS only: on Windows
    /// both helpers are topmost and the one started last is in front.
    #[arg(long, default_value_t = 1000)]
    level: i64,
    /// Close by itself after this many seconds, so a helper whose
    /// test died does not stay on the screen.
    #[arg(long, default_value_t = 120)]
    timeout_s: u64,
}

/// `RRGGBB` as red, green and blue.
fn parse_colour(hex: &str) -> Result<(u8, u8, u8)> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("a colour is six hex digits, RRGGBB; got \"{hex}\"");
    }
    let part = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16);
    Ok((part(0)?, part(2)?, part(4)?))
}

/// What every platform checks before it makes a window.
fn checked(args: &TestWindowArgs) -> Result<(u8, u8, u8)> {
    let colour = parse_colour(&args.color)?;
    if !(args.width >= 1.0 && args.height >= 1.0) {
        bail!("the window needs a width and a height of at least 1");
    }
    Ok(colour)
}

/// A helper whose test was killed must not stay on the screen.
fn close_after(seconds: u64) {
    let timeout = Duration::from_secs(seconds.max(1));
    std::thread::spawn(move || {
        std::thread::sleep(timeout);
        std::process::exit(0);
    });
}

#[cfg(target_os = "macos")]
pub(super) fn run(args: TestWindowArgs) -> Result<()> {
    use std::io::Write;

    use anyhow::anyhow;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSScreen, NSWindow,
        NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    let (red, green, blue) = checked(&args)?;
    let mtm = MainThreadMarker::new().ok_or_else(|| anyhow!("the test window must be made on the main thread"))?;
    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon, no menu bar, never the active app.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    // The arguments are in the coordinates Vision uses: origin at the
    // top left of the primary display, y downward. AppKit counts from
    // the bottom left of that display, upward.
    let screens = NSScreen::screens(mtm);
    let primary = screens.firstObject().ok_or_else(|| anyhow!("no display"))?;
    let primary_height = primary.frame().size.height;
    let rect = NSRect::new(
        NSPoint::new(args.x, primary_height - args.y - args.height),
        NSSize::new(args.width, args.height),
    );
    // SAFETY: a plain NSWindow made on the main thread, kept alive by
    // this function for the life of the process.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            rect,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: the window is owned by `window` here, not by AppKit's
    // close handling.
    unsafe { window.setReleasedWhenClosed(false) };
    let part = |v: u8| f64::from(v) / 255.0;
    window.setBackgroundColor(Some(&NSColor::colorWithSRGBRed_green_blue_alpha(
        part(red),
        part(green),
        part(blue),
        1.0,
    )));
    window.setOpaque(true);
    window.setHasShadow(false);
    window.setIgnoresMouseEvents(true);
    window.setLevel(args.level as isize);
    window.orderFrontRegardless();

    close_after(args.timeout_s);
    println!("ready");
    std::io::stdout().flush().ok();
    app.run();
    Ok(())
}

/// Windows: a borderless, topmost popup filled with one colour. Not
/// run on a real machine yet; written so the live pixel test can be
/// run there (see `cli/tests/vision_live.rs`).
///
/// - The process is made per-monitor DPI aware first, so the
///   arguments are real pixels of the virtual desktop, the unit
///   Vision reports window rectangles in.
/// - `WS_EX_TOPMOST` puts it above normal windows; of two such
///   helpers the one started last is in front, which is the order
///   the test starts them in. `--level` is not used.
/// - `WS_EX_TOOLWINDOW` keeps it off the taskbar, `WS_EX_NOACTIVATE`
///   and `SW_SHOWNOACTIVATE` keep it from taking the keyboard focus:
///   the app in front stays the one it was.
/// - The colour is the window class's background brush, so the
///   system paints it; there is no drawing code.
#[cfg(target_os = "windows")]
pub(super) fn run(args: TestWindowArgs) -> Result<()> {
    use std::io::Write;

    use anyhow::anyhow;
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{CreateSolidBrush, UpdateWindow};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HWND_TOPMOST, MSG, PostQuitMessage,
        RegisterClassW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowPos,
        ShowWindow, TranslateMessage, WM_DESTROY, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        WS_POPUP,
    };
    use windows::core::w;

    unsafe extern "system" fn on_message(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // SAFETY: the arguments are the ones the system called with.
        unsafe {
            if message == WM_DESTROY {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
    }

    let (red, green, blue) = checked(&args)?;
    // SAFETY (this block): Win32 calls with values this function owns;
    // the class name is a static string, and the window lives until
    // the process ends.
    unsafe {
        // Real pixels from here on. Older systems refuse the value,
        // and the window is then merely scaled.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None).map_err(|e| anyhow!("this program's module handle: {e}"))?;
        // A COLORREF is 0x00BBGGRR.
        let colour = COLORREF(u32::from(red) | u32::from(green) << 8 | u32::from(blue) << 16);
        let brush = CreateSolidBrush(colour);
        if brush.is_invalid() {
            bail!("the colour could not be made into a brush");
        }
        let class_name = w!("OpenWhisperVisionTestWindow");
        let class = WNDCLASSW {
            lpfnWndProc: Some(on_message),
            hInstance: instance.into(),
            hbrBackground: brush,
            lpszClassName: class_name,
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            bail!("the window class could not be registered");
        }
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!(""),
            WS_POPUP,
            args.x.round() as i32,
            args.y.round() as i32,
            args.width.round() as i32,
            args.height.round() as i32,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| anyhow!("the test window could not be made: {e}"))?;
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        // Painted now, not when the message loop gets to it.
        let _ = UpdateWindow(hwnd);

        close_after(args.timeout_s);
        println!("ready");
        std::io::stdout().flush().ok();

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(super) fn run(args: TestWindowArgs) -> Result<()> {
    checked(&args)?;
    let _ = (args.x, args.y, args.level);
    close_after(args.timeout_s);
    bail!("the test window is only available on macOS and Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_six_hex_digits() {
        assert_eq!(parse_colour("FF00ff").unwrap(), (255, 0, 255));
        assert_eq!(parse_colour("#00C800").unwrap(), (0, 200, 0));
        for bad in ["", "fff", "gggggg", "ff00ff00"] {
            assert!(parse_colour(bad).is_err(), "{bad}");
        }
    }
}
