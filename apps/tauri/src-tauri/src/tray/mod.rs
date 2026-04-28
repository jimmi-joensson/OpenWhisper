//! System tray icon — phase-aware mic glyph in menu bar (Mac) / system tray (Win).
//!
//! Idle = template-tinted mono mic. Recording = orange (#E07000). Tooltip
//! reflects current phase. Right-click context menu offers Open / Toggle /
//! Quit. Double-click (Win) or left-click (Mac is its own dance — left-click
//! opens menu by default) brings the main window forward.
//!
//! Glyph is rasterized once at startup from the same 26-rect list as
//! `apps/macos/App/OpenWhisperApp.swift:242-269` and
//! `apps/windows/OpenWhisper/Tray/StatusIconRenderer.cs`. One source of
//! truth means a glyph change is a one-line edit per shell.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use openwhisper_core::dictation::{self, PHASE_RECORDING};
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuEvent, MenuItem, MenuItemBuilder};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::{do_toggle, TICK_MS};

/// Mic glyph rect list — keep in sync with the Swift / C# / SVG copies.
/// Source: `OpenWhisperApp.swift` `StatusIconRenderer.micRects`.
const MIC_RECTS: &[(u32, u32, u32, u32)] = &[
    (204, 188, 64, 64),
    (204, 284, 64, 64),
    (204, 380, 64, 64),
    (204, 476, 64, 64),
    (204, 700, 64, 64),
    (268, 28, 64, 64),
    (268, 92, 256, 64),
    (268, 188, 64, 64),
    (268, 284, 64, 64),
    (268, 380, 64, 64),
    (268, 476, 256, 64),
    (268, 700, 256, 64),
    (364, 28, 64, 64),
    (364, 156, 64, 320),
    (364, 572, 64, 64),
    (364, 636, 64, 64),
    (460, 28, 64, 64),
    (460, 188, 64, 64),
    (460, 284, 64, 64),
    (460, 380, 64, 64),
    (524, 92, 64, 64),
    (524, 188, 64, 64),
    (524, 284, 64, 64),
    (524, 380, 64, 64),
    (524, 476, 64, 64),
    (524, 700, 64, 64),
];
const VIEW_BOX: f32 = 792.0;

/// Tray bitmap size — rasterized at 2× the shipped 18-pt logical menubar
/// size so the 12×12 mic-rect grid lands on integer pixel boundaries (3 px
/// per grid cell). Tauri hands this to NSStatusItem as an NSImage with
/// logical-size = pixel-size; the button cell scales 0.5× down to fit the
/// 22-pt menubar, yielding a crisp 1:1 retina render.
///
/// Apps/macos sidesteps this by drawing into NSImage via a closure that
/// the system invokes at the live device scale — vector-style, no fixed
/// pixel buffer. We can't do that through Tauri's `Image` API, so we
/// ship a high-enough-resolution raster instead.
const ICON_SIZE: u32 = 36;

/// Render the mic glyph as a Tauri [`Image`] (raw RGBA) at `size × size`.
/// Uses 4× supersampling + a 4×4 box filter to smooth the rect edges; the
/// 64-px-wide grid cells in `MIC_RECTS` rasterize to clean 3-px squares at
/// ICON_SIZE=36, so AA matters mostly at glyph perimeters.
fn render_glyph(size: u32, rgba: [u8; 4]) -> Image<'static> {
    const SUPER: u32 = 4;
    let hi = size * SUPER;
    let scale = hi as f32 / VIEW_BOX;

    let mut hi_buf = vec![0u8; (hi * hi * 4) as usize];
    for &(rx, ry, rw, rh) in MIC_RECTS {
        let x0 = (rx as f32 * scale).round() as u32;
        let y0 = (ry as f32 * scale).round() as u32;
        let x1 = ((rx + rw) as f32 * scale).round().min(hi as f32) as u32;
        let y1 = ((ry + rh) as f32 * scale).round().min(hi as f32) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * hi + x) * 4) as usize;
                hi_buf[i] = rgba[0];
                hi_buf[i + 1] = rgba[1];
                hi_buf[i + 2] = rgba[2];
                hi_buf[i + 3] = rgba[3];
            }
        }
    }

    let mut buf = vec![0u8; (size * size * 4) as usize];
    let n = (SUPER * SUPER) as u32;
    for y in 0..size {
        for x in 0..size {
            let mut sums = [0u32; 4];
            for dy in 0..SUPER {
                for dx in 0..SUPER {
                    let sx = x * SUPER + dx;
                    let sy = y * SUPER + dy;
                    let i = ((sy * hi + sx) * 4) as usize;
                    sums[0] += hi_buf[i] as u32;
                    sums[1] += hi_buf[i + 1] as u32;
                    sums[2] += hi_buf[i + 2] as u32;
                    sums[3] += hi_buf[i + 3] as u32;
                }
            }
            let i = ((y * size + x) * 4) as usize;
            buf[i] = (sums[0] / n) as u8;
            buf[i + 1] = (sums[1] / n) as u8;
            buf[i + 2] = (sums[2] / n) as u8;
            buf[i + 3] = (sums[3] / n) as u8;
        }
    }
    Image::new_owned(buf, size, size)
}

/// Idle: opaque black mic. Tauri / OS apply menubar tint on Mac (template
/// behavior) when the icon is monochrome black with alpha. Win shows it raw,
/// which on the dark default taskbar is also fine.
fn idle_icon() -> Image<'static> {
    render_glyph(ICON_SIZE, [0, 0, 0, 255])
}

/// Recording: orange #E07000, opaque. Stays vivid regardless of OS theme —
/// matches the shipped Mac behavior (`OpenWhisperApp.swift:289-300`).
fn recording_icon() -> Image<'static> {
    render_glyph(ICON_SIZE, [0xE0, 0x70, 0x00, 0xFF])
}

fn open_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn open_settings(app: &AppHandle) {
    // Settings is an in-window route — bring main forward and emit
    // `ow_navigate` so the React tree swaps to the Settings shell.
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    let _ = app.emit("ow_navigate", "settings");
}

fn quit(app: &AppHandle) {
    app.exit(0);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    LoadingModel,
    Recording,
    Transcribing,
}

impl Phase {
    fn from_core(phase: u32) -> Self {
        use openwhisper_core::dictation::{
            PHASE_LOADING_MODEL, PHASE_RECORDING, PHASE_TRANSCRIBING,
        };
        match phase {
            PHASE_LOADING_MODEL => Self::LoadingModel,
            PHASE_RECORDING => Self::Recording,
            PHASE_TRANSCRIBING => Self::Transcribing,
            _ => Self::Idle,
        }
    }

    fn tooltip(self, app_name: &str) -> String {
        match self {
            Self::Idle => app_name.to_string(),
            Self::LoadingModel => format!("{app_name} — loading model…"),
            Self::Recording => format!("{app_name} — recording"),
            Self::Transcribing => format!("{app_name} — transcribing…"),
        }
    }

    fn dictation_label(self) -> &'static str {
        match self {
            Self::Idle => "Start Dictation",
            Self::LoadingModel => "Loading model…",
            Self::Recording => "Stop Dictation",
            Self::Transcribing => "Transcribing…",
        }
    }

    fn dictation_enabled(self) -> bool {
        // Mirror DictationService.isInteractable on Mac:
        // toggle is gated to idle + recording, never the in-flight phases.
        matches!(self, Self::Idle | Self::Recording)
    }
}

/// IDs returned by [`build_menu`] so the menu-event handler can tell which
/// item fired without string-matching scattered constants.
struct MenuIds {
    open: String,
    toggle: String,
    preferences: String,
    quit: String,
}

/// Build the right-click context menu. Rebuilt on every phase change so the
/// "Start / Stop / Loading…" label + enabled state stay current. Cheap
/// enough — menu has 4 items + separators.
fn build_menu(
    app: &AppHandle,
    phase: Phase,
    app_name: &str,
) -> tauri::Result<(tauri::menu::Menu<Wry>, MenuIds)> {
    let ids = MenuIds {
        open: "ow.open".into(),
        toggle: "ow.toggle".into(),
        preferences: "ow.preferences".into(),
        quit: "ow.quit".into(),
    };

    let open_item: MenuItem<Wry> =
        MenuItemBuilder::with_id(&ids.open, format!("Open {app_name}")).build(app)?;
    let toggle_item: MenuItem<Wry> = MenuItemBuilder::with_id(&ids.toggle, phase.dictation_label())
        .enabled(phase.dictation_enabled())
        .build(app)?;
    let prefs_item: MenuItem<Wry> = MenuItemBuilder::with_id(&ids.preferences, "Preferences…")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;
    let quit_item: MenuItem<Wry> = MenuItemBuilder::with_id(&ids.quit, format!("Quit {app_name}"))
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&open_item)
        .separator()
        .item(&toggle_item)
        .separator()
        .item(&prefs_item)
        .separator()
        .item(&quit_item)
        .build()?;
    Ok((menu, ids))
}

/// Install the system tray + spawn the phase-watcher.
///
/// Phase-watcher runs in its own thread and polls `dictation_snapshot` at
/// the same 50 ms cadence as the dictation_tick emitter — cheap (atomics
/// inside the core), and means we don't have to plumb an event listener
/// onto the main thread.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let app_name = crate::product_name(app);
    let initial_phase = Phase::from_core(dictation::dictation_snapshot().phase());

    let (menu, ids) = build_menu(app, initial_phase, &app_name)?;
    let ids = Arc::new(ids);
    let ids_for_handler = Arc::clone(&ids);

    // Tray is owned by tauri's runtime via the registered id; we look it up
    // later via `app.tray_by_id("ow.tray")` rather than holding the builder
    // return value.
    let _tray = TrayIconBuilder::with_id("ow.tray")
        .icon(idle_icon())
        .icon_as_template(true) // Mac: tint to match menu bar
        .tooltip(initial_phase.tooltip(&app_name))
        .menu(&menu)
        // Mac: left-click opens menu (matches shipped SwiftUI app).
        // Win: left-click is no-op, right-click opens menu, double-click
        // opens main window (matches shipped WinUI 3 app).
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(move |app, event: MenuEvent| {
            let id = event.id().as_ref();
            let ids = &*ids_for_handler;
            if id == ids.open {
                open_main(app);
            } else if id == ids.toggle {
                if let Err(e) = do_toggle() {
                    eprintln!("tray toggle failed: {e}");
                }
            } else if id == ids.preferences {
                open_settings(app);
            } else if id == ids.quit {
                quit(app);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                open_main(tray.app_handle());
            }
        })
        .build(app)?;

    let app_handle = app.clone();
    let icon_idle = Arc::new(idle_icon());
    let icon_rec = Arc::new(recording_icon());
    let app_name_for_thread = app_name.clone();

    thread::Builder::new()
        .name("openwhisper-tray-watcher".into())
        .spawn(move || {
            let mut last = initial_phase;
            loop {
                thread::sleep(Duration::from_millis(TICK_MS));
                let snap = dictation::dictation_snapshot();
                let now = Phase::from_core(snap.phase());
                if now == last {
                    continue;
                }
                last = now;

                if let Some(tray) = app_handle.tray_by_id("ow.tray") {
                    let icon = if snap.phase() == PHASE_RECORDING {
                        (*icon_rec).clone()
                    } else {
                        (*icon_idle).clone()
                    };
                    let _ = tray.set_icon(Some(icon));
                    let _ = tray.set_icon_as_template(snap.phase() != PHASE_RECORDING);
                    let _ = tray.set_tooltip(Some(now.tooltip(&app_name_for_thread)));

                    if let Ok((menu, _)) = build_menu(&app_handle, now, &app_name_for_thread) {
                        let _ = tray.set_menu(Some(menu));
                    }
                }
            }
        })
        .expect("spawn tray watcher");

    Ok(())
}
