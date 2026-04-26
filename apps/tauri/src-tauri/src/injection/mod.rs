//! Text injection — paste a transcript into whatever app is focused.
//!
//! Ports `apps/macos/App/TextInjector.swift` (and the equivalent C# in
//! `apps/windows/OpenWhisper/TextInjection/TextInjector.cs`) to a
//! cross-platform Rust path:
//!
//! 1. Save current clipboard text (text-only — non-text formats are lossy
//!    on both OSes; same as the WinUI 3 shipped behavior).
//! 2. Set clipboard to the transcript.
//! 3. Synthesize Cmd+V (Mac) / Ctrl+V (Win) into the focused app.
//! 4. Sleep 200 ms so the target reads the pasteboard.
//! 5. Restore the saved clipboard.
//!
//! Self-frontmost guard: if our own main/pill window is focused at paste
//! time, **skip the paste AND skip the restore**. Transcript stays in the
//! clipboard so the user can Cmd+V it manually wherever they want. This is
//! the manual-fallback path the user explicitly asked for in Phase 5.
//!
//! AX permission on Mac is already requested by `hotkey::install` —
//! `CGEventPost` for Cmd+V uses the same grant, no second prompt.

use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use openwhisper_core::dictation::Injector;
use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "windows")]
mod windows;

/// Matches `TextInjector.swift`'s 200 ms restore delay. Shorter intervals
/// occasionally clobber the paste — the target hasn't finished reading
/// the pasteboard yet.
const RESTORE_DELAY: Duration = Duration::from_millis(200);

/// Tauri-side `Injector` impl. Registered with the core at boot via
/// `dictation::set_injector`. Core calls `inject(text)` after a transcript
/// is delivered.
pub struct TauriInjector {
    app: AppHandle,
}

impl TauriInjector {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl Injector for TauriInjector {
    fn inject(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        let app = self.app.clone();
        let text = text.to_string();
        thread::Builder::new()
            .name("openwhisper-inject".into())
            .spawn(move || do_inject(app, &text))
            .expect("spawn injection thread");
    }
}

fn do_inject(app: AppHandle, text: &str) {
    let mut clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("inject: clipboard init failed: {e}");
            return;
        }
    };

    let saved = clipboard.get_text().ok();

    if let Err(e) = clipboard.set_text(text.to_string()) {
        eprintln!("inject: clipboard set failed: {e}");
        return;
    }

    if is_self_frontmost(&app) {
        // Manual-fallback path. Transcript stays in the clipboard so the
        // user can paste it wherever they want. Don't restore the saved
        // clipboard — that would clobber the manual fallback.
        return;
    }

    synthesize_paste();

    thread::sleep(RESTORE_DELAY);

    if let Some(prev) = saved {
        if let Err(e) = clipboard.set_text(prev) {
            eprintln!("inject: clipboard restore failed: {e}");
        }
    }
}

/// True when one of our own windows is the focus target. The pill is
/// non-activating so it should never report focused, but check it anyway —
/// costs nothing and protects against future window-config drift.
fn is_self_frontmost(app: &AppHandle) -> bool {
    let focused = |label: &str| {
        app.get_webview_window(label)
            .and_then(|w| w.is_focused().ok())
            .unwrap_or(false)
    };
    focused("main") || focused("pill")
}

#[cfg(target_os = "macos")]
fn synthesize_paste() {
    mac::synthesize_paste();
}

#[cfg(target_os = "windows")]
fn synthesize_paste() {
    windows::synthesize_paste();
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn synthesize_paste() {}
