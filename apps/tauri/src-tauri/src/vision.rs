//! Vision in the shell (TASK-98.4): the commands behind the "Screen"
//! section of the Ask settings. Thin wrappers: core owns the settings
//! block and its rules (`openwhisper_core::settings`), the permission
//! check (`openwhisper_core::vision`) and the screenshot itself, which
//! only an ask can take (`openwhisper_core::ask`).
//!
//! # What is not here, on purpose
//!
//! No command takes a screenshot, returns image data or starts a
//! capture. A screenshot exists only inside an ask: the ask session
//! in core takes it, hands it to the backend and drops it. The image
//! never reaches a webview; the windows only hear *that* the screen
//! was looked at (`AskSnapshot::screen` on `ask_tick`). The tests at
//! the bottom of this file hold that line: the commands of this
//! module are exactly [`COMMANDS`], and nothing in the shell names
//! core's capture functions.
//!
//! The same settings are reachable headless through
//! `openwhisper vision …`.
//!
//! # Windows
//!
//! Screen capture on Windows has not been verified on a real machine
//! yet. Until it has, Vision is off there unless the person switches
//! it on (core's default trigger), and [`VisionState`] says so
//! (`capture_verified`) for the Screen section to show a plain
//! notice. Windows cannot leave an app out of a capture, so this
//! process's own overlay windows are marked as excluded from capture
//! by their owner, here ([`exclude_own_overlays`]).

use std::sync::{Mutex, MutexGuard};

use openwhisper_core::focus;
use openwhisper_core::settings::{
    self as core_settings, default_never_capture, default_phrases, NeverCaptureApp, VisionSettings, VisionTrigger,
};
use openwhisper_core::vision::{self, PermissionState};
use tauri::{AppHandle, Emitter};
#[cfg(target_os = "windows")]
use tauri::Manager;

use crate::settings::settings_path;

/// Emitted to every window after the Vision settings changed.
/// Payload: the stored [`VisionSettings`].
pub(crate) const VISION_CHANGED_EVENT: &str = "vision_changed";

/// Every command of this module. None of them captures anything.
#[cfg(test)]
const COMMANDS: [&str; 5] = [
    "vision_get",
    "vision_set",
    "vision_request_permission",
    "vision_open_settings",
    "vision_running_apps",
];

/// What the Screen section loads: the stored block, the lists in
/// force, the built-in lists, and whether this process may record the
/// screen. Reading it captures nothing and prompts for nothing.
#[derive(serde::Serialize)]
pub struct VisionState {
    /// The stored block. A list that is `null` is the built-in one.
    pub settings: VisionSettings,
    /// The phrases in force: the person's list, or the built-in one.
    pub phrases: Vec<String>,
    /// The never-capture list in force.
    pub never_capture: Vec<NeverCaptureApp>,
    /// The lists Vision ships with on this platform.
    pub default_phrases: Vec<String>,
    pub default_never_capture: Vec<NeverCaptureApp>,
    /// The list in force is the built-in one, entry for entry.
    pub phrases_are_default: bool,
    pub never_capture_is_default: bool,
    /// Checked without asking for it.
    pub permission: PermissionState,
    /// `macos`, `windows` or `other`. Only macOS has a permission to
    /// give.
    pub platform: &'static str,
    /// Screen capture has been checked on a real machine of this
    /// platform. `false` on Windows for now: the Screen section then
    /// says so, says what protects password managers there, and
    /// Vision is off until the person switches it on.
    pub capture_verified: bool,
    /// The trigger of a person who never chose one, on this platform.
    pub default_trigger: VisionTrigger,
}

/// Whether capture has been verified on a real machine of the
/// platform. A function of the platform so both answers are tested
/// everywhere; flip the Windows answer when the live pixel test has
/// passed there (`cli/tests/vision_live.rs`).
fn capture_verified_on(windows: bool) -> bool {
    !windows
}

fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "other"
    }
}

fn state(settings: VisionSettings, permission: PermissionState) -> VisionState {
    let phrases = settings.effective_phrases();
    let never_capture = settings.effective_never_capture();
    let default_phrases = default_phrases();
    let default_never_capture = default_never_capture();
    VisionState {
        phrases_are_default: phrases == default_phrases,
        never_capture_is_default: never_capture == default_never_capture,
        settings,
        phrases,
        never_capture,
        default_phrases,
        default_never_capture,
        permission,
        platform: platform(),
        capture_verified: capture_verified_on(cfg!(target_os = "windows")),
        default_trigger: core_settings::default_trigger(),
    }
}

/// Held by everything in this process that puts a Vision block into
/// core's cache: a save and a fresh read. Without it a read that
/// started before a save could finish after it and leave the cache
/// (which the next ask reads) holding the block from before the save:
/// a trigger the person just switched off would still capture. Same
/// reason as the locks in `ask.rs` and `modes.rs`.
fn vision_lock() -> MutexGuard<'static, ()> {
    static VISION: Mutex<()> = Mutex::new(());
    VISION.lock().unwrap_or_else(|e| e.into_inner())
}

fn announce(app: &AppHandle, now: &VisionSettings) {
    if let Err(e) = app.emit(VISION_CHANGED_EVENT, now) {
        eprintln!("[vision] {VISION_CHANGED_EVENT} emit failed: {e}");
    }
}

/// Boot-time: tell core where the settings file is, and load the
/// Vision block into core's cache.
///
/// An ask reads the Vision block, and what an ask tells the model
/// about the computer, from the file every time it starts
/// (`fresh_vision_settings`, `fresh_ask_context`): the headless CLI
/// edits the same file while the app runs, and a rule set there has
/// to hold for the next ask. For that core must know the path. The
/// cache is what an ask falls back to when the file says nothing,
/// and what the windows are shown.
pub fn init(app: &AppHandle) {
    if let Ok(path) = settings_path(app) {
        core_settings::set_settings_file(Some(&path));
        let _held = vision_lock();
        let _ = core_settings::load_vision_settings(&path);
    }
    exclude_own_overlays(app);
}

/// This app's own windows that sit on top of other apps: the pill and
/// the answer panel.
const OWN_OVERLAYS: [&str; 2] = ["pill", crate::ask::ANSWER_LABEL];

/// Windows: keep the pill and the answer panel out of every capture
/// of the screen.
///
/// macOS leaves this process's windows out of an ask's screenshot in
/// the capture itself. Windows has no such thing for the one who
/// captures; what it has is a switch on the window's own side,
/// `SetWindowDisplayAffinity` with `WDA_EXCLUDEFROMCAPTURE`: a window
/// marked with it is absent from any capture, and what is behind it
/// shows. So the two overlays are marked once, when the app starts
/// (they are made from the config before `setup` runs and live as
/// long as the app).
///
/// - Needs Windows 10 version 2004. On an older build the call fails
///   and the overlays are in the image, as before; the failure is
///   logged and nothing else changes.
/// - It holds for every capture, not only ours: a screenshot or a
///   screen share made by another program does not show the pill or
///   the answer panel either.
/// - The main window is not marked: it is an ordinary window, and a
///   person must be able to show it to someone.
///
/// Does nothing on other platforms.
pub fn exclude_own_overlays(app: &AppHandle) {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE};
        for label in OWN_OVERLAYS {
            let Some(window) = app.get_webview_window(label) else {
                continue;
            };
            match window.hwnd() {
                // SAFETY: a handle of a live window of this process.
                Ok(hwnd) => unsafe {
                    if let Err(e) = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) {
                        eprintln!("[vision] the {label} window could not be kept out of captures: {e}");
                    }
                },
                Err(e) => eprintln!("[vision] {label} window handle: {e}"),
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, OWN_OVERLAYS);
    }
}

/// Fresh read from disk (the block is also edited by the headless
/// CLI), with the lists in force, the built-in lists and the
/// permission state. Takes no screenshot and shows no prompt. Off the
/// UI thread for the file read.
#[tauri::command]
pub async fn vision_get(app: AppHandle) -> Result<VisionState, String> {
    let path = settings_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (before, settings) = {
            let _held = vision_lock();
            (core_settings::current_vision_settings(), core_settings::load_vision_settings(&path))
        };
        // The file was changed behind this process's back.
        if before != settings {
            announce(&app, &settings);
        }
        state(settings, vision::permission())
    })
    .await
    .map_err(|e| format!("vision settings: {e}"))
}

/// Save the whole block; returns what core stored (lists tidied).
/// Saving puts the block in core's cache, so the next ask uses it.
/// Emits `vision_changed`.
#[tauri::command]
pub fn vision_set(app: AppHandle, settings: VisionSettings) -> Result<VisionSettings, String> {
    let path = settings_path(&app)?;
    let saved = {
        let _held = vision_lock();
        core_settings::save_vision_settings(&path, settings)?
    };
    announce(&app, &saved);
    Ok(saved)
}

/// Ask the system for the right to record the screen, because the
/// person pressed the button for it, and answer with the state after
/// that. macOS shows its prompt the first time and nothing later; a
/// grant given now only counts after a restart, so the answer can
/// still be `denied`. On a blocking thread: the call can wait for
/// the system.
#[tauri::command]
pub async fn vision_request_permission() -> Result<PermissionState, String> {
    tauri::async_runtime::spawn_blocking(|| {
        if vision::request_permission() {
            PermissionState::Granted
        } else {
            vision::permission()
        }
    })
    .await
    .map_err(|e| format!("vision permission: {e}"))
}

/// Open System Settings on Privacy & Security, Screen Recording.
/// Returns whether a pane was opened: `false` on Windows, where there
/// is nothing to grant.
#[tauri::command]
pub fn vision_open_settings() -> bool {
    #[cfg(target_os = "macos")]
    {
        match crate::open_pref_pane("com.apple.preference.security?Privacy_ScreenCapture") {
            Ok(()) => true,
            Err(e) => {
                eprintln!("[vision] open Screen Recording settings: {e}");
                false
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// The apps that are open now, as id and name, for adding one to the
/// never-capture list. Ids are in lower case, like the ids the list
/// holds. Asks the OS which apps run, so off the UI thread.
#[tauri::command]
pub async fn vision_running_apps() -> Result<Vec<NeverCaptureApp>, String> {
    tauri::async_runtime::spawn_blocking(|| running(focus::running_apps()))
        .await
        .map_err(|e| format!("vision apps: {e}"))
}

fn running(apps: Vec<(String, String)>) -> Vec<NeverCaptureApp> {
    apps.into_iter().map(|(id, name)| NeverCaptureApp::new(&id.to_lowercase(), &name)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This file up to its tests: the part that ships.
    fn shipped() -> &'static str {
        let source = include_str!("vision.rs");
        let end = source.find("#[cfg(test)]\nmod tests").expect("tests module");
        &source[..end]
    }

    /// The names of the functions marked `#[tauri::command]` in
    /// `source`, in order.
    fn command_names(source: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut rest = source;
        while let Some(at) = rest.find("#[tauri::command") {
            rest = &rest[at + 1..];
            let Some(f) = rest.find("fn ") else { break };
            let name: String =
                rest[f + 3..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            names.push(name);
        }
        names
    }

    /// The entries of `generate_handler![…]` in lib.rs, comments left
    /// out.
    fn registered() -> Vec<String> {
        let lib = include_str!("lib.rs");
        let start = lib.find("generate_handler![").expect("handler list") + "generate_handler![".len();
        let end = start + lib[start..].find(']').expect("end of handler list");
        let list: String = lib[start..end]
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let entries: Vec<String> =
            list.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        // The whole list was read, not a part of it.
        assert!(entries.len() > 80 && entries.iter().any(|e| e == "ask::ask_snapshot"), "{entries:?}");
        entries
    }

    #[test]
    fn the_commands_of_this_module_are_the_five_listed_and_all_are_registered() {
        assert_eq!(command_names(shipped()), COMMANDS);
        let registered: Vec<String> = registered()
            .into_iter()
            .filter_map(|entry| entry.strip_prefix("vision::").map(str::to_string))
            .collect();
        assert_eq!(registered, COMMANDS);
    }

    #[test]
    fn no_command_takes_a_screenshot_or_hands_out_an_image() {
        // By name: nothing the webview can call is about a capture.
        // (`settings_capture_hotkey_*` records a key press, not the
        // screen, hence the exact words.)
        for entry in registered() {
            let name = entry.rsplit("::").next().unwrap_or(&entry).to_lowercase();
            for word in ["screenshot", "screen_capture", "capture_screen", "vision_capture", "vision_probe", "image", "snapshot_screen"] {
                assert!(!name.contains(word), "{entry} sounds like a capture command");
            }
        }
        for name in COMMANDS {
            for word in ["capture", "shot", "probe", "image", "png"] {
                assert!(!name.contains(word), "{name}");
            }
        }

        // By what is called: the shell never names core's capture
        // entry points, the token an ask captures with, or the image
        // types. Only `core::ask` takes a screenshot.
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        let mut dirs = vec![src];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        assert!(files.len() > 10, "{files:?}");
        // Put together here so this test does not find itself.
        let banned: Vec<String> = [
            ("capture_", "for_ask"),
            ("probe_", "for_test"),
            ("AskCapture", "Token"),
            ("Raw", "Capture"),
            ("vision::", "Screenshot"),
            ("Ask", "Image"),
        ]
        .iter()
        .map(|(a, b)| format!("{a}{b}"))
        .collect();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            for word in &banned {
                assert!(!text.contains(word.as_str()), "{} names {word}", file.display());
            }
        }
    }

    #[test]
    fn the_state_payload_is_the_block_the_lists_in_force_and_the_permission() {
        let value =
            serde_json::to_value(state(VisionSettings::default(), PermissionState::Denied)).unwrap();
        let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        // The fields `VisionState` in src/lib/vision.ts mirrors.
        assert_eq!(
            keys,
            [
                "capture_verified",
                "default_never_capture",
                "default_phrases",
                "default_trigger",
                "never_capture",
                "never_capture_is_default",
                "permission",
                "phrases",
                "phrases_are_default",
                "platform",
                "settings",
            ]
        );
        // Verified on macOS, not yet on Windows; and the trigger a
        // person never chose is the platform's.
        assert_eq!(value["capture_verified"], !cfg!(target_os = "windows"));
        assert_eq!(value["default_trigger"], core_settings::default_trigger().as_str());
        // Nothing edited: the stored lists are null, the lists in
        // force are the built-in ones.
        assert_eq!(
            value["settings"],
            serde_json::json!({
                "trigger": core_settings::default_trigger().as_str(),
                "phrases": null,
                "never_capture": null,
            })
        );
        assert_eq!(value["phrases"], value["default_phrases"]);
        assert_eq!(value["never_capture"], value["default_never_capture"]);
        assert_eq!(value["phrases_are_default"], true);
        assert_eq!(value["never_capture_is_default"], true);
        assert_eq!(value["permission"], "denied");
        assert!(["macos", "windows", "other"].contains(&value["platform"].as_str().unwrap()));
        let first = &value["never_capture"][0];
        assert!(first["id"].is_string() && first["name"].is_string(), "{first}");
    }

    #[test]
    fn an_edited_list_is_not_the_default_and_an_emptied_one_stays_empty() {
        let edited = VisionSettings {
            trigger: VisionTrigger::Smart,
            phrases: Some(vec!["look here".into()]),
            never_capture: Some(Vec::new()),
        };
        let s = state(edited, PermissionState::Granted);
        assert!(!s.phrases_are_default);
        assert!(!s.never_capture_is_default);
        assert_eq!(s.phrases, ["look here"]);
        assert!(s.never_capture.is_empty());
        // The built-in lists are still offered, for "Restore defaults".
        assert_eq!(s.default_phrases, default_phrases());
        assert_eq!(s.default_never_capture, default_never_capture());

        // A list written out in full that equals the built-in one
        // counts as the default: there is nothing to restore.
        let same = VisionSettings { phrases: Some(default_phrases()), ..VisionSettings::default() };
        assert!(state(same, PermissionState::Granted).phrases_are_default);
    }

    #[test]
    fn windows_is_not_verified_yet_and_starts_switched_off() {
        assert!(capture_verified_on(false));
        assert!(!capture_verified_on(true));
        // The two go together: where capture is not verified, a
        // person who never chose a trigger has Vision off.
        assert_eq!(core_settings::default_trigger_on(true), VisionTrigger::Off);
        assert_eq!(core_settings::default_trigger_on(false), VisionTrigger::Phrases);
    }

    #[test]
    fn the_overlays_are_kept_out_of_captures_on_windows_when_the_app_starts() {
        // The two windows that sit over other apps, by the labels the
        // config gives them.
        assert_eq!(OWN_OVERLAYS, ["pill", crate::ask::ANSWER_LABEL]);
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let labels: Vec<&str> =
            conf["app"]["windows"].as_array().unwrap().iter().filter_map(|w| w["label"].as_str()).collect();
        for label in OWN_OVERLAYS {
            assert!(labels.contains(&label), "{label} is not a window of the app");
        }
        // The main window is not one of them.
        assert!(!OWN_OVERLAYS.contains(&"main"));
        // The mark is the one that removes a window from a capture
        // (not the one that blacks it out), set on Windows only, and
        // `init`, which `setup` calls, sets it.
        let source = shipped();
        let function = &source[source.find("pub fn exclude_own_overlays").expect("the function")..];
        assert!(function.contains("SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)"));
        assert!(function.contains("#[cfg(target_os = \"windows\")]"));
        let init = &source[source.find("pub fn init(").expect("init")..];
        let init = &init[..init.find("\n}\n").expect("the end of init")];
        assert!(init.contains("exclude_own_overlays(app);"), "{init}");
        assert!(init.contains("set_settings_file(Some(&path))"), "{init}");
        assert!(include_str!("lib.rs").contains("vision::init(app.handle());"));
    }

    #[test]
    fn the_changed_payload_is_the_stored_block() {
        let value = serde_json::to_value(VisionSettings {
            trigger: VisionTrigger::Always,
            phrases: Some(vec!["look here".into()]),
            never_capture: Some(vec![NeverCaptureApp::new("com.example.vault", "Vault")]),
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "trigger": "always",
                "phrases": ["look here"],
                "never_capture": [{ "id": "com.example.vault", "name": "Vault" }],
            })
        );
    }

    #[test]
    fn open_apps_are_offered_with_the_ids_the_list_holds() {
        let apps = running(vec![
            ("com.Example.Notes".into(), "Notes".into()),
            ("vault.exe".into(), "Vault".into()),
        ]);
        assert_eq!(
            apps,
            [NeverCaptureApp::new("com.example.notes", "Notes"), NeverCaptureApp::new("vault.exe", "Vault")]
        );
    }

    #[test]
    fn the_permission_state_is_sent_as_a_word() {
        for (state, word) in [
            (PermissionState::Granted, "granted"),
            (PermissionState::Denied, "denied"),
            (PermissionState::Unknown, "unknown"),
        ] {
            assert_eq!(serde_json::to_value(state).unwrap(), word);
        }
    }
}
