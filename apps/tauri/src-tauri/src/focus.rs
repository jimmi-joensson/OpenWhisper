//! Window focus helpers + AX-grant watcher (Mac).
//!
//! `bring_main_to_front` shows + focuses the main window. Used at
//! permission-prompt moments so the user sees OW's banners alongside the
//! OS dialogs — without it, an `accessory`-policy app stays behind
//! whatever else owned focus when the prompt fired.
//!
//! The AX watcher polls `AXIsProcessTrusted()` because there is no
//! Apple notification API for TCC trust transitions. Granting AX
//! happens in System Settings while OW is in the background — the
//! watcher catches the false → true edge and brings main forward so the
//! user sees the Restart banner. Edge-only firing (no focus theft on
//! every tick).

use tauri::AppHandle;

#[cfg(target_os = "macos")]
use tauri::Manager;

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

/// Show + unminimize + focus the main window. Tauri's `set_focus` calls
/// `[NSApp activateIgnoringOtherApps:YES]` underneath, which works for
/// accessory-policy apps even without a Dock icon.
#[cfg(target_os = "macos")]
pub fn bring_main_to_front(app: &AppHandle) {
    let Some(main) = app.get_webview_window("main") else {
        return;
    };
    let _ = main.show();
    let _ = main.unminimize();
    let _ = main.set_focus();
}

/// Spawn the AX trust watcher. No-op on non-Mac platforms — Windows
/// has no equivalent prompt-after-grant flow.
pub fn install_ax_watcher(app: AppHandle) {
    #[cfg(target_os = "macos")]
    {
        std::thread::Builder::new()
            .name("openwhisper-ax-watcher".into())
            .spawn(move || ax_watch_loop(app))
            .expect("spawn AX watcher");
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
    }
}

#[cfg(target_os = "macos")]
fn ax_watch_loop(app: AppHandle) {
    use std::time::Duration;
    // Seed the state so we don't fire on the first tick if AX was
    // already granted at boot.
    let mut last = unsafe { AXIsProcessTrusted() };
    loop {
        std::thread::sleep(Duration::from_millis(1500));
        let now = unsafe { AXIsProcessTrusted() };
        if now == last {
            continue;
        }
        if now {
            // Edge false → true. The user just granted AX (typically in
            // System Settings while OW was in the background). Bring main
            // forward so the Restart banner is visible. We do NOT auto-
            // restart — TCC's kernel cache requires a relaunch before
            // CGEventTapCreate succeeds, so the user clicks Restart and
            // we do `app.restart()` from the hotkey_retry command path.
            eprintln!("[ax-watcher] AX trust granted — bringing main forward");
            bring_main_to_front(&app);
        } else {
            // Edge true → false (revoked). No focus action — the
            // hotkey watchdog will re-emit an error banner if needed.
            eprintln!("[ax-watcher] AX trust revoked");
        }
        last = now;
    }
}
