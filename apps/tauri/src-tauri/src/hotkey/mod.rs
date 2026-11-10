//! Global hotkeys.
//!
//! The slots are a table, [`SLOTS`]: toggle (start/stop dictation),
//! cancel (discard the current recording or stop a running ask, only
//! while there is one), the optional cycle-mode slot (switch to the
//! next mode) and the optional ask slot (start an ask instead of a
//! dictation, TASK-97). Defaults: Right ⌘ / Esc on mac, Ctrl+Space /
//! Esc on Windows; cycle-mode is unbound until the user records a
//! key; ask is Right ⌥ on mac and Ctrl+Shift+Space on Windows, and is
//! only installed while Ask is switched on ([`effective_settings`]).
//! All rebindable via Settings → Shortcuts. A new slot is one more
//! table entry plus its action; neither backend names a slot.
//!
//! Per-platform because activation gestures differ:
//! - **Mac**: `CGEventTap`. Every slot supports both modifier-tap
//!   (default Right Cmd: tap-not-hold) and chord.
//! - **Windows**: `WH_KEYBOARD_LL` hook, chord-only for every slot.
//!
//! Status surface: `hotkey_status` Tauri event (see [`HotkeyStatus`]) and
//! the `hotkey_retry` command. UI shows a HealthBanner when `ok = false`.
//! A required slot (toggle, cancel) that cannot be compiled fails the
//! install. An optional slot that cannot be is left out and named in
//! `skipped`, with `ok` still true: dictation keeps working.
//!
//! Capture surface: Settings flips [`set_capture_active`] true while the
//! user clicks "press keys…". The next eligible event is delivered via
//! the `hotkey_captured` Tauri event (payload `{ target, config }`) and
//! capture mode auto-disables.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use openwhisper_core::dictation;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::settings::{HotkeyConfig, HotkeySettings, HotkeyTarget};

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Serialize, Clone, Debug)]
pub struct HotkeyStatus {
    pub ok: bool,
    pub error: String,
    /// Optional slots that have a binding but were not installed, and
    /// why. Left out of the payload when there are none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<SkippedSlot>,
}

/// An optional slot the backend left out. `target` is
/// [`HotkeyTarget::as_str`]; `reason` is a sentence for the user.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct SkippedSlot {
    pub target: &'static str,
    pub reason: String,
}

/// One hotkey slot: which binding it reads and what it does.
pub(crate) struct SlotSpec {
    pub target: HotkeyTarget,
    /// When set, the slot fires only while this returns true; while
    /// it returns false the key passes through to the focused app.
    /// Called on the hook / tap thread for every matching key event,
    /// so it has to be lock-free.
    pub gate: Option<fn() -> bool>,
    /// Called on the hook / tap thread once per press. Must return at
    /// once: anything slow goes on its own thread.
    pub action: fn(),
}

/// Every slot, in match order: when two slots would match the same
/// key event, the earlier one wins. Whether a slot may be unbound is
/// [`HotkeyTarget::is_optional`]; an unbound slot installs nothing.
pub(crate) const SLOTS: &[SlotSpec] = &[
    SlotSpec { target: HotkeyTarget::Toggle, gate: None, action: fire_toggle },
    // Escape is swallowed only while it would do something: a
    // recording, or an ask that is being transcribed or answered.
    SlotSpec {
        target: HotkeyTarget::Cancel,
        gate: Some(dictation::cancel_would_act),
        action: fire_cancel,
    },
    SlotSpec { target: HotkeyTarget::CycleMode, gate: None, action: fire_cycle_mode },
    SlotSpec { target: HotkeyTarget::Ask, gate: None, action: fire_ask },
];

impl SlotSpec {
    /// The slot may fire now. See [`SlotSpec::gate`].
    pub(crate) fn gate_passes(&self) -> bool {
        self.gate.is_none_or(|open| open())
    }
}

/// Run a start/stop toggle the way each backend always has: the
/// Windows hook hands it to a thread (the hook callback has to return
/// within the system's timeout), the mac tap runs it in place.
fn run_toggle(run: impl FnOnce() + Send + 'static) {
    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(run);
    }
    #[cfg(not(target_os = "windows"))]
    run();
}

fn fire_toggle() {
    run_toggle(|| {
        if let Err(e) = crate::do_toggle() {
            eprintln!("hotkey toggle failed: {e}");
        }
    });
}

fn fire_ask() {
    run_toggle(|| {
        if let Err(e) = crate::do_ask_toggle() {
            eprintln!("hotkey ask failed: {e}");
        }
    });
}

fn fire_cancel() {
    std::thread::spawn(|| {
        let _ = crate::do_cancel();
    });
}

fn fire_cycle_mode() {
    std::thread::spawn(|| {
        if let Err(e) = crate::do_cycle_mode() {
            eprintln!("hotkey cycle-mode failed: {e}");
        }
    });
}

/// The binding stored for a slot, `None` when an optional slot is
/// unbound.
fn slot_config(settings: &HotkeySettings, target: HotkeyTarget) -> Option<&HotkeyConfig> {
    match target {
        HotkeyTarget::Toggle => Some(&settings.toggle),
        HotkeyTarget::Cancel => Some(&settings.cancel),
        HotkeyTarget::CycleMode => settings.cycle_mode.as_ref(),
        HotkeyTarget::Ask => settings.ask.as_ref(),
    }
}

/// The bindings to install, given whether Ask is switched on. With
/// Ask off its slot is left out, so the key stays with the focused
/// app and nothing can start an ask; the saved binding is kept for
/// when Ask is switched on again.
pub(crate) fn effective_settings(mut settings: HotkeySettings, ask_enabled: bool) -> HotkeySettings {
    if !ask_enabled {
        settings.ask = None;
    }
    settings
}

/// Same key and same modifiers, whatever order the modifiers are in.
fn same_binding(a: &HotkeyConfig, b: &HotkeyConfig) -> bool {
    let sorted = |c: &HotkeyConfig| {
        let mut mods = c.mods.clone();
        mods.sort();
        mods
    };
    a.kind == b.kind && a.code == b.code && sorted(a) == sorted(b)
}

/// Compile every bound slot with the backend's `compile`, which
/// returns the compiled binding or a sentence saying why it cannot be
/// used on this platform.
///
/// Returns the compiled slots as `(index into SLOTS, compiled)` in
/// table order, plus the optional slots that were left out. A required
/// slot that does not compile is an `Err`: the install fails as a
/// whole, as it always has. An optional slot is left out when it does
/// not compile, or when it repeats the keys of a slot before it (the
/// earlier slot would win every press, and two modifier-taps on one
/// key would both fire).
pub(crate) fn compile_slots<T>(
    settings: &HotkeySettings,
    compile: impl Fn(&HotkeyConfig) -> Result<T, String>,
) -> Result<(Vec<(usize, T)>, Vec<SkippedSlot>), String> {
    let mut compiled: Vec<(usize, T)> = Vec::new();
    let mut skipped = Vec::new();
    for (index, spec) in SLOTS.iter().enumerate() {
        let Some(config) = slot_config(settings, spec.target) else {
            continue;
        };
        let optional = spec.target.is_optional();
        if optional {
            let taken = compiled.iter().find(|(i, _)| {
                slot_config(settings, SLOTS[*i].target).is_some_and(|c| same_binding(c, config))
            });
            if let Some((i, _)) = taken {
                skipped.push(SkippedSlot {
                    target: spec.target.as_str(),
                    reason: format!(
                        "It uses the same keys as the {} shortcut.",
                        SLOTS[*i].target.as_str()
                    ),
                });
                continue;
            }
        }
        match compile(config) {
            Ok(value) => compiled.push((index, value)),
            Err(reason) if optional => {
                skipped.push(SkippedSlot { target: spec.target.as_str(), reason });
            }
            Err(_) => {
                return Err(format!(
                    "Unsupported {} hotkey: kind={:?} code={} mods={:?}",
                    spec.target.as_str(),
                    config.kind,
                    config.code,
                    config.mods
                ));
            }
        }
    }
    Ok((compiled, skipped))
}

#[derive(Serialize, Clone, Debug)]
pub struct HotkeyCapturedPayload {
    pub target: &'static str,
    pub config: HotkeyConfig,
}

pub const HOTKEY_STATUS_EVENT: &str = "hotkey_status";
pub const HOTKEY_CAPTURED_EVENT: &str = "hotkey_captured";

static LAST_STATUS: Mutex<Option<HotkeyStatus>> = Mutex::new(None);

/// Stored at first install — backends emit capture results from worker
/// threads so we cache the handle here rather than threading it through
/// the per-platform tap state.
static APP_HANDLE: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();

static CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);
static CAPTURE_TARGET: Mutex<Option<HotkeyTarget>> = Mutex::new(None);

fn handle_slot() -> &'static Mutex<Option<AppHandle>> {
    APP_HANDLE.get_or_init(|| Mutex::new(None))
}

fn store_handle(app: &AppHandle) {
    if let Ok(mut g) = handle_slot().lock() {
        *g = Some(app.clone());
    }
}

fn cached_handle() -> Option<AppHandle> {
    handle_slot().lock().ok().and_then(|g| g.clone())
}

fn emit_status(app: &AppHandle, result: Result<Vec<SkippedSlot>, String>) {
    let status = match result {
        Ok(skipped) => {
            for slot in &skipped {
                eprintln!("hotkey: {} shortcut not installed: {}", slot.target, slot.reason);
            }
            HotkeyStatus { ok: true, error: String::new(), skipped }
        }
        Err(error) => HotkeyStatus { ok: false, error, skipped: Vec::new() },
    };
    if let Ok(mut last) = LAST_STATUS.lock() {
        *last = Some(status.clone());
    }
    if let Err(e) = app.emit(HOTKEY_STATUS_EVENT, &status) {
        eprintln!("hotkey_status emit failed: {e}");
    }
}

/// Toggle capture-mode on/off. While true, the next eligible hotkey event
/// is captured and delivered via `hotkey_captured` rather than firing the
/// real toggle/cancel. Backend-agnostic — both `mac.rs` and `windows.rs`
/// poll this from their callbacks.
pub fn set_capture_active(active: bool, target: Option<HotkeyTarget>) {
    if active {
        if let (true, Some(t)) = (active, target) {
            if let Ok(mut g) = CAPTURE_TARGET.lock() {
                *g = Some(t);
            }
        }
    } else if let Ok(mut g) = CAPTURE_TARGET.lock() {
        *g = None;
    }
    CAPTURE_ACTIVE.store(active, Ordering::Relaxed);
}

pub fn is_capture_active() -> bool {
    CAPTURE_ACTIVE.load(Ordering::Relaxed)
}

fn current_capture_target() -> Option<HotkeyTarget> {
    CAPTURE_TARGET.lock().ok().and_then(|g| *g)
}

/// Emit a captured descriptor to the front-end and exit capture mode.
/// Called by the per-platform backends from the hook/tap thread.
pub(crate) fn deliver_capture(config: HotkeyConfig) {
    let target = current_capture_target().unwrap_or(HotkeyTarget::Toggle);
    CAPTURE_ACTIVE.store(false, Ordering::Relaxed);
    if let Ok(mut g) = CAPTURE_TARGET.lock() {
        *g = None;
    }
    let Some(app) = cached_handle() else {
        return;
    };
    let payload = HotkeyCapturedPayload {
        target: target.as_str(),
        config,
    };
    if let Err(e) = app.emit(HOTKEY_CAPTURED_EVENT, &payload) {
        eprintln!("hotkey_captured emit failed: {e}");
    }
}

/// Toggle the global hotkey on/off. Used by the fullscreen-aware path.
pub fn set_active(app: &AppHandle, active: bool) {
    if active {
        install(app);
    } else {
        #[cfg(target_os = "macos")]
        mac::teardown();
        #[cfg(target_os = "windows")]
        windows::teardown(app);
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let _ = app;
    }
}

/// Install the platform hook for every bound slot. Idempotent: calling
/// twice tears down and reinstalls. Used at boot, from `hotkey_retry`,
/// after any hotkey save, reset or clear, and after Ask is switched
/// on or off (`ask_set`).
pub fn install(app: &AppHandle) {
    store_handle(app);
    let settings = effective_settings(
        crate::settings::current_settings().unwrap_or_else(crate::settings::default_settings),
        openwhisper_core::settings::current_ask_settings().enabled,
    );
    #[cfg(target_os = "macos")]
    emit_status(app, mac::install(app, &settings));
    #[cfg(target_os = "windows")]
    emit_status(app, windows::install(app, &settings));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = settings;
        emit_status(app, Err("hotkey: platform not supported in MVP".into()));
    }
}

#[tauri::command]
pub fn hotkey_retry(app: AppHandle) {
    #[cfg(target_os = "macos")]
    {
        // Tauri 2's app.restart() re-execs current_exe() directly. On Mac
        // that skips LaunchServices/launchctl registration, so the process
        // exits but the new instance silently fails to start — especially
        // in dev builds where ad-hoc signing + TCC handoff is fragile.
        // Canonical Mac relaunch is `open -n -a <bundle>`, which goes
        // through LaunchServices and registers with launchctl cleanly.
        // Walk current_exe ancestors to find the enclosing .app bundle.
        if let Ok(exe) = std::env::current_exe() {
            if let Some(bundle) = exe.ancestors().find(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s == "app")
                    .unwrap_or(false)
            }) {
                let _ = std::process::Command::new("open")
                    .args(["-n", "-a"])
                    .arg(bundle)
                    .spawn();
                app.exit(0);
                return;
            }
        }
        // Fallback for the bare-binary case (running target/debug/openwhisper-tauri
        // without a .app wrapper). Tauri's restart() is correct there.
        app.restart();
    }
    #[cfg(target_os = "windows")]
    {
        install(&app);
    }
}

#[tauri::command]
pub fn hotkey_status_current() -> Option<HotkeyStatus> {
    LAST_STATUS.lock().ok().and_then(|g| g.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::default_settings;

    /// A stand-in backend: chords compile to their key name, a
    /// modifier-tap does not compile at all.
    fn chords_only(config: &HotkeyConfig) -> Result<String, String> {
        match config.kind {
            crate::settings::HotkeyKind::Chord => Ok(config.code.clone()),
            crate::settings::HotkeyKind::ModifierTap => Err("Chords only.".into()),
        }
    }

    fn chord_settings() -> HotkeySettings {
        HotkeySettings {
            toggle: HotkeyConfig::chord("Space", &["Ctrl"]),
            cancel: HotkeyConfig::chord("Escape", &[]),
            cycle_mode: None,
            ask: None,
        }
    }

    fn targets<T>(compiled: &[(usize, T)]) -> Vec<&'static str> {
        compiled.iter().map(|(i, _)| SLOTS[*i].target.as_str()).collect()
    }

    #[test]
    fn the_table_has_one_entry_per_target_and_only_cancel_is_gated() {
        let listed: Vec<&str> = SLOTS.iter().map(|s| s.target.as_str()).collect();
        assert_eq!(listed, ["toggle", "cancel", "cycle-mode", "ask"]);
        let gated: Vec<&str> =
            SLOTS.iter().filter(|s| s.gate.is_some()).map(|s| s.target.as_str()).collect();
        assert_eq!(gated, ["cancel"]);
    }

    #[test]
    fn the_cancel_gate_is_closed_while_nothing_records_and_no_ask_runs() {
        // Nothing records and no ask runs in this test binary, so
        // Escape is left to the focused app; the ungated slots pass.
        let passes: Vec<(&str, bool)> =
            SLOTS.iter().map(|s| (s.target.as_str(), s.gate_passes())).collect();
        assert_eq!(
            passes,
            [("toggle", true), ("cancel", false), ("cycle-mode", true), ("ask", true)]
        );
        // The gate is core's: a recording, or an ask that is running.
        let cancel = &SLOTS[1];
        assert_eq!(cancel.gate.map(|g| g()), Some(dictation::cancel_would_act()));
    }

    #[test]
    fn the_ask_slot_is_installed_only_while_ask_is_on_and_bound() {
        let ask = HotkeyConfig::chord("Space", &["Ctrl", "Shift"]);
        let bound = HotkeySettings { ask: Some(ask.clone()), ..chord_settings() };

        // Ask off: the binding is kept in the file, not installed.
        let off = effective_settings(bound.clone(), false);
        assert_eq!(off.ask, None);
        let (compiled, skipped) = compile_slots(&off, chords_only).unwrap();
        assert_eq!(targets(&compiled), ["toggle", "cancel"]);
        assert!(skipped.is_empty());

        // Ask on: installed after the others.
        let on = effective_settings(bound.clone(), true);
        assert_eq!(on, bound);
        let (compiled, skipped) = compile_slots(&on, chords_only).unwrap();
        assert_eq!(targets(&compiled), ["toggle", "cancel", "ask"]);
        assert_eq!(compiled[2].1, "Space");
        assert!(skipped.is_empty());

        // Ask on, slot cleared: nothing to install, nothing to report.
        let cleared = effective_settings(chord_settings(), true);
        let (compiled, skipped) = compile_slots(&cleared, chords_only).unwrap();
        assert_eq!(targets(&compiled), ["toggle", "cancel"]);
        assert!(skipped.is_empty());

        // Only the ask slot is touched.
        assert_eq!(off.toggle, bound.toggle);
        assert_eq!(off.cancel, bound.cancel);
        assert_eq!(off.cycle_mode, bound.cycle_mode);
    }

    #[test]
    fn an_ask_binding_the_backend_cannot_install_is_skipped_not_fatal() {
        // The mac default (a modifier tap) on a chords-only backend,
        // as on Windows with a settings file brought over from a Mac.
        let settings = HotkeySettings {
            ask: Some(HotkeyConfig::modifier_tap("RightOption")),
            ..chord_settings()
        };
        let (compiled, skipped) = compile_slots(&settings, chords_only).unwrap();
        assert_eq!(targets(&compiled), ["toggle", "cancel"]);
        assert_eq!(skipped, [SkippedSlot { target: "ask", reason: "Chords only.".into() }]);

        // On the keys of the switch-mode slot: the earlier slot keeps them.
        let settings = HotkeySettings {
            cycle_mode: Some(HotkeyConfig::chord("M", &["Ctrl", "Alt"])),
            ask: Some(HotkeyConfig::chord("M", &["Alt", "Ctrl"])),
            ..chord_settings()
        };
        let (compiled, skipped) = compile_slots(&settings, chords_only).unwrap();
        assert_eq!(targets(&compiled), ["toggle", "cancel", "cycle-mode"]);
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].target, "ask");
        assert!(skipped[0].reason.contains("cycle-mode"), "{}", skipped[0].reason);
    }

    #[test]
    fn the_platform_default_ask_key_installs_next_to_the_other_defaults() {
        // The defaults as shipped, Ask on, on a backend that takes
        // every kind of binding (the mac tap).
        let settings = effective_settings(default_settings(), true);
        let (compiled, skipped) =
            compile_slots(&settings, |c| Ok::<_, String>(c.code.clone())).unwrap();
        assert_eq!(targets(&compiled), ["toggle", "cancel", "ask"]);
        assert!(skipped.is_empty(), "{skipped:?}");
    }

    #[test]
    fn an_unbound_optional_slot_installs_nothing_and_reports_nothing() {
        let (compiled, skipped) = compile_slots(&chord_settings(), chords_only).unwrap();
        assert_eq!(targets(&compiled), ["toggle", "cancel"]);
        assert!(skipped.is_empty());

        // The platform defaults compile on a backend that takes
        // everything. Ask is off by default, so its key is not there.
        let (compiled, skipped) = compile_slots(&effective_settings(default_settings(), false), |c| {
            Ok::<_, String>(c.code.clone())
        })
        .unwrap();
        assert_eq!(targets(&compiled), ["toggle", "cancel"]);
        assert!(skipped.is_empty());
    }

    #[test]
    fn a_bound_optional_slot_is_compiled_after_the_required_ones() {
        let settings = HotkeySettings {
            cycle_mode: Some(HotkeyConfig::chord("M", &["Ctrl", "Alt"])),
            ..chord_settings()
        };
        let (compiled, skipped) = compile_slots(&settings, chords_only).unwrap();
        assert_eq!(targets(&compiled), ["toggle", "cancel", "cycle-mode"]);
        assert_eq!(compiled[2].1, "M");
        assert!(skipped.is_empty());
    }

    #[test]
    fn a_required_slot_that_does_not_compile_fails_the_install() {
        let settings = HotkeySettings {
            toggle: HotkeyConfig::modifier_tap("RightCommand"),
            cycle_mode: Some(HotkeyConfig::chord("M", &["Ctrl"])),
            ..chord_settings()
        };
        let err = compile_slots(&settings, chords_only).unwrap_err();
        assert_eq!(err, "Unsupported toggle hotkey: kind=ModifierTap code=RightCommand mods=[]");
    }

    #[test]
    fn an_optional_slot_that_does_not_compile_is_skipped_with_the_reason() {
        let settings = HotkeySettings {
            cycle_mode: Some(HotkeyConfig::modifier_tap("RightOption")),
            ..chord_settings()
        };
        let (compiled, skipped) = compile_slots(&settings, chords_only).unwrap();
        assert_eq!(targets(&compiled), ["toggle", "cancel"]);
        assert_eq!(
            skipped,
            [SkippedSlot { target: "cycle-mode", reason: "Chords only.".into() }]
        );
    }

    #[test]
    fn an_optional_slot_on_the_keys_of_another_slot_is_skipped() {
        // Same chord as toggle, modifiers in another order.
        let settings = HotkeySettings {
            toggle: HotkeyConfig::chord("Space", &["Ctrl", "Shift"]),
            cycle_mode: Some(HotkeyConfig::chord("Space", &["Shift", "Ctrl"])),
            ..chord_settings()
        };
        let (compiled, skipped) = compile_slots(&settings, chords_only).unwrap();
        assert_eq!(targets(&compiled), ["toggle", "cancel"]);
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].target, "cycle-mode");
        assert!(skipped[0].reason.contains("toggle"), "{}", skipped[0].reason);

        // The cancel key too: outside a recording the cycle slot would
        // otherwise take Escape away from every app.
        let settings = HotkeySettings {
            cycle_mode: Some(HotkeyConfig::chord("Escape", &[])),
            ..chord_settings()
        };
        let (_, skipped) = compile_slots(&settings, chords_only).unwrap();
        assert!(skipped[0].reason.contains("cancel"), "{}", skipped[0].reason);
    }

    #[test]
    fn the_status_payload_only_grows_when_a_slot_was_skipped() {
        let clean = HotkeyStatus { ok: true, error: String::new(), skipped: Vec::new() };
        assert_eq!(
            serde_json::to_value(&clean).unwrap(),
            serde_json::json!({ "ok": true, "error": "" })
        );
        let with_skip = HotkeyStatus {
            ok: true,
            error: String::new(),
            skipped: vec![SkippedSlot { target: "cycle-mode", reason: "Chords only.".into() }],
        };
        assert_eq!(
            serde_json::to_value(&with_skip).unwrap(),
            serde_json::json!({
                "ok": true,
                "error": "",
                "skipped": [{ "target": "cycle-mode", "reason": "Chords only." }]
            })
        );
    }
}
