---
id: TASK-69
title: >-
  Mac: hotkey_retry uses 'open -n' instead of app.restart() to keep launchctl
  registration
status: In Progress
assignee: []
created_date: '2026-05-01 18:33'
updated_date: '2026-05-01 18:33'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Tauri's app.restart() re-execs current_exe() directly. On Mac that bypasses LaunchServices/launchctl registration, so the process exits but the new instance silently fails to start (especially in dev builds where signing/TCC is fragile). Switch the Mac branch of hotkey_retry to spawn 'open -n /path/to/.app' then app.exit(0). Release builds get the same fix — open -n is the canonical Mac relaunch idiom.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Mac branch of hotkey_retry walks current_exe() ancestors to find the .app bundle and spawns 'open -n -a <bundle>' before app.exit(0).
- [ ] #2 Release build smoke: clicking Restart in the hotkey banner relaunches the app cleanly (one process, banner cleared after relaunch).
- [ ] #3 Dev build smoke: clicking Restart in the hotkey banner relaunches the app cleanly without dev-run.sh re-trigger.
- [ ] #4 Falls back to app.restart() if the .app ancestor walk fails (e.g. binary run directly without bundle).
<!-- AC:END -->
