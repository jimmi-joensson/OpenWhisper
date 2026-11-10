//! Focus context: which app the user is working in right now.
//!
//! One call, [`current`], returns the frontmost app's identifier and
//! name and the title of its focused window. Transcript history uses
//! it to record which app a dictation went into (TASK-95); the
//! default mode per app (TASK-96) uses [`current_app`], the same read
//! without the title, and [`running_apps`], the apps that are open,
//! to offer apps to set a default for. The ask, vision and agent
//! features build on it later (TASK-97 to TASK-99).
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
    finish(read_platform(true), std::process::id())
}

/// The frontmost app right now, without its window title
/// (`window_title` is empty). The title is the one part of a focus
/// read that asks another process and can wait for it; callers that
/// only need to know which app it is (the default mode per app, on
/// the path between the transcript and the paste) use this.
pub fn current_app() -> FocusContext {
    finish(read_platform(false), std::process::id())
}

/// The apps that are open now, as `(app_id, app_name)` pairs with the
/// same ids and names [`current`] reports, each app once, sorted by
/// name. For offering apps a default mode can be given to (TASK-96)
/// without any history of dictations. Never fails; empty when
/// nothing can be read.
///
/// - macOS: the running applications with a regular activation
///   policy (the ones with a Dock icon) and a bundle id.
/// - Windows: the processes that own a visible top-level window with
///   a title.
/// - Elsewhere: empty.
///
/// This process is left out: OpenWhisper's own windows never get a
/// default mode.
pub fn running_apps() -> Vec<(String, String)> {
    finish_running(read_running_platform())
}

#[cfg(target_os = "macos")]
fn read_platform(with_title: bool) -> FocusContext {
    mac::read(with_title)
}

#[cfg(target_os = "windows")]
fn read_platform(with_title: bool) -> FocusContext {
    windows::read(with_title)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn read_platform(_with_title: bool) -> FocusContext {
    FocusContext::default()
}

#[cfg(target_os = "macos")]
fn read_running_platform() -> Vec<(String, String)> {
    mac::running()
}

#[cfg(target_os = "windows")]
fn read_running_platform() -> Vec<(String, String)> {
    windows::running()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn read_running_platform() -> Vec<(String, String)> {
    Vec::new()
}

/// Tidy what the platform listed: ids trimmed, names tidied like
/// [`current`]'s (the id stands in for a missing name), entries
/// without an id dropped, each id once whatever its case (the first
/// one listed wins), sorted by name.
fn finish_running(apps: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (id, name) in apps {
        let id = id.trim().to_string();
        if id.is_empty() || out.iter().any(|(known, _)| known.to_lowercase() == id.to_lowercase()) {
            continue;
        }
        let name = tidy_name(&name);
        out.push((id.clone(), if name.is_empty() { id } else { name }));
    }
    out.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()).then_with(|| a.0.cmp(&b.0)));
    out
}

/// Tidy what the platform returned and mark our own process.
fn finish(mut ctx: FocusContext, own_pid: u32) -> FocusContext {
    ctx.app_id = ctx.app_id.trim().to_string();
    ctx.app_name = tidy_name(&ctx.app_name);
    ctx.window_title = ctx.window_title.trim().chars().take(MAX_TITLE_CHARS).collect();
    ctx.is_self = ctx.pid != 0 && ctx.pid == own_pid;
    ctx
}

/// Some apps ship a display name wrapped in invisible direction marks
/// (WhatsApp's starts with U+200E). They are noise in a file name, a
/// search or a list, so they go, together with outer whitespace.
fn tidy_name(name: &str) -> String {
    let invisible = |c: char| {
        c.is_whitespace()
            || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
    };
    name.trim_matches(invisible).to_string()
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
    fn current_app_is_the_same_app_without_the_title() {
        let app = current_app();
        assert_eq!(app.window_title, "");
        if app.is_self {
            assert_eq!(app.pid, std::process::id());
        }
    }

    #[test]
    fn running_apps_are_well_formed_and_leave_this_process_out() {
        let apps = running_apps();
        let mut ids: Vec<String> = apps.iter().map(|(id, _)| id.to_lowercase()).collect();
        assert!(apps.iter().all(|(id, name)| !id.trim().is_empty() && !name.trim().is_empty()), "{apps:?}");
        ids.sort();
        let listed = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), listed, "each app once: {apps:?}");
        // The test binary has no bundle id and no window, so it can
        // only be in the list by mistake.
        let own = std::env::current_exe().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
        assert!(!apps.iter().any(|(id, _)| Some(id.to_lowercase()) == own), "{apps:?}");
    }

    #[test]
    fn running_apps_are_listed_once_with_a_name_and_sorted_by_it() {
        let listed = finish_running(vec![
            (" com.example.notes ".into(), "Notes".into()),
            ("com.example.Chat".into(), "\u{200E}chatter".into()),
            ("COM.EXAMPLE.CHAT".into(), "Chatter helper".into()),
            ("   ".into(), "No id".into()),
            ("editor.exe".into(), "  ".into()),
            ("com.example.browser".into(), "Browser".into()),
        ]);
        assert_eq!(
            listed,
            vec![
                ("com.example.browser".to_string(), "Browser".to_string()),
                ("com.example.Chat".to_string(), "chatter".to_string()),
                // No name: the id stands in for it.
                ("editor.exe".to_string(), "editor.exe".to_string()),
                ("com.example.notes".to_string(), "Notes".to_string()),
            ]
        );
        assert!(finish_running(Vec::new()).is_empty());
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
    fn app_names_lose_invisible_marks() {
        assert_eq!(tidy_name("\u{200E}WhatsApp"), "WhatsApp");
        assert_eq!(tidy_name(" \u{FEFF}Notes\u{200F} "), "Notes");
        // Marks inside a name are left alone.
        assert_eq!(tidy_name("A\u{200D}B"), "A\u{200D}B");
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
