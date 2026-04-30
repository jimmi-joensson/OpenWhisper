//! TASK-48 — clear stale TCC entries when the bundle version changes.
//!
//! Why: ad-hoc-signed apps are identified to TCC by cdhash, and every
//! release rebuild produces a new cdhash. The bundle id stays stable, so
//! System Settings keeps showing the old "OpenWhisper" toggle in the
//! Accessibility / Privacy panes, but the TCC database treats the new
//! binary as a different identity and re-prompts the user. Without this
//! reset the user has to manually delete the old entry from System
//! Settings before the fresh prompt can land. Paid Developer ID would
//! anchor TCC to the Team Identifier and obviate this — until then, this
//! module is the interim.
//!
//! Behavior on first launch of a new version (or a version we haven't
//! seen before): shell out to `tccutil reset <Service> <bundle-id>` for
//! Accessibility, Microphone, and ListenEvent (the 11.0+ keyboard-monitor
//! service CGEventTap relies on), then persist the current version to a
//! marker file. tccutil exit code is intentionally ignored — "no entries
//! to reset" is exit 1 on a clean install and is the desired no-op.
//!
//! Marker-file-absent is treated as a reset trigger, NOT a quiet first
//! run. Reasoning: on a brand-new install no TCC entries exist, so the
//! reset is a no-op anyway; on a 0.3.0 → 0.4.0 upgrade the marker is
//! absent because 0.3.0 never wrote it, and that's exactly the case we
//! need to handle. Cost is zero either way.

#[cfg(target_os = "macos")]
use std::fs;
#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(target_os = "macos")]
use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
const MARKER_FILE: &str = "tcc-last-version";

#[cfg(target_os = "macos")]
const TCC_SERVICES: &[&str] = &["Accessibility", "Microphone", "ListenEvent"];

#[cfg(target_os = "macos")]
pub fn reset_if_version_changed(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }

    let current = app.config().version.clone().unwrap_or_default();
    if current.is_empty() {
        return;
    }

    let bundle_id = app.config().identifier.clone();
    if bundle_id.is_empty() {
        return;
    }

    let Ok(dir) = app.path().app_config_dir() else {
        return;
    };
    let _ = fs::create_dir_all(&dir);
    let marker = dir.join(MARKER_FILE);

    let prior = fs::read_to_string(&marker).ok();
    let prior_trimmed = prior.as_deref().map(str::trim).unwrap_or("");

    if prior_trimmed == current {
        return;
    }

    for service in TCC_SERVICES {
        let _ = Command::new("tccutil")
            .args(["reset", service, &bundle_id])
            .status();
    }

    if let Err(e) = fs::write(&marker, &current) {
        eprintln!("[version_reset] failed to write marker: {e}");
    } else {
        eprintln!(
            "[version_reset] reset TCC for {bundle_id} on version change ({} → {current})",
            if prior_trimmed.is_empty() {
                "<none>"
            } else {
                prior_trimmed
            }
        );
    }
}

#[cfg(not(target_os = "macos"))]
pub fn reset_if_version_changed(_app: &tauri::AppHandle) {}
