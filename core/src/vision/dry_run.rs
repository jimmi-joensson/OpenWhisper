//! What a capture would do right now, found out without taking one.
//!
//! A screenshot exists only inside an ask ([`super`]). So the question
//! "would Vision work here, and what would it leave out" cannot be
//! answered by taking a screenshot to see: there is no ask. This file
//! answers it by looking at everything a capture looks at, except the
//! pixels: the permission, the display, the app in front, the apps on
//! the never-capture list and their windows.
//!
//! Nothing in here captures, prompts or needs the permission.

use serde::Serialize;

use super::{AppSelector, DisplayInfo, NoScreenshot, PermissionState, Platform, PlatformError, id_matches};
use crate::settings::{NeverCaptureApp, VisionSettings};

/// An app that counts as being in front, and whether it is protected.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct FrontSeen {
    pub id: String,
    pub name: String,
    /// The app is on the never-capture list (itself or as a helper of
    /// a listed app): with it in front there is no screenshot.
    pub listed: bool,
}

/// An app on the never-capture list, as it is right now.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct ListedApp {
    pub id: String,
    pub name: String,
    /// It is open: among the running apps, or it has a window on
    /// screen.
    pub running: bool,
    /// How many windows it has on screen: the rectangles a capture
    /// would paint over. `None` when the windows could not be read.
    pub windows: Option<usize>,
}

/// What [`dry_run`] found. No field holds a pixel or a window title.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DryRun {
    /// Whether this process may record the screen.
    pub permission: PermissionState,
    /// The display a capture would take. `None` when there is none or
    /// it could not be told; `display_problem` then says why.
    pub display: Option<DisplayInfo>,
    pub display_problem: String,
    /// The apps that count as being in front. Empty when they could
    /// not be read; `front_problem` then says why.
    pub front: Vec<FrontSeen>,
    pub front_problem: String,
    /// Every app on the never-capture list in force.
    pub listed: Vec<ListedApp>,
    /// Why the windows of the listed apps could not be read; empty
    /// when they could.
    pub windows_problem: String,
    /// Why a capture would be refused right now, as far as that can
    /// be told without trying: the first of the reasons a capture
    /// checks, in its order. `None`: nothing stands in the way.
    pub refusal: Option<NoScreenshot>,
}

/// Look at everything a capture for an ask would look at, in the same
/// order, and take no screenshot.
///
/// Reads the permission, the display that would be captured, the apps
/// in front and the windows of the apps on the never-capture list of
/// `settings`. Asks the system which apps and windows there are, so
/// call it off the UI thread. Safe to call at any time, with or
/// without the permission: it never captures and never prompts.
pub fn dry_run(settings: &VisionSettings) -> DryRun {
    let running = crate::focus::running_apps();
    // Unit tests look at the fake a test installed, like a capture.
    #[cfg(test)]
    if let Some(fake) = super::test_support::installed() {
        return dry_run_with(&*fake, settings, &running);
    }
    dry_run_with(&super::system(), settings, &running)
}

/// [`dry_run`] against a given platform. `running`: id and name of
/// the apps that are open.
pub(crate) fn dry_run_with(platform: &dyn Platform, settings: &VisionSettings, running: &[(String, String)]) -> DryRun {
    let never: Vec<NeverCaptureApp> = settings.clone().cleaned().effective_never_capture();
    let permission = platform.permission();

    let (display, display_problem, no_display) = match platform.target_display() {
        Ok(display) => (Some(display), String::new(), false),
        Err(PlatformError::NoDisplay) => (None, NoScreenshot::NoDisplay.message(), true),
        Err(PlatformError::Failed(why)) => (None, why, false),
    };

    let (front, front_problem) = match platform.frontmost_apps() {
        Ok(apps) if !apps.is_empty() => (
            apps.into_iter()
                .map(|app| FrontSeen { listed: never.iter().any(|l| id_matches(&app.id, &l.id)), id: app.id, name: app.name })
                .collect(),
            String::new(),
        ),
        Ok(_) => (Vec::new(), "no app is in front".to_string()),
        Err(why) => (Vec::new(), why),
    };

    let mut windows_problem = String::new();
    let listed: Vec<ListedApp> = never
        .iter()
        .map(|app| {
            let ids = [app.id.clone()];
            let windows = match platform.window_rects(AppSelector::Ids(&ids)) {
                Ok(rects) => Some(rects.len()),
                Err(why) => {
                    windows_problem = why;
                    None
                }
            };
            let open = running.iter().any(|(id, _)| id_matches(id, &app.id));
            ListedApp {
                id: app.id.clone(),
                name: app.name.clone(),
                running: open || windows.is_some_and(|n| n > 0),
                windows,
            }
        })
        .collect();

    // The reasons a capture checks, in its order.
    let protected = front.iter().find(|app| app.listed).map(|app| {
        let name = match never.iter().find(|l| id_matches(&app.id, &l.id)) {
            Some(entry) => super::capture::label(entry, &app.name),
            None => app.name.clone(),
        };
        NoScreenshot::ProtectedAppInFront { name }
    });
    let refusal = if permission != PermissionState::Granted {
        Some(NoScreenshot::NoPermission)
    } else if front.is_empty() {
        Some(NoScreenshot::FrontAppUnknown)
    } else if protected.is_some() {
        protected
    } else if !windows_problem.is_empty() {
        Some(NoScreenshot::ProtectionFailed { detail: windows_problem.clone() })
    } else if no_display {
        Some(NoScreenshot::NoDisplay)
    } else if display.is_none() {
        Some(NoScreenshot::CaptureFailed { detail: display_problem.clone() })
    } else {
        None
    };

    DryRun { permission, display, display_problem, front, front_problem, listed, windows_problem, refusal }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::Fake;
    use super::super::{FrontApp, PointRect};
    use super::*;
    use crate::settings::VisionTrigger;

    const VAULT: &str = "com.example.vault";

    fn settings() -> VisionSettings {
        VisionSettings {
            trigger: VisionTrigger::Always,
            phrases: None,
            never_capture: Some(vec![
                NeverCaptureApp::new(VAULT, "Vault"),
                NeverCaptureApp::new("com.example.notrunning", "Not Running"),
                NeverCaptureApp::new("com.example.menubar", ""),
            ]),
        }
    }

    fn rect(x: f64, y: f64) -> PointRect {
        PointRect { x, y, width: 100.0, height: 80.0 }
    }

    fn open(apps: &[&str]) -> Vec<(String, String)> {
        apps.iter().map(|id| (id.to_string(), String::new())).collect()
    }

    #[test]
    fn a_dry_run_reports_everything_a_capture_looks_at_and_never_captures() {
        let fake = Fake {
            windows: vec![Ok(vec![
                (VAULT.to_string(), rect(10.0, 10.0)),
                ("com.example.vault.helper".to_string(), rect(200.0, 10.0)),
                ("com.example.browser".to_string(), rect(50.0, 50.0)),
            ])],
            ..Fake::default()
        };
        let seen = dry_run_with(&fake, &settings(), &open(&["com.example.editor", "com.example.menubar"]));
        assert_eq!(seen.permission, PermissionState::Granted);
        // 400 x 300 points at 2x.
        let display = seen.display.unwrap();
        assert_eq!((display.width, display.height), (800, 600));
        assert_eq!((display.point_width, display.point_height, display.x, display.y), (400.0, 300.0, 0.0, 0.0));
        assert_eq!(seen.front, [FrontSeen { id: "com.example.editor".into(), name: "Editor".into(), listed: false }]);
        assert_eq!(
            seen.listed,
            [
                // Open because it has windows; its helper's window counts as its own.
                ListedApp { id: VAULT.into(), name: "Vault".into(), running: true, windows: Some(2) },
                ListedApp { id: "com.example.notrunning".into(), name: "Not Running".into(), running: false, windows: Some(0) },
                // Open without a window on screen.
                ListedApp { id: "com.example.menubar".into(), name: String::new(), running: true, windows: Some(0) },
            ]
        );
        assert_eq!(seen.refusal, None);
        assert_eq!((seen.display_problem.as_str(), seen.front_problem.as_str(), seen.windows_problem.as_str()), ("", "", ""));
        // The whole point: not one capture.
        assert_eq!(fake.captures(), 0);
        assert!(!fake.calls().contains(&"capture"), "{:?}", fake.calls());
        assert!(fake.calls().contains(&"display"));
    }

    #[test]
    fn a_dry_run_names_the_reason_a_capture_would_be_refused_for() {
        // No permission: said first, and the rest is still reported.
        let fake = Fake { permission: PermissionState::Denied, ..Fake::default() };
        let seen = dry_run_with(&fake, &settings(), &[]);
        assert_eq!(seen.refusal, Some(NoScreenshot::NoPermission));
        assert!(seen.display.is_some() && !seen.front.is_empty());
        assert_eq!(fake.captures(), 0);

        // A protected app in front, also as a helper, and by the name
        // on the list.
        let fake = Fake {
            front: vec![Ok(vec![
                FrontApp { id: "com.example.editor".into(), name: "Editor".into(), pid: 1 },
                FrontApp { id: "COM.Example.Vault-Launcher".into(), name: "Vault Launcher".into(), pid: 2 },
            ])],
            ..Fake::default()
        };
        let seen = dry_run_with(&fake, &settings(), &[]);
        assert_eq!(seen.refusal, Some(NoScreenshot::ProtectedAppInFront { name: "Vault".into() }));
        assert_eq!(seen.front.iter().map(|app| app.listed).collect::<Vec<_>>(), [false, true]);

        // The app in front cannot be read.
        for front in [Err("no answer".to_string()), Ok(Vec::new())] {
            let fake = Fake { front: vec![front], ..Fake::default() };
            let seen = dry_run_with(&fake, &settings(), &[]);
            assert_eq!(seen.refusal, Some(NoScreenshot::FrontAppUnknown));
            assert!(seen.front.is_empty() && !seen.front_problem.is_empty());
        }

        // The windows cannot be read: no count, and a refusal.
        let fake = Fake { windows: vec![Err("the window list could not be read".into())], ..Fake::default() };
        let seen = dry_run_with(&fake, &settings(), &open(&[VAULT]));
        assert_eq!(seen.refusal, Some(NoScreenshot::ProtectionFailed { detail: "the window list could not be read".into() }));
        assert_eq!(seen.listed[0], ListedApp { id: VAULT.into(), name: "Vault".into(), running: true, windows: None });

        // No display, and a display that cannot be measured.
        let fake = Fake { capture: Err(PlatformError::NoDisplay), ..Fake::default() };
        let seen = dry_run_with(&fake, &settings(), &[]);
        assert_eq!((seen.display, seen.refusal.clone()), (None, Some(NoScreenshot::NoDisplay)));
        assert!(seen.display_problem.contains("no display is on"));
        let fake = Fake { capture: Err(PlatformError::Failed("the display could not be measured".into())), ..Fake::default() };
        let seen = dry_run_with(&fake, &settings(), &[]);
        assert_eq!(seen.refusal, Some(NoScreenshot::CaptureFailed { detail: "the display could not be measured".into() }));
        assert_eq!(fake.captures(), 0);
    }

    #[test]
    fn a_dry_run_uses_the_list_in_force_and_says_when_it_is_empty() {
        // Never edited: the built-in list.
        let seen = dry_run_with(&Fake::default(), &VisionSettings::default(), &[]);
        assert_eq!(seen.listed.len(), crate::settings::default_never_capture().len());
        // Emptied by the person: nothing is protected, nothing is
        // asked about windows, and nothing is refused.
        let emptied = VisionSettings { never_capture: Some(Vec::new()), ..VisionSettings::default() };
        let fake = Fake { front: vec![Fake::front_app(VAULT, "Vault")], ..Fake::default() };
        let seen = dry_run_with(&fake, &emptied, &[]);
        assert!(seen.listed.is_empty());
        assert_eq!(seen.refusal, None);
        assert!(!fake.calls().contains(&"rects"));
    }

    #[test]
    fn the_report_serializes_without_anything_of_the_screen() {
        let seen = dry_run_with(&Fake::default(), &settings(), &[]);
        let value = serde_json::to_value(&seen).unwrap();
        let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["display", "display_problem", "front", "front_problem", "listed", "permission", "refusal", "windows_problem"]
        );
        assert_eq!(value["permission"], "granted");
        assert_eq!(value["display"]["width"], 800);
        assert_eq!(value["refusal"], serde_json::Value::Null);
        let blocked = Fake { front: vec![Fake::front_app(VAULT, "")], ..Fake::default() };
        let value = serde_json::to_value(dry_run_with(&blocked, &settings(), &[])).unwrap();
        assert_eq!(value["refusal"], serde_json::json!({ "reason": "protected-app-in-front", "name": "Vault" }));
    }

    /// The public entry point against the real system: it must come
    /// back with a report on any machine, display on or asleep,
    /// permission or not, and it has no way to a capture.
    #[test]
    fn the_real_dry_run_reads_the_system_without_capturing() {
        let seen = dry_run(&VisionSettings::default());
        assert_eq!(seen.listed.len(), crate::settings::default_never_capture().len());
        // Either the display was measured or there is a reason.
        assert!(seen.display.is_some() || !seen.display_problem.is_empty());
        assert!(!seen.front.is_empty() || !seen.front_problem.is_empty());
    }
}
