//! Pill click target = the visible capsule, not the whole window.
//!
//! The pill window is 180×110 logical points (room for the 1.5× scale
//! and the shadow) but the idle capsule is only 38×22. A window either
//! accepts or ignores all mouse events, so while idle the whole
//! transparent window used to swallow clicks meant for the Dock and
//! apps behind it.
//!
//! Now the window ignores mouse events by default. A main-thread poll
//! (every [`POLL`]) checks whether the cursor is over the capsule and
//! accepts events only then (`OPENWHISPER_VERBOSE` logs each switch as
//! `[ow.pill-hover]`), so a click on the pill still raises the
//! main window. Outside idle (recording, transcribing) the window stays
//! click-through, as before.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager};

/// Idle capsule size in logical points (PillOverlay.tsx `PILL_OUTER_W.idle`
/// and PillOverlay.css `.pill-capsule` height).
const CAPSULE_IDLE_W: f64 = 38.0;
/// Pill window width in logical points (tauri.conf.json, `place_pill`).
const PILL_WIN_W: f64 = 180.0;
const CAPSULE_H: f64 = 22.0;
/// Extra logical points around the capsule that still count as a hit,
/// so the edge of the pill isn't a near miss.
const HIT_PAD: f64 = 4.0;
const POLL: Duration = Duration::from_millis(50);

/// Whether the pill state allows clicks at all (true only while idle).
static CLICKABLE: AtomicBool = AtomicBool::new(true);
/// Last value passed to `set_ignore_cursor_events`: 0 unknown,
/// 1 ignoring, 2 accepting. Avoids a native call per poll.
static APPLIED: AtomicU8 = AtomicU8::new(0);

/// Called from `set_pill_click_through` when the pill status changes.
pub fn set_clickable(app: &AppHandle, clickable: bool) {
    CLICKABLE.store(clickable, Ordering::Relaxed);
    reconcile_on_main(app);
}

/// Start the hover poll. Call once at boot.
pub fn install(app: AppHandle) {
    thread::Builder::new()
        .name("openwhisper-pill-hover".into())
        .spawn(move || loop {
            thread::sleep(POLL);
            reconcile_on_main(&app);
        })
        .expect("spawn pill hover");
}

fn reconcile_on_main(app: &AppHandle) {
    let handle = app.clone();
    // Cursor and window geometry come from AppKit on macOS, which is
    // main-thread-only.
    let _ = app.run_on_main_thread(move || reconcile(&handle));
}

fn reconcile(app: &AppHandle) {
    let Some(pill) = app.get_webview_window("pill") else {
        return;
    };
    let accept = CLICKABLE.load(Ordering::Relaxed)
        && pill.is_visible().unwrap_or(false)
        && cursor_over_capsule(app, &pill).unwrap_or(false);
    let want = if accept { 2 } else { 1 };
    if APPLIED.swap(want, Ordering::Relaxed) != want {
        openwhisper_core::verbose_log!(
            "[ow.pill-hover] {}",
            if accept { "accepting clicks" } else { "ignoring clicks" }
        );
        if let Err(e) = pill.set_ignore_cursor_events(!accept) {
            APPLIED.store(0, Ordering::Relaxed);
            eprintln!("[pill-hover] set_ignore_cursor_events: {e}");
        }
    }
}

fn cursor_over_capsule(app: &AppHandle, pill: &tauri::WebviewWindow) -> tauri::Result<bool> {
    let cursor = app.cursor_position()?;
    let pos = pill.outer_position()?;
    let size = pill.outer_size()?;
    Ok(hit(
        (cursor.x, cursor.y),
        (pos.x as f64, pos.y as f64),
        (size.width as f64, size.height as f64),
    ))
}

/// Hit test in whatever pixel unit the window reports: the unit per
/// logical point comes from the window's own width (180 pt), so it holds
/// on 1× and 2× displays alike. Is `cursor` within the padded capsule
/// centered in the window at `pos` with `size`?
fn hit(cursor: (f64, f64), pos: (f64, f64), size: (f64, f64)) -> bool {
    let unit = size.0 / PILL_WIN_W;
    let cx = pos.0 + size.0 / 2.0;
    let cy = pos.1 + size.1 / 2.0;
    let half_w = (CAPSULE_IDLE_W / 2.0 + HIT_PAD) * unit;
    let half_h = (CAPSULE_H / 2.0 + HIT_PAD) * unit;
    (cursor.0 - cx).abs() <= half_w && (cursor.1 - cy).abs() <= half_h
}

#[cfg(test)]
mod tests {
    use super::hit;

    // 180×110 window at (1000, 500) on a 2× display: center (1180, 610).
    const POS: (f64, f64) = (1000.0, 500.0);
    const SIZE: (f64, f64) = (360.0, 220.0);

    #[test]
    fn capsule_center_and_padded_edge_hit() {
        assert!(hit((1180.0, 610.0), POS, SIZE));
        // 19 + 4 = 23 pt half-width → 46 px at 2×.
        assert!(hit((1180.0 + 46.0, 610.0), POS, SIZE));
    }

    #[test]
    fn one_x_display_uses_points() {
        // Same window at 1×: 180×110 px, center (1090, 555), 23 px half-width.
        assert!(hit((1090.0 + 23.0, 555.0), POS, (180.0, 110.0)));
        assert!(!hit((1090.0 + 24.0, 555.0), POS, (180.0, 110.0)));
    }

    #[test]
    fn transparent_window_area_misses() {
        assert!(!hit((1180.0 + 47.0, 610.0), POS, SIZE));
        // Top-left corner of the window, well outside the capsule.
        assert!(!hit((1010.0, 510.0), POS, SIZE));
        // 11 + 4 = 15 pt half-height → 30 px at 2×.
        assert!(!hit((1180.0, 610.0 + 31.0), POS, SIZE));
    }
}
