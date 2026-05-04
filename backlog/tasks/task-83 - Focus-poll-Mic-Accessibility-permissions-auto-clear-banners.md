---
id: TASK-83
title: Focus-poll Mic + Accessibility permissions, auto-clear banners
status: To Do
assignee: []
created_date: '2026-05-04 19:25'
updated_date: '2026-05-04 19:25'
labels: []
dependencies: []
priority: medium
ordinal: 36000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Companion to TASK-82. That task added a window-focus-event handler that re-probes Automation TCC + Mic AVCaptureDevice authorization status and re-emits the corresponding events, so the Automation and (already-detected) Mic-denied banners clear without an app relaunch when the user grants in System Settings.

Open question for this task: can the Accessibility "Restart" CTA on `hotkey-banner` be retired the same way?

The current `focus.rs::install_ax_watcher` already polls `AXIsProcessTrusted()` every 1.5s and brings main-window forward on the false→true edge, but it does NOT auto-restart the app — the comment explicitly says TCC's kernel cache requires a relaunch before `CGEventTapCreate` succeeds, so we leave the Restart click to the user. **Verify whether this is still true on macOS 14+**: if the kernel cache invalidates on an out-of-process AX grant in current OS versions, we can call `hotkey::install` again on the AX-watcher's edge and clear the banner without relaunch.

If the kernel-cache constraint still holds: leave the Restart CTA, but rephrase the banner to make clear that it's an OS limitation (not an OW choice) and that Mic + Automation work *without* relaunch.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] Determine empirically (post-15.4): does `CGEventTapCreate` succeed after an out-of-process AX grant without an app relaunch? Check on a fresh user with TCC reset.
- [ ] If yes: AX watcher re-attempts `hotkey::install` on the false→true edge; banner clears automatically when the install succeeds.
- [ ] If no: rephrase the hotkey-banner copy so the user understands the relaunch is mandatory due to OS kernel cache, not OW's preference.
- [ ] Mic-denied banner copy already lost the "reopen {app_name}" line in TASK-82 — confirm focus-poll auto-clears it cleanly across grant flips in System Settings.
- [ ] No regressions: AX revoke (true→false) still surfaces the banner via the existing watcher.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
TASK-82 (commit 305675a + 5c11ce8 + 60b4c9e + 53823b7) shipped the focus-event scaffolding in `lib.rs`, plus `permissions::recheck` and `media_control::probe_authorization`. This task only needs to investigate the AX side and decide whether to extend or rephrase.
<!-- SECTION:NOTES:END -->
