//! TASK-48 — clear stale TCC entries when the binary's cdhash changes.
//!
//! Why: ad-hoc-signed apps are identified to TCC by **cdhash**, and every
//! rebuild — even of the same `CFBundleShortVersionString` — produces a
//! new cdhash. The bundle id stays stable, so System Settings keeps
//! showing the old "OpenWhisper" toggle in the Accessibility / Privacy
//! panes, but the TCC database treats the new binary as a different
//! identity and re-prompts the user. Without this reset the user has to
//! manually delete the old entry from System Settings before the fresh
//! prompt can land. Paid Developer ID would anchor TCC to the Team
//! Identifier and obviate this — until then, this module is the interim.
//!
//! Cachebuster: cdhash, not version. An earlier iteration keyed off
//! `CFBundleShortVersionString` and missed the within-version-rebuild
//! case (e.g. two 0.4.0 builds during release prep both wrote "0.4.0" to
//! the marker → reset never fired on the second install). cdhash is what
//! TCC itself keys on, so it captures any case TCC would treat as a
//! "new" identity.
//!
//! Behavior on boot: read own cdhash via `codesign -d --verbose=4`,
//! compare to the `tcc-last-cdhash` marker file. If absent or different,
//! shell out to `tccutil reset <Service> <bundle-id>` for Accessibility,
//! Microphone, and ListenEvent (the 11.0+ keyboard-monitor service
//! CGEventTap relies on), then persist the current cdhash. tccutil exit
//! code is intentionally ignored — "no entries to reset" is exit 1 on a
//! clean install and is the desired no-op.
//!
//! Verbosity matters: `CDHash=` is only emitted at `-dvvvv` (verbosity 4)
//! and above. `-dvv` shows `CodeDirectory` but not `CDHash`, so a too-low
//! verbose flag would make `current_cdhash()` silently return None and
//! skip the reset cycle on every boot.
//!
//! Marker-file-absent is treated as a reset trigger, not a quiet first
//! run: on a brand-new install no TCC entries exist so the reset is a
//! no-op, and on any upgrade-from-an-older-build the marker is absent
//! exactly because the older build didn't write the new format. Cost is
//! zero either way. Old `tcc-last-version` files left by the previous
//! iteration are harmless; they just stop being read.
//!
//! If `codesign -dvv` fails (binary not signed at all, codesign tool
//! missing, sandboxed dev path), skip the whole reset cycle silently —
//! we'd rather miss a reset than crash boot or fire reset on a partial
//! identity read.

#[cfg(target_os = "macos")]
use std::fs;
#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(target_os = "macos")]
use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
const MARKER_FILE: &str = "tcc-last-cdhash";

#[cfg(target_os = "macos")]
const TCC_SERVICES: &[&str] = &["Accessibility", "Microphone", "ListenEvent"];

#[cfg(target_os = "macos")]
fn current_cdhash() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe_str = exe.to_str()?;
    let output = Command::new("codesign")
        .args(["-d", "--verbose=4", exe_str])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    // codesign -dvv writes to stderr.
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stderr.lines() {
        if let Some(rest) = line.strip_prefix("CDHash=") {
            let trimmed = rest.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
pub fn reset_if_version_changed(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }

    let Some(current) = current_cdhash() else {
        eprintln!("[version_reset] could not read own cdhash, skipping reset cycle");
        return;
    };
    eprintln!("[version_reset] current cdhash = {current}");

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
            "[version_reset] reset TCC for {bundle_id} on cdhash change ({} → {current})",
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
