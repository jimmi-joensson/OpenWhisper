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
//! 4. Sleep — duration depends on the receiving app. See `restore_delay`:
//!    200 ms for sync readers (Mac, Win32 native, non-Chromium), 2 s for
//!    Chromium/Electron foreground on Windows (issue #6).
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
#[cfg(target_os = "windows")]
use arboard::SetExtWindows;
use openwhisper_core::dictation::Injector;
use openwhisper_core::verbose_log;
use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "windows")]
mod windows;

/// Short restore window. Used when the receiving control reads the
/// clipboard synchronously on Ctrl/Cmd+V — native Win32 (Notepad, cmd,
/// Windows Terminal, WinForms), Cocoa (Safari, Mail, TextEdit, Xcode),
/// and non-Chromium browsers all complete their read well inside this.
const RESTORE_DELAY_FAST: Duration = Duration::from_millis(200);

/// Long restore window. Required when the foreground app is a
/// Chromium/Electron host (Slack, Outlook, browsers, Discord, VSCode,
/// Teams) — those read multiple clipboard formats asynchronously after
/// Ctrl+V, and restoring too soon races their reads so the paste stalls
/// or retries (GitHub issue #6). SuperWhisper uses 3 s for the same
/// reason; 2 s is a 10× margin over Chromium's observed read window.
/// Only relevant on Windows — gated so the macOS build doesn't warn.
#[cfg(target_os = "windows")]
const RESTORE_DELAY_CHROMIUM: Duration = Duration::from_millis(2000);

/// Pick the restore delay based on the foreground app. Goal: only pay
/// the long Chromium-async-read margin where it's actually needed, so
/// Mac users (no Chromium read-race bug at all) and Windows users in
/// native apps (Terminal, Notepad) get their clipboard back at 200 ms
/// instead of 2 s.
fn restore_delay() -> Duration {
    #[cfg(target_os = "windows")]
    {
        let chromium = windows::foreground_is_chromium();
        let d = if chromium {
            RESTORE_DELAY_CHROMIUM
        } else {
            RESTORE_DELAY_FAST
        };
        verbose_log!(
            "[ow.inject] foreground_chromium={chromium} restore_delay_ms={}",
            d.as_millis()
        );
        d
    }
    #[cfg(not(target_os = "windows"))]
    {
        verbose_log!(
            "[ow.inject] platform=mac restore_delay_ms={}",
            RESTORE_DELAY_FAST.as_millis()
        );
        RESTORE_DELAY_FAST
    }
}

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
    let t_total = std::time::Instant::now();
    let mut clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("inject: clipboard init failed: {e}");
            return;
        }
    };

    let saved = clipboard.get_text().ok();

    let t_set = std::time::Instant::now();
    if let Err(e) = set_clipboard_text(&mut clipboard, text.to_string()) {
        eprintln!("inject: clipboard set failed: {e}");
        return;
    }
    let set_ms = t_set.elapsed().as_millis();

    if is_self_frontmost(&app) {
        // Manual-fallback path. Transcript stays in the clipboard so the
        // user can paste it wherever they want. Don't restore the saved
        // clipboard — that would clobber the manual fallback.
        verbose_log!("[ow.inject] self_frontmost=true clipboard_set_ms={set_ms} (manual fallback)");
        return;
    }

    let t_synth = std::time::Instant::now();
    synthesize_paste();
    let synth_ms = t_synth.elapsed().as_millis();

    let delay = restore_delay();
    thread::sleep(delay);

    let t_restore = std::time::Instant::now();
    if let Some(prev) = saved {
        if let Err(e) = set_clipboard_text(&mut clipboard, prev) {
            eprintln!("inject: clipboard restore failed: {e}");
        }
    }
    let restore_ms = t_restore.elapsed().as_millis();

    verbose_log!(
        "[ow.inject] clipboard_set_ms={set_ms} synth_ms={synth_ms} \
         restore_sleep_ms={} clipboard_restore_ms={restore_ms} total_ms={} chars={}",
        delay.as_millis(),
        t_total.elapsed().as_millis(),
        text.len()
    );
}

/// Set clipboard text, opting out of Windows clipboard-history (Win+V),
/// cloud-clipboard sync, and clipboard-monitor processing on Windows.
/// Both the transcript and the restore go through this so neither shows
/// up in clipboard managers — same approach SuperWhisper shipped in v2.10
/// ("prevent pollution of clipboard managers") and Wispr Flow documents.
fn set_clipboard_text(clipboard: &mut Clipboard, text: String) -> Result<(), arboard::Error> {
    #[cfg(target_os = "windows")]
    {
        clipboard.set().exclude_from_monitoring().text(text)
    }
    #[cfg(not(target_os = "windows"))]
    {
        clipboard.set_text(text)
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
