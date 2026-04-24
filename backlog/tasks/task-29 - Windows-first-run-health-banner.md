---
id: TASK-29
title: Windows first-run health banner
status: In Review
updated_date: '2026-04-24 21:45'
assignee: []
created_date: '2026-04-24 18:45'
labels:
  - windows
  - ui
  - onboarding
dependencies:
  - TASK-23
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Windows analog of Mac's post-permissions restart banner (`ContentView.swift` accessibility-granted banner). Windows doesn't require Accessibility the same way Mac does, but the user still needs reassurance that the hotkey hook installed, the mic was granted, and the Rust core loaded. Show a styled banner in the main window if any of those health checks fail on launch, with a concrete next action (e.g. "Open Settings → Privacy → Microphone" deep link, or "Restart the app" if hook install failed).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 On launch, app runs three health probes: hotkey hook installed, mic permission available, Rust core model loaded (or downloading)
- [ ] #2 If hotkey hook install failed: banner "Hotkey not active — restart the app" with Restart button
- [ ] #3 If mic permission is denied: banner "Microphone blocked" with a button opening `ms-settings:privacy-microphone`
- [ ] #4 If model is still downloading: existing `ModelLoadBar` behavior (not a banner)
- [ ] #5 Banner styled with shared tokens (TASK-23) — blue accent background, icon, title, body, action button — visually familiar to Mac users
- [ ] #6 Banner auto-dismisses once the underlying issue is resolved (re-polls every 2 s while open)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. New `HealthBanner.xaml` UserControl styled to match Mac's `ContentView.swift:restartBanner` visual: colored background, leading icon, title + body, trailing action button. 2. `HealthMonitor` service polling hook/mic/core at 2 s intervals while main window is open. 3. Mic check via `Windows.Media.Capture.MediaCapture` probe or Privacy API. 4. Wire deep-link buttons via `Launcher.LaunchUriAsync("ms-settings:privacy-microphone")`. 5. Hide banner when all three probes pass.
<!-- SECTION:PLAN:END -->
