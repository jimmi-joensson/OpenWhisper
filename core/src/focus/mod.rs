//! Focus context: which app the user is working in right now.
//!
//! One call, [`current`], returns the frontmost app's identifier and
//! name and the title of its focused window. Transcript history uses
//! it to record which app a dictation went into (TASK-95); a default
//! mode per app and the ask, vision and agent features build on it
//! later (TASK-96 to TASK-99).
//!
//! Every field is best effort. A piece that cannot be read (no
//! permission, a process that has gone away, an app that does not
//! answer in time) is left empty; the call never fails and never
//! waits longer than a short timeout, so it is safe on the dictation
//! path.
//!
//! - macOS: app id and name from `NSWorkspace` (no permission). The
//!   window title comes through the Accessibility API and is empty
//!   until the app has the Accessibility grant.
//! - Windows: the foreground window's owning process. The app id is
//!   the exe file name in lower case (`slack.exe`), the name is the
//!   file's description.
//! - Elsewhere: an empty context.
//!
//! This module only reads names. It never captures the screen or the
//! text inside a window.

use serde::Serialize;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "windows")]
mod windows;

/// Longest window title kept. Browser tabs can carry a whole page
/// title; nothing downstream needs more than this.
const MAX_TITLE_CHARS: usize = 300;

/// What the user is working in. See the module docs for what each
/// platform fills in.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusContext {
    /// Stable identifier of the app: the bundle id on macOS
    /// (`com.tinyspeck.slackmacgap`), the exe file name in lower case
    /// on Windows (`slack.exe`). Empty when unknown.
    pub app_id: String,
    /// Name to show a person ("Slack"). Empty when unknown.
    pub app_name: String,
    /// Title of the app's focused window. Empty when unknown or not
    /// permitted.
    pub window_title: String,
    /// Process id of the frontmost app; 0 when unknown.
    pub pid: u32,
    /// The frontmost app is this process (one of OpenWhisper's own
    /// windows has focus).
    pub is_self: bool,
}

impl FocusContext {
    /// Nothing could be read.
    pub fn is_empty(&self) -> bool {
        self.app_id.is_empty() && self.app_name.is_empty() && self.pid == 0
    }
}

/// The frontmost app right now. Never fails; see the module docs.
pub fn current() -> FocusContext {
    finish(read_platform(), std::process::id())
}

#[cfg(target_os = "macos")]
fn read_platform() -> FocusContext {
    mac::read()
}

#[cfg(target_os = "windows")]
fn read_platform() -> FocusContext {
    windows::read()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn read_platform() -> FocusContext {
    FocusContext::default()
}

/// Tidy what the platform returned and mark our own process.
fn finish(mut ctx: FocusContext, own_pid: u32) -> FocusContext {
    ctx.app_id = ctx.app_id.trim().to_string();
    ctx.app_name = ctx.app_name.trim().to_string();
    ctx.window_title = ctx.window_title.trim().chars().take(MAX_TITLE_CHARS).collect();
    ctx.is_self = ctx.pid != 0 && ctx.pid == own_pid;
    ctx
}

/// `C:\Users\me\AppData\Local\slack\app-4.1\Slack.exe` -> `slack.exe`.
/// Handles both separators so it can be tested off Windows.
#[cfg_attr(not(any(test, target_os = "windows")), allow(dead_code))]
fn exe_file_name(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_lowercase()
}

/// `C:\...\Slack.exe` -> `Slack`: the fallback app name when the exe
/// carries no description.
#[cfg_attr(not(any(test, target_os = "windows")), allow(dead_code))]
fn exe_stem(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or_default().trim();
    match file.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.eq_ignore_ascii_case("exe") => stem.to_string(),
        _ => file.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_never_panics_and_is_consistent() {
        let ctx = current();
        // On a build machine there may be no frontmost app at all; the
        // only promise is a well-formed answer.
        assert!(ctx.window_title.chars().count() <= MAX_TITLE_CHARS);
        if ctx.is_self {
            assert_eq!(ctx.pid, std::process::id());
        }
    }

    #[test]
    fn finish_trims_and_caps() {
        let ctx = finish(
            FocusContext {
                app_id: " com.example.app ".into(),
                app_name: " Example\n".into(),
                window_title: "x".repeat(MAX_TITLE_CHARS + 50),
                pid: 42,
                is_self: false,
            },
            7,
        );
        assert_eq!(ctx.app_id, "com.example.app");
        assert_eq!(ctx.app_name, "Example");
        assert_eq!(ctx.window_title.chars().count(), MAX_TITLE_CHARS);
        assert!(!ctx.is_self);
    }

    #[test]
    fn finish_marks_own_process() {
        let ctx = finish(FocusContext { pid: 7, ..Default::default() }, 7);
        assert!(ctx.is_self);
        // An unknown pid is never "us".
        let ctx = finish(FocusContext::default(), 0);
        assert!(!ctx.is_self);
    }

    #[test]
    fn empty_context_reports_empty() {
        assert!(FocusContext::default().is_empty());
        assert!(!FocusContext { app_name: "Notes".into(), ..Default::default() }.is_empty());
    }

    #[test]
    fn exe_names_from_windows_paths() {
        let path = r"C:\Users\me\AppData\Local\slack\app-4.1\Slack.exe";
        assert_eq!(exe_file_name(path), "slack.exe");
        assert_eq!(exe_stem(path), "Slack");
        assert_eq!(exe_file_name("notepad.exe"), "notepad.exe");
        assert_eq!(exe_stem("C:/Tools/my.tool.EXE"), "my.tool");
        assert_eq!(exe_stem("C:/Tools/noext"), "noext");
        assert_eq!(exe_file_name(""), "");
    }
}
