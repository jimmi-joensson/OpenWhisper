//! macOS hotkey via CGEventTap. Stub — implementation lands in the next
//! commit (Phase 4 commit 4). Returns Ok so the boot path doesn't surface
//! a misleading "hotkey unavailable" banner during the windows-first half
//! of the rollout.

use tauri::AppHandle;

pub fn install(_app: &AppHandle) -> Result<(), String> {
    // TODO(commit 4): port HotkeyService.swift CGEventTap + watchdog.
    Ok(())
}
